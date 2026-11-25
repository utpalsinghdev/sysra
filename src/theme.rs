//! Color themes. Built-in palettes live here; users can override any key from
//! `config.toml` or ship their own palette in `~/.config/sysra/themes/<name>.toml`.

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use std::collections::BTreeMap;

/// Every color the UI paints with. Keys match the names accepted in config files.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub select: Color,
    pub cpu_low: Color,
    pub cpu_mid: Color,
    pub cpu_high: Color,
    pub mem: Color,
    pub cache: Color,
    pub rx: Color,
    pub tx: Color,
    pub disk: Color,
    pub ok: Color,
    pub warn: Color,
    pub crit: Color,
}

/// Keys accepted in `[colors]` tables, in documentation order.
pub const COLOR_KEYS: &[&str] = &[
    "bg", "panel", "border", "text", "dim", "faint", "accent", "select", "cpu_low", "cpu_mid",
    "cpu_high", "mem", "cache", "rx", "tx", "disk", "ok", "warn", "crit",
];

/// Palette source: 19 hex colors in `COLOR_KEYS` order.
type Palette = [&'static str; 19];

const BUILTIN: &[(&str, Palette)] = &[
    (
        "pulse",
        [
            "#0b0e14", "#11151d", "#1d222c", "#d9dee8", "#7b8294", "#3b4252", "#8b9cf7", "#1a2030",
            "#7f8cf0", "#b79af2", "#f27a93", "#8b9cf7", "#4a5578", "#5ad1a6", "#f2708a", "#e6c07b",
            "#5ad1a6", "#e6c07b", "#f2708a",
        ],
    ),
    (
        "tokyo-night",
        [
            "#16161e", "#1a1b26", "#292e42", "#c0caf5", "#787c99", "#3b4261", "#7aa2f7", "#283457",
            "#7aa2f7", "#bb9af7", "#f7768e", "#7aa2f7", "#3d59a1", "#9ece6a", "#f7768e", "#e0af68",
            "#9ece6a", "#e0af68", "#f7768e",
        ],
    ),
    (
        "catppuccin",
        [
            "#11111b", "#181825", "#313244", "#cdd6f4", "#7f849c", "#45475a", "#cba6f7", "#313244",
            "#89b4fa", "#cba6f7", "#f38ba8", "#cba6f7", "#585b70", "#a6e3a1", "#f38ba8", "#f9e2af",
            "#a6e3a1", "#f9e2af", "#f38ba8",
        ],
    ),
    (
        "rose-pine",
        [
            "#191724", "#1f1d2e", "#26233a", "#e0def4", "#908caa", "#403d52", "#ebbcba", "#2a2837",
            "#9ccfd8", "#c4a7e7", "#eb6f92", "#c4a7e7", "#524f67", "#9ccfd8", "#eb6f92", "#f6c177",
            "#31748f", "#f6c177", "#eb6f92",
        ],
    ),
    (
        "nord",
        [
            "#242933", "#2e3440", "#3b4252", "#eceff4", "#8f99ab", "#4c566a", "#88c0d0", "#3b4252",
            "#81a1c1", "#b48ead", "#bf616a", "#88c0d0", "#4c566a", "#a3be8c", "#bf616a", "#ebcb8b",
            "#a3be8c", "#ebcb8b", "#bf616a",
        ],
    ),
    (
        "gruvbox",
        [
            "#1d2021", "#282828", "#3c3836", "#ebdbb2", "#a89984", "#504945", "#fabd2f", "#3c3836",
            "#83a598", "#d3869b", "#fb4934", "#83a598", "#504945", "#b8bb26", "#fb4934", "#fe8019",
            "#b8bb26", "#fabd2f", "#fb4934",
        ],
    ),
    (
        "dracula",
        [
            "#191a21", "#21222c", "#343746", "#f8f8f2", "#8b8fa8", "#44475a", "#bd93f9", "#343746",
            "#8be9fd", "#bd93f9", "#ff5555", "#bd93f9", "#6272a4", "#50fa7b", "#ff79c6", "#f1fa8c",
            "#50fa7b", "#f1fa8c", "#ff5555",
        ],
    ),
    (
        "paper",
        [
            "#f6f5f1", "#ecebe5", "#d6d4cb", "#2a2b30", "#73757d", "#b9b8b0", "#4c5fd5", "#dcdcf0",
            "#4c5fd5", "#8b5cf6", "#d9344f", "#4c5fd5", "#a5acd8", "#11845b", "#d9344f", "#b7791f",
            "#11845b", "#b7791f", "#d9344f",
        ],
    ),
];

/// Names of the built-in themes, default first.
pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN.iter().map(|(name, _)| *name)
}

impl Theme {
    /// Look up a built-in theme by name.
    pub fn builtin(name: &str) -> Option<Self> {
        let (name, palette) = BUILTIN.iter().find(|(n, _)| *n == name)?;
        let mut theme = Self::blank(name);
        for (key, hex) in COLOR_KEYS.iter().zip(palette) {
            theme.set(key, hex).expect("built-in palettes are valid");
        }
        Some(theme)
    }

    fn blank(name: &str) -> Self {
        let c = Color::Reset;
        Self {
            name: name.to_string(),
            bg: c,
            panel: c,
            border: c,
            text: c,
            dim: c,
            faint: c,
            accent: c,
            select: c,
            cpu_low: c,
            cpu_mid: c,
            cpu_high: c,
            mem: c,
            cache: c,
            rx: c,
            tx: c,
            disk: c,
            ok: c,
            warn: c,
            crit: c,
        }
    }

    /// Set one color by its config key.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let color = parse_color(value).with_context(|| format!("color `{key}`"))?;
        let slot = match key {
            "bg" => &mut self.bg,
            "panel" => &mut self.panel,
            "border" => &mut self.border,
            "text" => &mut self.text,
            "dim" => &mut self.dim,
            "faint" => &mut self.faint,
            "accent" => &mut self.accent,
            "select" => &mut self.select,
            "cpu_low" => &mut self.cpu_low,
            "cpu_mid" => &mut self.cpu_mid,
            "cpu_high" => &mut self.cpu_high,
            "mem" => &mut self.mem,
            "cache" => &mut self.cache,
            "rx" => &mut self.rx,
            "tx" => &mut self.tx,
            "disk" => &mut self.disk,
            "ok" => &mut self.ok,
            "warn" => &mut self.warn,
            "crit" => &mut self.crit,
            other => bail!(
                "unknown color key `{other}` (valid: {})",
                COLOR_KEYS.join(", ")
            ),
        };
        *slot = color;
        Ok(())
    }

    /// Apply a `[colors]` table on top of this theme.
    pub fn apply(&mut self, colors: &BTreeMap<String, String>) -> Result<()> {
        colors.iter().try_for_each(|(k, v)| self.set(k, v))
    }

    /// Drop the painted backgrounds so the terminal's own (often translucent) bg shows.
    pub fn make_transparent(&mut self) {
        self.bg = Color::Reset;
        self.panel = Color::Reset;
    }

    /// CPU-style heat color for a 0..=1 fraction.
    pub fn heat(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            lerp(self.cpu_low, self.cpu_mid, t * 2.0)
        } else {
            lerp(self.cpu_mid, self.cpu_high, (t - 0.5) * 2.0)
        }
    }

    /// Dim solid color for empty bar tracks: between the panel and `faint`.
    pub fn track(&self) -> Color {
        let base = if self.panel == Color::Reset {
            self.bg
        } else {
            self.panel
        };
        if base == Color::Reset {
            self.faint
        } else {
            lerp(base, self.faint, 0.55)
        }
    }

    /// Soft background tint for the area under a graph line.
    pub fn tint(&self, color: Color) -> Option<Color> {
        let base = if self.panel == Color::Reset {
            self.bg
        } else {
            self.panel
        };
        (base != Color::Reset).then(|| lerp(base, color, 0.12))
    }

    /// Severity color for a percentage: calm, warning, critical.
    pub fn severity(&self, pct: f64) -> Color {
        match pct {
            p if p >= 90.0 => self.crit,
            p if p >= 70.0 => self.warn,
            _ => self.accent,
        }
    }
}

/// Linear blend between two RGB colors; non-RGB colors snap to the nearer end.
pub fn lerp(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
            Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
        }
        _ if t < 0.5 => a,
        _ => b,
    }
}

/// Parse `#rrggbb`, `#rgb`, a named ANSI color, or `reset`/`default`/`none`.
pub fn parse_color(value: &str) -> Result<Color> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let expanded: String = match hex.len() {
            3 => hex.chars().flat_map(|c| [c, c]).collect(),
            6 => hex.to_string(),
            _ => bail!("`{v}` is not #rgb or #rrggbb"),
        };
        let n = u32::from_str_radix(&expanded, 16).with_context(|| format!("bad hex `{v}`"))?;
        return Ok(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8));
    }
    Ok(match v.to_ascii_lowercase().as_str() {
        "reset" | "default" | "none" | "transparent" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "white" => Color::White,
        other => bail!("unknown color `{other}`"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_parses() {
        for name in builtin_names() {
            assert!(Theme::builtin(name).is_some(), "{name}");
        }
    }

    #[test]
    fn parses_hex_and_names() {
        assert_eq!(parse_color("#fff").unwrap(), Color::Rgb(255, 255, 255));
        assert_eq!(parse_color("#102030").unwrap(), Color::Rgb(16, 32, 48));
        assert_eq!(parse_color("Reset").unwrap(), Color::Reset);
        assert!(parse_color("#12").is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        let mut t = Theme::builtin("pulse").unwrap();
        assert!(t.set("bogus", "#000").is_err());
    }
}
