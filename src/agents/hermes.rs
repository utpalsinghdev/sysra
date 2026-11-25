//! Hermes agent: per-session token totals in `~/.hermes/state.db` (SQLite). Hermes
//! doesn't keep per-call timestamps, so a session counts at its last activity.

use super::{AgentId, Event, Source, Usage};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::SystemTime;

pub struct Hermes {
    db: PathBuf,
}

impl Hermes {
    pub fn new() -> Self {
        Self {
            db: super::env_or_home("HERMES_HOME", ".hermes").join("state.db"),
        }
    }
}

impl Source for Hermes {
    fn id(&self) -> AgentId {
        AgentId::Hermes
    }

    fn installed(&self) -> bool {
        self.db.is_file()
    }

    fn scan(&mut self, since: SystemTime) -> Usage {
        match read(&self.db, super::unix(since)) {
            Ok(events) => Usage {
                events,
                limits: Vec::new(),
                note: Some("shares provider limits".into()),
            },
            Err(e) => Usage {
                note: Some(format!("db: {e}")),
                ..Usage::default()
            },
        }
    }
}

fn read(db: &PathBuf, since: i64) -> rusqlite::Result<Vec<Event>> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    let mut stmt = conn.prepare(
        "SELECT CAST(COALESCE(last_activity_at, ended_at, started_at) AS INTEGER) AS ts,
                COALESCE(input_tokens,0) + COALESCE(output_tokens,0)
                  + COALESCE(cache_read_tokens,0) + COALESCE(cache_write_tokens,0)
                  + COALESCE(reasoning_tokens,0),
                COALESCE(actual_cost_usd, estimated_cost_usd, 0.0)
         FROM sessions WHERE COALESCE(last_activity_at, ended_at, started_at) >= ?1",
    )?;
    let rows = stmt.query_map([since], |r| {
        Ok(Event {
            ts: r.get(0)?,
            tokens: r.get::<_, i64>(1)?.max(0) as u64,
            cost: r.get(2)?,
        })
    })?;
    Ok(rows.flatten().filter(|e| e.tokens > 0).collect())
}
