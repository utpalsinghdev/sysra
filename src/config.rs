//! User configuration, loaded from `~/.config/sysra/config.toml`.

use crate::theme::{self, Theme};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Built-in theme name or a file stem from `~/.config/sysra/themes/`.
    pub theme: String,
    /// Milliseconds between samples.
    pub refresh_ms: u64,
    /// Leave the terminal background unpainted.
    pub transparent: bool,
    /// `filled` paints panel backgrounds; `outline` draws rounded borders instead.
    pub panel_style: PanelStyle,
    /// Per-key color overrides applied on top of the theme.
    pub colors: BTreeMap<String, String>,
    pub agents: AgentsConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelStyle {
    Filled,
    Outline,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentsConfig {
    /// Show the AI agents panel.
    pub enabled: bool,
    /// Agent ids to hide: claude, codex, pi, opencode, hermes.
    pub hide: Vec<String>,
    /// Seconds between rescans of agent usage logs.
    pub refresh_secs: u64,
    /// Ask Anthropic for live Claude plan limits using Claude Code's own login.
    pub claude_live_limits: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "pulse".into(),
            refresh_ms: 1000,
            transparent: false,
            panel_style: PanelStyle::Filled,
            colors: BTreeMap::new(),
            agents: AgentsConfig::default(),
        }
    }
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hide: Vec::new(),
            refresh_secs: 30,
            claude_live_limits: true,
        }
    }
}

/// A theme file in the themes directory: an optional base plus overrides.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    extends: Option<String>,
    #[serde(default)]
    colors: BTreeMap<String, String>,
}

/// `$XDG_CONFIG_HOME/sysra`, falling back to `~/.config/sysra` on every OS.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sysra")
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

impl Config {
    /// Load the config file; a missing file yields defaults.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Resolve a theme by name with the user's overrides and transparency applied.
    pub fn resolve_theme(&self, name: &str) -> Result<Theme> {
        let mut theme = load_theme(name, 0)?;
        theme.apply(&self.colors).context("[colors] in config")?;
        if self.transparent {
            theme.make_transparent();
        }
        Ok(theme)
    }
}

fn load_theme(name: &str, depth: u8) -> Result<Theme> {
    if depth > 4 {
        bail!("theme `{name}` extends too deeply");
    }
    let path = config_dir().join("themes").join(format!("{name}.toml"));
    if path.exists() {
        let text = fs::read_to_string(&path)?;
        let file: ThemeFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let mut theme = load_theme(file.extends.as_deref().unwrap_or("pulse"), depth + 1)?;
        theme.name = name.to_string();
        theme
            .apply(&file.colors)
            .with_context(|| format!("in {}", path.display()))?;
        return Ok(theme);
    }
    Theme::builtin(name).with_context(|| {
        format!(
            "unknown theme `{name}` (built-in: {})",
            theme_names().join(", ")
        )
    })
}

/// Built-in theme names followed by user theme files, without duplicates.
pub fn theme_names() -> Vec<String> {
    let mut names: Vec<String> = theme::builtin_names().map(String::from).collect();
    if let Ok(entries) = fs::read_dir(config_dir().join("themes")) {
        let mut custom: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()? == "toml").then(|| p.file_stem()?.to_str().map(String::from))?
            })
            .filter(|n| !names.contains(n))
            .collect();
        custom.sort();
        names.extend(custom);
    }
    names
}

/// The commented starter config written by `sysra --init-config`.
pub const DEFAULT_CONFIG: &str = r##"# sysra configuration
# Every key is optional. Delete anything you don't want to change.

# Built-in: pulse, tokyo-night, catppuccin, rose-pine, nord, gruvbox, dracula, paper
# Custom themes: drop `<name>.toml` into ~/.config/sysra/themes/ (see README).
theme = "pulse"

# Milliseconds between samples.
refresh_ms = 1000

# Leave the terminal background alone (great with translucent terminals).
transparent = false

# "filled" paints soft panel backgrounds, "outline" draws rounded borders.
panel_style = "filled"

# Override individual colors of whatever theme is active.
[colors]
# accent = "#8b9cf7"
# cpu_low = "#7f8cf0"
# cpu_mid = "#b79af2"
# cpu_high = "#f27a93"
# keys: bg panel border text dim faint accent select cpu_low cpu_mid cpu_high
#       mem cache rx tx disk ok warn crit

[agents]
enabled = true
# Hide agents you don't use: claude, codex, pi, opencode, hermes
hide = []
# Seconds between rescans of local usage logs.
refresh_secs = 30
# Fetch live Claude plan limits (5h / weekly) with Claude Code's existing login.
claude_live_limits = true
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_config_parses() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.theme, "pulse");
        assert_eq!(cfg.panel_style, PanelStyle::Filled);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>("thme = \"nord\"").is_err());
    }
}
