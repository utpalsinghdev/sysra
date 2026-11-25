//! Map listening TCP ports to the processes that own them (Linux only).
//! Scanning every fd is not free, so results are cached for a few seconds.

use std::collections::HashMap;
use std::time::{Duration, Instant};

const RESCAN: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct PortScanner {
    at: Option<Instant>,
    by_pid: HashMap<u32, Vec<u16>>,
}

impl PortScanner {
    pub fn scan(&mut self, pids: &[u32]) -> &HashMap<u32, Vec<u16>> {
        if self.at.is_none_or(|t| t.elapsed() >= RESCAN) {
            self.at = Some(Instant::now());
            self.by_pid = scan(pids);
        }
        &self.by_pid
    }
}

#[cfg(target_os = "linux")]
fn scan(pids: &[u32]) -> HashMap<u32, Vec<u16>> {
    let mut listeners: HashMap<u64, u16> = HashMap::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            // f[1] = local "ADDR:PORT" (hex), f[3] = state (0A = LISTEN), f[9] = inode.
            if f.len() < 10 || f[3] != "0A" {
                continue;
            }
            let port = f[1]
                .rsplit(':')
                .next()
                .and_then(|p| u16::from_str_radix(p, 16).ok());
            if let (Some(port), Ok(inode)) = (port, f[9].parse::<u64>()) {
                listeners.insert(inode, port);
            }
        }
    }
    let mut out = HashMap::new();
    if listeners.is_empty() {
        return out;
    }
    for &pid in pids {
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        let mut ports: Vec<u16> = fds
            .flatten()
            .filter_map(|fd| std::fs::read_link(fd.path()).ok())
            .filter_map(|target| {
                let t = target.to_str()?;
                let inode = t
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse()
                    .ok()?;
                listeners.get(&inode).copied()
            })
            .collect();
        if !ports.is_empty() {
            ports.sort_unstable();
            ports.dedup();
            out.insert(pid, ports);
        }
    }
    out
}

#[cfg(not(target_os = "linux"))]
fn scan(_pids: &[u32]) -> HashMap<u32, Vec<u16>> {
    HashMap::new()
}
