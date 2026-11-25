//! Whole-disk read/write throughput from `/proc/diskstats` (Linux).
//! Other platforms return `None` and the caller sums per-process IO instead.

#[derive(Default)]
pub struct DiskIo {
    last: Option<(u64, u64)>,
}

impl DiskIo {
    /// Bytes/sec read and written since the previous call.
    pub fn sample(&mut self, dt: f64) -> Option<(f64, f64)> {
        let now = read_totals()?;
        let prev = self.last.replace(now).unwrap_or(now);
        Some((
            now.0.saturating_sub(prev.0) as f64 / dt,
            now.1.saturating_sub(prev.1) as f64 / dt,
        ))
    }
}

#[cfg(target_os = "linux")]
fn read_totals() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/diskstats").ok()?;
    let mut totals = (0u64, 0u64);
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 || !is_physical_disk(f[2]) {
            continue;
        }
        // Fields 5 and 9 are sectors read/written; a sector here is always 512 bytes.
        totals.0 += f[5].parse::<u64>().unwrap_or(0) * 512;
        totals.1 += f[9].parse::<u64>().unwrap_or(0) * 512;
    }
    Some(totals)
}

/// Whole devices appear in /sys/block; partitions, loop and zram devices are skipped.
#[cfg(target_os = "linux")]
fn is_physical_disk(name: &str) -> bool {
    !name.starts_with("loop")
        && !name.starts_with("ram")
        && !name.starts_with("zram")
        && std::path::Path::new("/sys/block").join(name).exists()
}

#[cfg(not(target_os = "linux"))]
fn read_totals() -> Option<(u64, u64)> {
    None
}
