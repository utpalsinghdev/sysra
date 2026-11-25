//! Recognize what a process *is*: a known app, an AI agent, a terminal, a shell
//! that should be transparent, or a session root whose children are separate apps.

use crate::sys::Proc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    Agent,
    Terminal,
    System,
    Kernel,
}

/// Name rules matched against the lowercase executable basename.
/// `(prefix, display name, kind)`; first match wins.
const BY_BASENAME: &[(&str, &str, Kind)] = &[
    ("claude", "Claude Code", Kind::Agent),
    ("codex", "Codex", Kind::Agent),
    ("opencode", "OpenCode", Kind::Agent),
    ("hermes", "Hermes", Kind::Agent),
    ("gemini", "Gemini CLI", Kind::Agent),
    ("cursor-agent", "Cursor Agent", Kind::Agent),
    ("brave", "Brave", Kind::App),
    ("t3code", "T3 Code", Kind::App),
    ("google-chrome", "Chrome", Kind::App),
    ("chrome", "Chrome", Kind::App),
    ("chromium", "Chromium", Kind::App),
    ("firefox", "Firefox", Kind::App),
    ("librewolf", "LibreWolf", Kind::App),
    ("zen", "Zen", Kind::App),
    ("code", "VS Code", Kind::App),
    ("cursor", "Cursor", Kind::App),
    ("zed", "Zed", Kind::App),
    ("slack", "Slack", Kind::App),
    ("discord", "Discord", Kind::App),
    ("vesktop", "Discord", Kind::App),
    ("spotify", "Spotify", Kind::App),
    ("obsidian", "Obsidian", Kind::App),
    ("telegram", "Telegram", Kind::App),
    ("dockerd", "Docker", Kind::System),
    ("containerd", "Docker", Kind::System),
    ("docker", "Docker", Kind::System),
    ("postgres", "PostgreSQL", Kind::System),
    ("mysqld", "MySQL", Kind::System),
    ("mariadbd", "MariaDB", Kind::System),
    ("redis-server", "Redis", Kind::System),
    ("ollama", "Ollama", Kind::App),
    ("tailscaled", "Tailscale", Kind::System),
    ("pipewire", "PipeWire", Kind::System),
    ("wireplumber", "PipeWire", Kind::System),
    ("pulseaudio", "PulseAudio", Kind::System),
    ("networkmanager", "NetworkManager", Kind::System),
    ("systemd", "systemd", Kind::System),
    ("dbus", "D-Bus", Kind::System),
    ("xwayland", "Xwayland", Kind::System),
    ("xorg", "Xorg", Kind::System),
    ("foot", "foot", Kind::Terminal),
    ("kitty", "kitty", Kind::Terminal),
    ("alacritty", "Alacritty", Kind::Terminal),
    ("wezterm", "WezTerm", Kind::Terminal),
    ("ghostty", "Ghostty", Kind::Terminal),
    ("gnome-terminal", "GNOME Terminal", Kind::Terminal),
    ("konsole", "Konsole", Kind::Terminal),
    ("xterm", "xterm", Kind::Terminal),
    ("urxvt", "urxvt", Kind::Terminal),
    ("tilix", "Tilix", Kind::Terminal),
    ("terminator", "Terminator", Kind::Terminal),
    ("warp", "Warp", Kind::Terminal),
    ("iterm2", "iTerm2", Kind::Terminal),
    ("terminal", "Terminal", Kind::Terminal),
    ("tmux", "tmux", Kind::Terminal),
    ("zellij", "Zellij", Kind::Terminal),
    ("screen", "screen", Kind::Terminal),
    ("sshd", "SSH sessions", Kind::Terminal),
    ("mosh-server", "mosh", Kind::Terminal),
];

/// Substrings of the full command line that identify apps run through an interpreter.
const BY_CMDLINE: &[(&str, &str, Kind)] = &[
    ("@anthropic-ai/claude-code", "Claude Code", Kind::Agent),
    ("claude-code/cli", "Claude Code", Kind::Agent),
    // The native installer names the binary after its version: ~/.local/share/claude/versions/2.1.3
    ("/claude/versions/", "Claude Code", Kind::Agent),
    ("@openai/codex", "Codex", Kind::Agent),
    ("pi-coding-agent", "Pi", Kind::Agent),
    ("/coding-agent/dist/", "Pi", Kind::Agent),
    ("hermes-agent", "Hermes", Kind::Agent),
    ("@google/gemini-cli", "Gemini CLI", Kind::Agent),
    ("brave", "Brave", Kind::App),
];

/// Processes whose children are independent apps (init, compositors, desktop sessions).
const SESSION_ROOTS: &[&str] = &[
    "systemd",
    "init",
    "launchd",
    "kthreadd",
    "hyprland",
    "sway",
    "river",
    "niri",
    "labwc",
    "wayfire",
    "gnome-shell",
    "gnome-session-binary",
    "gnome-session",
    "kwin_wayland",
    "kwin_x11",
    "plasmashell",
    "ksmserver",
    "xfce4-session",
    "cosmic-comp",
    "mutter",
    "i3",
    "bspwm",
    "awesome",
    "openbox",
    "dwm",
    "uwsm",
    "sddm",
    "gdm",
    "lightdm",
    "login",
    "agetty",
    "dbus-daemon",
    "dbus-broker",
    "startplasma-wayland",
    "startplasma-x11",
];

const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "tcsh", "csh", "nu", "elvish", "xonsh",
];

/// Thin launchers that only exec something else.
const WRAPPERS: &[&str] = &[
    "sudo",
    "doas",
    "su",
    "runuser",
    "env",
    "nohup",
    "setsid",
    "timeout",
    "nice",
    "ionice",
    "stdbuf",
    "script",
    "uwsm-app",
    "app2unit",
    "flatpak-spawn",
    "bwrap",
    "xdg-open",
    "gtk-launch",
    "systemd-run",
];

/// Process supervisors: every child they manage is its own app.
const SUPERVISORS: &[&str] = &[
    "pm2",
    "supervisord",
    "runsvdir",
    "s6-svscan",
    "process-compose",
];

/// Interpreters and package runners: name the app after its project folder or script.
const INTERPRETERS: &[&str] = &[
    "npm", "npx", "pnpm", "yarn", "bunx", "uv", "uvx", "poetry", "pipx", "node", "nodejs", "bun",
    "deno", "python", "python3", "ruby", "perl", "php", "java", "lua",
];

/// Generic entry-point names that say nothing about the app; use the parent dir instead.
const GENERIC_SCRIPTS: &[&str] = &[
    "index", "main", "cli", "server", "app", "run", "start", "__main__",
];

/// Lowercase basename of the executable, for matching.
pub fn basename(p: &Proc) -> String {
    display_basename(p).to_ascii_lowercase()
}

/// Basename of the program as the user knows it, original case.
///
/// Falls back to argv[0] when the exe is unreadable, and prefers argv[0] when the exe is
/// an interpreter but the process renamed itself (`node` running as `pi`).
fn display_basename(p: &Proc) -> String {
    let argv0 = p
        .cmd
        .first()
        .filter(|a| !a.contains('[') && !a.contains(':'))
        .map(|a| a.rsplit('/').next().unwrap_or(a));
    let exe = p
        .exe
        .rsplit('/')
        .next()
        .unwrap_or(&p.exe)
        .trim_end_matches(" (deleted)");
    let exe_is_interpreter = INTERPRETERS.iter().any(|i| exe.starts_with(i));
    // Versioned binaries (`.../versions/2.1.3`) say nothing; argv[0] does.
    let exe_is_version = exe.starts_with(|c: char| c.is_ascii_digit());
    let base = match argv0 {
        Some(a)
            if exe.is_empty()
                || ((exe_is_interpreter || exe_is_version)
                    && !a.is_empty()
                    && !is_interpreter(a)) =>
        {
            a
        }
        _ if !exe.is_empty() => exe,
        _ => &p.name,
    };
    if base.is_empty() {
        p.name.clone()
    } else {
        base.to_string()
    }
}

fn is_interpreter(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    INTERPRETERS.iter().any(|i| n.starts_with(i))
}

pub fn is_session_root(p: &Proc) -> bool {
    let base = basename(p);
    p.pid == 1
        || SESSION_ROOTS.contains(&base.as_str())
        || SUPERVISORS.contains(&base.as_str())
        || p.name.starts_with("PM2")
}

pub fn is_shell(p: &Proc) -> bool {
    let b = basename(p);
    SHELLS.contains(&b.trim_start_matches('-'))
}

/// Shells and launch wrappers never own resources themselves; they belong to whoever
/// started them.
pub fn is_passthrough(p: &Proc) -> bool {
    is_shell(p) || WRAPPERS.contains(&basename(p).as_str())
}

/// An interactive shell (no `-c` command, no script) is where a user starts things,
/// so its children are separate apps. `bash -c ...` spawned by an agent is not.
pub fn is_interactive_shell(p: &Proc) -> bool {
    is_shell(p)
        && !p.cmd.iter().skip(1).any(|a| {
            !a.starts_with('-') || (a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
        })
}

/// Display name and kind for a process that starts a new app group.
pub fn identify(p: &Proc) -> (String, Kind) {
    if p.kernel {
        return ("kernel".into(), Kind::Kernel);
    }
    let shown = display_basename(p);
    let shown = shown.trim_end_matches(".exe").trim_end_matches(".AppImage");
    let lower = shown.to_ascii_lowercase();
    let base = lower.as_str();
    if base == "pi" {
        return ("Pi".into(), Kind::Agent);
    }
    if let Some((_, name, kind)) = BY_BASENAME
        .iter()
        .find(|(prefix, ..)| base.starts_with(prefix))
    {
        return ((*name).into(), *kind);
    }
    let hay = format!("{} {}", p.exe, p.cmd.join(" ")).to_ascii_lowercase();
    if let Some((_, name, kind)) = BY_CMDLINE.iter().find(|(needle, ..)| hay.contains(needle)) {
        return ((*name).into(), *kind);
    }
    if is_interpreter(base)
        && let Some(name) = project_name(&p.cwd).or_else(|| script_name(&p.cmd))
    {
        return (name, Kind::App);
    }
    (shown.to_string(), Kind::App)
}

/// The folder a dev server or script runs in says more than `node` does.
fn project_name(cwd: &str) -> Option<String> {
    let home = crate::agents::home();
    let path = std::path::Path::new(cwd);
    if cwd.is_empty() || path.parent().is_none() || path == home || path.starts_with("/tmp") {
        return None;
    }
    path.file_name()?
        .to_str()
        .filter(|n| !n.starts_with('.'))
        .map(String::from)
}

/// For `node path/to/vite.js --flag`, return `vite`; for `.../my-tool/index.js`, `my-tool`.
fn script_name(cmd: &[String]) -> Option<String> {
    let arg = cmd.iter().skip(1).find(|a| !a.starts_with('-'))?;
    let mut parts = arg.rsplit('/');
    let file = parts.next()?;
    let stem = file.split('.').next().unwrap_or(file);
    if stem.is_empty() {
        return None;
    }
    if GENERIC_SCRIPTS.contains(&stem) {
        let dir =
            parts.find(|d| !matches!(*d, "dist" | "bin" | "lib" | "build" | "src" | "out" | ""))?;
        return Some(dir.to_string());
    }
    Some(stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, cmd: &[&str]) -> Proc {
        Proc {
            pid: 100,
            ppid: Some(1),
            name: name.into(),
            exe: String::new(),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            user: "me".into(),
            cwd: String::new(),
            kernel: false,
            cpu: 0.0,
            mem: 0,
            read_rate: 0.0,
            write_rate: 0.0,
        }
    }

    #[test]
    fn identifies_agents_through_interpreters() {
        let p = proc(
            "node",
            &[
                "node",
                "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            ],
        );
        assert_eq!(identify(&p), ("Claude Code".into(), Kind::Agent));
        let p = proc("pi", &["/home/u/.local/bin/pi"]);
        assert_eq!(identify(&p).0, "Pi");
        let mut p = proc("claude", &["claude", "--resume"]);
        p.exe = "/home/u/.local/share/claude/versions/2.1.331".into();
        assert_eq!(identify(&p).0, "Claude Code", "native installer");
        let mut p = proc("pi", &["pi"]);
        p.exe = "/opt/node/bin/node".into();
        assert_eq!(identify(&p).0, "Pi", "retitled interpreter");
    }

    #[test]
    fn names_generic_scripts_by_directory() {
        let p = proc("node", &["node", "/srv/my-api/dist/index.js"]);
        assert_eq!(identify(&p).0, "my-api");
        let p = proc(
            "node",
            &["node", "node_modules/.bin/vite", "--port", "5173"],
        );
        assert_eq!(identify(&p).0, "vite");
    }

    #[test]
    fn names_dev_servers_by_project() {
        let mut p = proc("node", &["node", "node_modules/.bin/vite"]);
        p.cwd = "/srv/wiki-web".into();
        assert_eq!(identify(&p).0, "wiki-web");
    }

    #[test]
    fn shell_interactivity() {
        assert!(is_interactive_shell(&proc("bash", &["bash"])));
        assert!(is_interactive_shell(&proc("zsh", &["-zsh"])));
        assert!(is_interactive_shell(&proc("bash", &["bash", "-l"])));
        assert!(!is_interactive_shell(&proc(
            "bash",
            &["/bin/bash", "-c", "ls"]
        )));
        assert!(!is_interactive_shell(&proc("bash", &["bash", "-lc", "ls"])));
        assert!(!is_interactive_shell(&proc("bash", &["bash", "build.sh"])));
    }
}
