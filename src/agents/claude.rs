//! Claude Code: tokens from `~/.claude/projects/**/*.jsonl`; plan limits from the same
//! endpoint Claude Code's `/usage` uses, authenticated with its existing OAuth login.

use super::{AgentId, Event, Limit, Source, Usage, jsonl::Tail, parse_ts};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The usage endpoint rate-limits aggressively; results are shared between sysra
/// instances through a small cache file.
const FRESH_SECS: i64 = 300;
const BACKOFF_SECS: i64 = 900;

pub struct Claude {
    roots: Vec<PathBuf>,
    tail: Tail<HashSet<String>>,
    live_limits: bool,
    limits: Vec<Limit>,
    note: Option<String>,
    next_fetch: i64,
}

impl Claude {
    pub fn new(live_limits: bool) -> Self {
        let mut roots = Vec::new();
        if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            roots.push(PathBuf::from(dir).join("projects"));
        }
        roots.push(super::home().join(".claude/projects"));
        roots.push(super::home().join(".config/claude/projects"));
        roots.retain(|r| r.is_dir());
        Self {
            roots,
            tail: Tail::new(),
            live_limits,
            limits: Vec::new(),
            note: None,
            next_fetch: 0,
        }
    }

    fn refresh_limits(&mut self) {
        if !self.live_limits {
            self.note = Some("live limits off".into());
            return;
        }
        let now = super::unix(SystemTime::now());
        if now < self.next_fetch {
            return;
        }
        // Another sysra may have fetched recently.
        if let Some((at, body)) = read_cache() {
            self.limits = parse_limits(&body);
            self.note = None;
            if at + FRESH_SECS > now {
                self.next_fetch = at + FRESH_SECS;
                return;
            }
        }
        match fetch_limits() {
            Ok(body) => {
                self.limits = parse_limits(&body);
                self.note = None;
                self.next_fetch = now + FRESH_SECS;
                write_cache(now, &body);
            }
            Err(e) => {
                self.next_fetch = now + BACKOFF_SECS;
                if self.limits.is_empty() {
                    self.note = Some(e);
                }
            }
        }
    }
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| super::home().join(".cache"))
        .join("sysra/claude-limits.json")
}

fn read_cache() -> Option<(i64, Value)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(cache_path()).ok()?).ok()?;
    Some((v.get("fetched_at")?.as_i64()?, v.get("body")?.clone()))
}

fn write_cache(at: i64, body: &Value) {
    let path = cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        path,
        serde_json::json!({ "fetched_at": at, "body": body }).to_string(),
    );
}

impl Source for Claude {
    fn id(&self) -> AgentId {
        AgentId::Claude
    }

    fn installed(&self) -> bool {
        !self.roots.is_empty()
    }

    fn scan(&mut self, since: SystemTime) -> Usage {
        let events = self.tail.scan(&self.roots, since, parse_line);
        self.refresh_limits();
        Usage {
            events,
            limits: self.limits.clone(),
            note: self.note.clone(),
        }
    }
}

/// Claude Code rewrites the same assistant message several times while streaming;
/// `message.id` + `requestId` identifies one billed call.
fn parse_line(line: &str, seen: &mut HashSet<String>) -> Option<Event> {
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let msg = v.get("message")?;
    let usage = msg.get("usage")?;
    let key = format!(
        "{}:{}",
        msg.get("id").and_then(Value::as_str).unwrap_or(""),
        v.get("requestId").and_then(Value::as_str).unwrap_or("")
    );
    if key != ":" && !seen.insert(key) {
        return None;
    }
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    let tokens = n("input_tokens")
        + n("output_tokens")
        + n("cache_creation_input_tokens")
        + n("cache_read_input_tokens");
    let ts = parse_ts(v.get("timestamp")?.as_str()?)?;
    (tokens > 0).then_some(Event {
        ts,
        tokens,
        cost: v.get("costUSD").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

/// The OAuth access token Claude Code stored after `claude login`.
fn access_token() -> Result<String, String> {
    let raw = read_credentials().ok_or("not logged in")?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| "unreadable credentials")?;
    let oauth = v
        .get("claudeAiOauth")
        .ok_or("API-key login (no plan limits)")?;
    let expires = oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX);
    if expires / 1000 < super::unix(SystemTime::now()) {
        return Err("login expired; open claude to refresh".into());
    }
    oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| "no token".into())
}

fn read_credentials() -> Option<String> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| super::home().join(".claude"));
    if let Ok(s) = std::fs::read_to_string(dir.join(".credentials.json")) {
        return Some(s);
    }
    // macOS keeps the login in the Keychain.
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
            .ok()?;
        if out.status.success() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    None
}

fn fetch_limits() -> Result<Value, String> {
    let token = access_token()?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .new_agent();
    let body = agent
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", concat!("sysra/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(429) => "limits rate-limited; retrying soon".to_string(),
            ureq::Error::StatusCode(401) => "login expired; open claude to refresh".to_string(),
            e => format!("limits unavailable ({e})"),
        })?
        .body_mut()
        .read_to_string()
        .map_err(|_| "limits unreadable".to_string())?;
    serde_json::from_str(&body).map_err(|_| "limits unreadable".to_string())
}

fn parse_limits(v: &Value) -> Vec<Limit> {
    [
        ("five_hour", "5h"),
        ("seven_day", "week"),
        ("seven_day_opus", "opus"),
        ("seven_day_sonnet", "sonnet"),
    ]
    .into_iter()
    .filter_map(|(key, label)| {
        let w = v.get(key)?;
        Some(Limit {
            label: label.into(),
            used_pct: w.get("utilization")?.as_f64()?,
            resets_at: w
                .get("resets_at")
                .and_then(Value::as_str)
                .and_then(parse_ts),
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_streamed_messages() {
        let line = r#"{"type":"assistant","requestId":"r1","timestamp":"2026-01-01T00:00:00Z","message":{"id":"m1","usage":{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":5,"cache_creation_input_tokens":0}}}"#;
        let mut seen = HashSet::new();
        assert_eq!(parse_line(line, &mut seen).map(|e| e.tokens), Some(10));
        assert!(parse_line(line, &mut seen).is_none());
    }

    #[test]
    fn reads_limit_windows() {
        let v: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":6.0,"resets_at":"2026-11-25T23:29:59+00:00"},"seven_day":{"utilization":0.0,"resets_at":null},"seven_day_opus":null}"#,
        )
        .unwrap();
        let l = parse_limits(&v);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].label, "5h");
        assert!(l[0].resets_at.is_some());
    }
}
