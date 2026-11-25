//! Compact human formatting for sizes, rates, token counts and durations.

/// `7.4G`, `459M`, `12K`, `0B` — fits in 5 columns.
pub fn bytes(n: u64) -> String {
    scaled(n as f64, 1024.0, &["B", "K", "M", "G", "T", "P"])
}

/// `121 KB/s`
pub fn rate(bps: f64) -> String {
    let s = scaled(bps.max(0.0), 1024.0, &[" B", " KB", " MB", " GB", " TB"]);
    format!("{s}/s")
}

/// `12.4M`, `1.2B`, `830`
pub fn tokens(n: u64) -> String {
    scaled(n as f64, 1000.0, &["", "K", "M", "B", "T"])
}

fn scaled(mut v: f64, step: f64, units: &[&str]) -> String {
    let mut i = 0;
    while v >= step * 0.995 && i + 1 < units.len() {
        v /= step;
        i += 1;
    }
    if i == 0 || v >= 100.0 {
        format!("{v:.0}{}", units[i])
    } else {
        format!("{v:.1}{}", units[i])
    }
}

/// `3h 37m`, `2d 4h`, `45s`
pub fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

/// Time until a unix timestamp, compact: `3h12m`, `6d`, `now`.
pub fn until(ts: i64, now: i64) -> String {
    let s = (ts - now).max(0) as u64;
    match s {
        0..60 => "now".into(),
        60..3600 => format!("{}m", s / 60),
        3600..86_400 => format!("{}h{:02}m", s / 3600, s / 60 % 60),
        _ => format!("{}d{}h", s / 86_400, s / 3600 % 24),
    }
}

/// Left-pad/truncate to an exact display width (ASCII-oriented; app names are short).
pub fn fit(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count <= width {
        format!("{s:<width$}")
    } else if width > 1 {
        let cut: String = s.chars().take(width - 1).collect();
        format!("{cut}…")
    } else {
        s.chars().take(width).collect()
    }
}

/// Right-aligned variant of [`fit`].
pub fn rfit(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count <= width {
        format!("{s:>width$}")
    } else {
        fit(s, width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(7_945_000_000), "7.4G");
        assert_eq!(bytes(481_000_000), "459M");
        assert_eq!(tokens(152_273_838), "152M");
        assert_eq!(tokens(1_205_094), "1.2M");
        assert_eq!(rate(123_904.0), "121 KB/s");
        assert_eq!(duration(13_020), "3h 37m");
        assert_eq!(until(1000 + 3 * 3600 + 12 * 60, 1000), "3h12m");
        assert_eq!(fit("brave", 3), "br…");
    }
}
