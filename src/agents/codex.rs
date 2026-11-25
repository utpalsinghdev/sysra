//! Codex CLI: `token_count` events in `~/.codex/sessions/**/rollout-*.jsonl` carry both
//! cumulative token totals and the account's current rate-limit windows.

use super::{AgentId, Event, Limit, Source, Usage, jsonl::Tail, parse_ts};
use serde_json::Value;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct Codex {
    roots: Vec<PathBuf>,
    tail: Tail<u64>,
    /// Newest rate-limit snapshot seen in any rollout: (timestamp, limits).
    latest: Option<(i64, Vec<Limit>)>,
}

impl Codex {
    pub fn new() -> Self {
        let home = super::env_or_home("CODEX_HOME", ".codex");
        let roots = vec![home.join("sessions"), home.join("archived_sessions")];
        Self {
            roots,
            tail: Tail::new(),
            latest: None,
        }
    }
}

impl Source for Codex {
    fn id(&self) -> AgentId {
        AgentId::Codex
    }

    fn installed(&self) -> bool {
        self.roots[0].is_dir()
    }

    fn scan(&mut self, since: SystemTime) -> Usage {
        let latest = &mut self.latest;
        let events = self.tail.scan(&self.roots, since, |line, prev_total| {
            let (event, limits) = parse_line(line, prev_total)?;
            if let Some(limits) = limits
                && latest.as_ref().is_none_or(|(ts, _)| event.ts >= *ts)
            {
                *latest = Some((event.ts, limits));
            }
            (event.tokens > 0).then_some(event)
        });
        let limits = self
            .latest
            .as_ref()
            .map(|(_, l)| l.clone())
            .unwrap_or_default();
        let note = limits.is_empty().then(|| "no limit data yet".to_string());
        Usage {
            events,
            limits,
            note,
        }
    }
}

/// Codex repeats `token_count` events when only rate limits change, so tokens are the
/// growth of the session's running total rather than each event's `last_token_usage`.
fn parse_line(line: &str, prev_total: &mut u64) -> Option<(Event, Option<Vec<Limit>>)> {
    if !line.contains("\"token_count\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let payload = v.get("payload")?;
    let ts = parse_ts(v.get("timestamp")?.as_str()?)?;
    let total = payload
        .pointer("/info/total_token_usage/total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(*prev_total);
    let tokens = total.saturating_sub(*prev_total);
    *prev_total = total.max(*prev_total);
    let limits = payload
        .get("rate_limits")
        .filter(|r| !r.is_null())
        .map(|r| {
            ["primary", "secondary"]
                .into_iter()
                .filter_map(|k| window(r.get(k)?))
                .collect::<Vec<_>>()
        });
    Some((
        Event {
            ts,
            tokens,
            cost: 0.0,
        },
        limits.filter(|l| !l.is_empty()),
    ))
}

fn window(w: &Value) -> Option<Limit> {
    let minutes = w.get("window_minutes")?.as_u64()?;
    let label = match minutes {
        m if m >= 7 * 24 * 60 - 60 => "week".to_string(),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    };
    Some(Limit {
        label,
        used_pct: w.get("used_percent")?.as_f64()?,
        resets_at: w.get("resets_at").and_then(Value::as_i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_growth_and_reads_limits() {
        let line = |total: u64| {
            format!(
                r#"{{"timestamp":"2026-11-25T14:02:41Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":{total}}}}},"rate_limits":{{"primary":{{"used_percent":8.0,"window_minutes":300,"resets_at":1795632445}},"secondary":{{"used_percent":36.0,"window_minutes":10080,"resets_at":1796126753}}}}}}}}"#
            )
        };
        let mut prev = 0;
        let (e, limits) = parse_line(&line(100), &mut prev).unwrap();
        assert_eq!(e.tokens, 100);
        let limits = limits.unwrap();
        assert_eq!(limits[0].label, "5h");
        assert_eq!(limits[1].label, "week");
        assert_eq!(parse_line(&line(100), &mut prev).unwrap().0.tokens, 0);
        assert_eq!(parse_line(&line(150), &mut prev).unwrap().0.tokens, 50);
    }
}
