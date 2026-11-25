//! OpenCode: assistant messages live in `~/.local/share/opencode/opencode.db` (SQLite),
//! each with a JSON `data` column holding tokens and cost.

use super::{AgentId, Event, Source, Usage};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct OpenCode {
    db: PathBuf,
}

impl OpenCode {
    pub fn new() -> Self {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| super::home().join(".local/share"));
        Self {
            db: data.join("opencode/opencode.db"),
        }
    }
}

impl Source for OpenCode {
    fn id(&self) -> AgentId {
        AgentId::OpenCode
    }

    fn installed(&self) -> bool {
        self.db.is_file()
    }

    fn scan(&mut self, since: SystemTime) -> Usage {
        match read(&self.db, super::unix(since) * 1000) {
            Ok(events) => Usage {
                events,
                limits: Vec::new(),
                note: None,
            },
            Err(e) => Usage {
                note: Some(format!("db: {e}")),
                ..Usage::default()
            },
        }
    }
}

fn read(db: &PathBuf, since_ms: i64) -> rusqlite::Result<Vec<Event>> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    let mut stmt = conn.prepare(
        "SELECT time_created, data FROM message WHERE time_created >= ?1 AND data LIKE '%\"tokens\"%'",
    )?;
    let rows = stmt.query_map([since_ms], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(rows
        .flatten()
        .filter_map(|(ms, data)| parse(ms, &data))
        .collect())
}

fn parse(created_ms: i64, data: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(data).ok()?;
    let t = v.get("tokens")?;
    let n = |p: &str| t.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    let tokens =
        n("/input") + n("/output") + n("/reasoning") + n("/cache/read") + n("/cache/write");
    let cost = v.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
    (tokens > 0).then_some(Event {
        ts: created_ms / 1000,
        tokens,
        cost,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn sums_token_fields() {
        let data = r#"{"role":"assistant","cost":0.02,"tokens":{"input":10,"output":5,"reasoning":1,"cache":{"read":100,"write":4}}}"#;
        let e = super::parse(1_000_000, data).unwrap();
        assert_eq!(e.tokens, 120);
        assert_eq!(e.ts, 1000);
    }
}
