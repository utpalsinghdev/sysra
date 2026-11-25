//! Pi coding agent: assistant messages in `~/.pi/agent/sessions/**/*.jsonl` record
//! tokens and the provider-reported cost.

use super::{AgentId, Event, Source, Usage, jsonl::Tail, parse_ts};
use serde_json::Value;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct Pi {
    roots: Vec<PathBuf>,
    tail: Tail<()>,
}

impl Pi {
    pub fn new() -> Self {
        let dir = super::env_or_home("PI_CODING_AGENT_DIR", ".pi/agent");
        Self {
            roots: vec![dir.join("sessions")],
            tail: Tail::new(),
        }
    }
}

impl Source for Pi {
    fn id(&self) -> AgentId {
        AgentId::Pi
    }

    fn installed(&self) -> bool {
        self.roots[0].is_dir()
    }

    fn scan(&mut self, since: SystemTime) -> Usage {
        let events = self
            .tail
            .scan(&self.roots, since, |line, _| parse_line(line));
        // Pi runs on top of provider subscriptions (Claude, ChatGPT, ...), whose limits
        // are shown on those agents' rows.
        Usage {
            events,
            limits: Vec::new(),
            note: Some("shares provider limits".into()),
        }
    }
}

fn parse_line(line: &str) -> Option<Event> {
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let msg = v.get("message")?;
    if msg.get("role")?.as_str()? != "assistant" {
        return None;
    }
    let usage = msg.get("usage")?;
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    let tokens = match n("totalTokens") {
        0 => n("input") + n("output") + n("cacheRead") + n("cacheWrite"),
        t => t,
    };
    let cost = usage
        .pointer("/cost/total")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let ts = parse_ts(v.get("timestamp")?.as_str()?)?;
    (tokens > 0).then_some(Event { ts, tokens, cost })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_assistant_usage() {
        let line = r#"{"type":"message","timestamp":"2026-11-25T18:33:28.566Z","message":{"role":"assistant","usage":{"input":4,"output":232,"cacheRead":0,"cacheWrite":25322,"totalTokens":25558,"cost":{"total":0.131266}}}}"#;
        let e = parse_line(line).unwrap();
        assert_eq!(e.tokens, 25558);
        assert!((e.cost - 0.131266).abs() < 1e-9);
    }
}
