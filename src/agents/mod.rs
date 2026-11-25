//! AI coding agent usage: tokens burned recently and plan limits left, read from each
//! agent's own local logs/databases on a background thread.

mod claude;
mod codex;
mod hermes;
mod jsonl;
mod opencode;
mod pi;

use crate::config::AgentsConfig;
use chrono::{Local, TimeZone};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// One model call (or one session, for sources that only keep totals).
#[derive(Debug, Clone, Copy)]
pub struct Event {
    /// Unix seconds.
    pub ts: i64,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentId {
    Claude,
    Codex,
    Pi,
    OpenCode,
    Hermes,
}

impl AgentId {
    /// Config id, as used in `agents.hide`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
        }
    }

    /// Matches the app row name produced by process grouping.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
            Self::Hermes => "Hermes",
        }
    }
}

/// A plan limit window, e.g. Claude's 5-hour session or Codex's weekly cap.
#[derive(Debug, Clone)]
pub struct Limit {
    pub label: String,
    pub used_pct: f64,
    /// Unix seconds.
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AgentReport {
    pub id: AgentId,
    pub tokens_5h: u64,
    pub tokens_today: u64,
    pub tokens_week: u64,
    pub cost_today: Option<f64>,
    /// Tokens per local day, oldest first, today last.
    pub daily: [u64; 7],
    pub limits: Vec<Limit>,
    /// Why limits are missing, or a data-source problem.
    pub note: Option<String>,
}

/// What a source hands back each scan.
#[derive(Default)]
struct Usage {
    events: Vec<Event>,
    limits: Vec<Limit>,
    note: Option<String>,
}

/// Every source keeps its own incremental state between scans.
trait Source: Send {
    fn id(&self) -> AgentId;
    /// Whether this agent appears installed (its data directory exists).
    fn installed(&self) -> bool;
    fn scan(&mut self, since: SystemTime) -> Usage;
}

pub type Shared = Arc<Mutex<Option<Vec<AgentReport>>>>;

/// Spawn the background scanner. `None` in the shared slot means "first scan pending".
pub fn spawn(cfg: &AgentsConfig) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(None));
    let out = Arc::clone(&shared);
    let every = Duration::from_secs(cfg.refresh_secs.max(5));
    let mut sources: Vec<Box<dyn Source>> = vec![
        Box::new(claude::Claude::new(cfg.claude_live_limits)),
        Box::new(codex::Codex::new()),
        Box::new(pi::Pi::new()),
        Box::new(opencode::OpenCode::new()),
        Box::new(hermes::Hermes::new()),
    ];
    sources.retain(|s| !cfg.hide.iter().any(|h| h.eq_ignore_ascii_case(s.id().id())));
    std::thread::Builder::new()
        .name("sysra-agents".into())
        .spawn(move || {
            loop {
                let since = SystemTime::now() - Duration::from_secs(8 * 86_400);
                let reports = sources
                    .iter_mut()
                    .filter(|s| s.installed())
                    .map(|s| summarize(s.id(), s.scan(since)))
                    .collect();
                if let Ok(mut slot) = out.lock() {
                    *slot = Some(reports);
                }
                std::thread::sleep(every);
            }
        })
        .expect("spawn agent scanner");
    shared
}

fn summarize(id: AgentId, usage: Usage) -> AgentReport {
    let now = Local::now();
    let today_start = Local
        .from_local_datetime(&now.date_naive().and_hms_opt(0, 0, 0).expect("midnight"))
        .earliest()
        .map_or(now.timestamp() - 86_400, |t| t.timestamp());
    let now_ts = now.timestamp();
    let mut r = AgentReport {
        id,
        tokens_5h: 0,
        tokens_today: 0,
        tokens_week: 0,
        cost_today: None,
        daily: [0; 7],
        limits: usage.limits,
        note: usage.note,
    };
    for e in &usage.events {
        if e.ts >= now_ts - 5 * 3600 {
            r.tokens_5h += e.tokens;
        }
        if e.ts >= today_start {
            r.tokens_today += e.tokens;
            if e.cost > 0.0 {
                *r.cost_today.get_or_insert(0.0) += e.cost;
            }
        }
        // Day index counted back from today; DST shifts are close enough for a sparkline.
        let days_ago = if e.ts >= today_start {
            0
        } else {
            (today_start - e.ts - 1) / 86_400 + 1
        };
        if (0..7).contains(&days_ago) {
            r.daily[6 - days_ago as usize] += e.tokens;
            r.tokens_week += e.tokens;
        }
    }
    r
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Env var path override, else `~/<rel>`.
fn env_or_home(var: &str, rel: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(rel))
}

/// RFC 3339 timestamp to unix seconds.
fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.timestamp())
}

fn unix(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_by_window() {
        let now = Local::now().timestamp();
        let events = vec![
            Event {
                ts: now - 60,
                tokens: 10,
                cost: 0.5,
            },
            Event {
                ts: now - 3 * 86_400,
                tokens: 100,
                cost: 0.0,
            },
            Event {
                ts: now - 30 * 86_400,
                tokens: 1000,
                cost: 0.0,
            },
        ];
        let r = summarize(
            AgentId::Pi,
            Usage {
                events,
                ..Usage::default()
            },
        );
        assert_eq!(r.tokens_5h, 10);
        assert_eq!(r.tokens_week, 110);
        assert_eq!(r.daily[6], 10);
        assert_eq!(r.cost_today, Some(0.5));
    }
}
