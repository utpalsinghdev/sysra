//! Incremental reader for append-only JSONL session logs. Each refresh only parses
//! bytes appended since the previous scan, so watching hundreds of MB stays cheap.

use super::Event;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

struct FileState<S> {
    offset: u64,
    events: Vec<Event>,
    state: S,
}

/// `S` is per-file parser state (e.g. Codex's running token total).
pub struct Tail<S> {
    files: HashMap<PathBuf, FileState<S>>,
}

impl<S: Default> Tail<S> {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Parse new lines from every `*.jsonl` under `roots` modified after `since`.
    /// Returns every event currently known from those files.
    pub fn scan(
        &mut self,
        roots: &[PathBuf],
        since: SystemTime,
        mut parse: impl FnMut(&str, &mut S) -> Option<Event>,
    ) -> Vec<Event> {
        let mut live = Vec::new();
        for root in roots {
            walk(root, since, &mut live);
        }
        self.files
            .retain(|path, _| live.iter().any(|(p, _)| p == path));
        for (path, len) in live {
            let entry = self.files.entry(path.clone()).or_insert_with(|| FileState {
                offset: 0,
                events: Vec::new(),
                state: S::default(),
            });
            if len < entry.offset {
                // Rewritten or truncated: start over.
                *entry = FileState {
                    offset: 0,
                    events: Vec::new(),
                    state: S::default(),
                };
            }
            if len > entry.offset {
                let _ = read_from(&path, entry, &mut parse);
            }
        }
        self.files
            .values()
            .flat_map(|f| f.events.iter().copied())
            .collect()
    }
}

fn read_from<S>(
    path: &Path,
    file: &mut FileState<S>,
    parse: &mut impl FnMut(&str, &mut S) -> Option<Event>,
) -> std::io::Result<()> {
    let mut reader = BufReader::with_capacity(1 << 16, File::open(path)?);
    reader.seek(SeekFrom::Start(file.offset))?;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        // Stop at EOF or at a half-written last line; it'll be re-read next scan.
        if n == 0 || !line.ends_with('\n') {
            break;
        }
        file.offset += n as u64;
        if let Some(ev) = parse(&line, &mut file.state) {
            file.events.push(ev);
        }
    }
    Ok(())
}

fn walk(dir: &Path, since: SystemTime, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let path = entry.path();
        if meta.is_dir() {
            walk(&path, since, out);
        } else if path.extension().is_some_and(|e| e == "jsonl")
            && meta.modified().is_ok_and(|m| m >= since)
        {
            out.push((path, meta.len()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_only_appended_complete_lines() {
        let dir = std::env::temp_dir().join(format!("sysra-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.jsonl");
        let mut f = File::create(&path).unwrap();
        write!(f, "1\n2\n3").unwrap(); // "3" is incomplete
        f.flush().unwrap();

        let mut tail: Tail<()> = Tail::new();
        let parse = |l: &str, _: &mut ()| {
            Some(Event {
                ts: 0,
                tokens: l.trim().parse().ok()?,
                cost: 0.0,
            })
        };
        let roots = [dir.clone()];
        let total = |evs: Vec<Event>| evs.iter().map(|e| e.tokens).sum::<u64>();
        assert_eq!(total(tail.scan(&roots, SystemTime::UNIX_EPOCH, parse)), 3);

        writeln!(f, "0\n4").unwrap(); // completes "30", adds "4"
        f.flush().unwrap();
        assert_eq!(total(tail.scan(&roots, SystemTime::UNIX_EPOCH, parse)), 37);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
