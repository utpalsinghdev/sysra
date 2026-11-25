//! sysra: an app-centric system monitor for the terminal.

mod agents;
mod app;
mod apps;
mod config;
mod sys;
mod theme;
mod ui;

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HELP: &str = "\
sysra — see which apps are using your machine, and how much AI you've burned today.

USAGE:
    sysra [OPTIONS]

OPTIONS:
    -t, --theme <NAME>     Use a theme for this run (see --list-themes)
    -c, --config <PATH>    Config file [default: ~/.config/sysra/config.toml]
    -r, --refresh <MS>     Milliseconds between samples
        --transparent      Don't paint the background
        --no-agents        Hide the AI agents panel
        --init-config      Write a starter config file and exit
        --list-themes      List available themes and exit
    -h, --help             Show this help
    -V, --version          Show version
";

#[derive(Default)]
struct Args {
    theme: Option<String>,
    config: Option<PathBuf>,
    refresh: Option<u64>,
    transparent: bool,
    no_agents: bool,
    init_config: bool,
    list_themes: bool,
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "-t" | "--theme" => args.theme = Some(value()?),
            "-c" | "--config" => args.config = Some(PathBuf::from(value()?)),
            "-r" | "--refresh" => {
                args.refresh = Some(value()?.parse().context("--refresh takes milliseconds")?)
            }
            "--transparent" => args.transparent = true,
            "--no-agents" => args.no_agents = true,
            "--init-config" => args.init_config = true,
            "--list-themes" => args.list_themes = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("sysra {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            other => bail!("unknown option `{other}`\n\n{HELP}"),
        }
    }
    Ok(Some(args))
}

fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let path = args
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);

    if args.init_config {
        if path.exists() {
            bail!("{} already exists; not overwriting", path.display());
        }
        std::fs::create_dir_all(path.parent().context("config path has no parent")?)?;
        std::fs::write(&path, config::DEFAULT_CONFIG)?;
        std::fs::create_dir_all(config::config_dir().join("themes"))?;
        println!("wrote {}", path.display());
        return Ok(());
    }
    if args.list_themes {
        for name in config::theme_names() {
            println!("{name}");
        }
        return Ok(());
    }

    let mut cfg = config::Config::load(&path)?;
    if let Some(t) = args.theme {
        cfg.theme = t;
    }
    if let Some(ms) = args.refresh {
        cfg.refresh_ms = ms;
    }
    cfg.refresh_ms = cfg.refresh_ms.clamp(200, 10_000);
    cfg.transparent |= args.transparent;
    cfg.agents.enabled &= !args.no_agents;
    let theme = cfg.resolve_theme(&cfg.theme.clone())?;

    let app = app::App::new(cfg, theme);
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, mut app: app::App) -> Result<()> {
    let every = Duration::from_millis(app.cfg.refresh_ms);
    let mut next_tick = Instant::now() + every;
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        // Wake at least twice a second so the clock and agent panel stay current.
        let timeout = next_tick
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(500));
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Resize(..) => {}
                _ => continue,
            }
            if app.quit {
                return Ok(());
            }
        }
        if Instant::now() >= next_tick {
            app.tick();
            next_tick = Instant::now() + every;
        } else {
            app.pull_agents();
        }
    }
}
