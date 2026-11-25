//! Application state and input handling. Rendering lives in `ui`.

use crate::agents::{self, AgentReport};
use crate::apps::{self, AppRow};
use crate::config::{self, Config};
use crate::sys::Sampler;
use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

/// Per-app CPU samples kept for the trend column.
const TREND: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Cpu,
    Mem,
    Io,
    Procs,
    Name,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Mem => "mem",
            Self::Io => "io",
            Self::Procs => "procs",
            Self::Name => "name",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Cpu => Self::Mem,
            Self::Mem => Self::Io,
            Self::Io => Self::Procs,
            Self::Procs => Self::Name,
            Self::Name => Self::Cpu,
        }
    }
}

pub enum Mode {
    Normal,
    Filter,
    Help,
    Confirm { name: String, pids: Vec<u32> },
}

/// One visible line of the apps table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Line {
    App(usize),
    /// App index, index into `sampler.procs`.
    Proc(usize, usize),
}

pub struct App {
    pub cfg: Config,
    pub theme: Theme,
    pub sampler: Sampler,
    pub apps: Vec<AppRow>,
    pub ports: HashMap<u32, Vec<u16>>,
    pub trends: HashMap<String, VecDeque<f64>>,
    agents: Option<agents::Shared>,
    pub agent_reports: Option<Vec<AgentReport>>,
    pub lines: Vec<Line>,
    pub selected: usize,
    pub scroll: usize,
    /// Selection identity, so the cursor follows an app as rows re-sort.
    selected_key: Option<(String, Option<u32>)>,
    pub expanded: HashSet<String>,
    pub sort: Sort,
    pub filter: String,
    pub mode: Mode,
    pub show_agents: bool,
    pub flash: Option<(String, Instant)>,
    pub quit: bool,
}

impl App {
    pub fn new(cfg: Config, theme: Theme) -> Self {
        let agents = cfg.agents.enabled.then(|| agents::spawn(&cfg.agents));
        let show_agents = cfg.agents.enabled;
        let mut app = Self {
            cfg,
            theme,
            sampler: Sampler::new(),
            apps: Vec::new(),
            ports: HashMap::new(),
            trends: HashMap::new(),
            agents,
            agent_reports: None,
            lines: Vec::new(),
            selected: 0,
            scroll: 0,
            selected_key: None,
            expanded: HashSet::new(),
            sort: Sort::Cpu,
            filter: String::new(),
            mode: Mode::Normal,
            show_agents,
            flash: None,
            quit: false,
        };
        app.tick();
        app
    }

    /// Sample the system and rebuild everything derived from it.
    pub fn tick(&mut self) {
        self.sampler.tick();
        self.ports = self.sampler.ports().clone();
        self.apps = apps::group(&self.sampler.procs, &self.ports);
        for a in &self.apps {
            let t = self.trends.entry(a.name.clone()).or_default();
            if t.len() == TREND {
                t.pop_front();
            }
            t.push_back(a.cpu as f64);
        }
        let live: HashSet<&str> = self.apps.iter().map(|a| a.name.as_str()).collect();
        self.trends.retain(|name, _| live.contains(name.as_str()));
        self.pull_agents();
        self.rebuild();
    }

    /// Grab the latest agent scan without blocking the UI.
    pub fn pull_agents(&mut self) {
        if let Some(shared) = &self.agents
            && let Ok(slot) = shared.try_lock()
            && slot.is_some()
        {
            self.agent_reports.clone_from(&slot);
        }
    }

    pub fn agents_enabled(&self) -> bool {
        self.agents.is_some()
    }

    pub fn app_by_name(&self, name: &str) -> Option<&AppRow> {
        self.apps.iter().find(|a| a.name == name)
    }

    fn rebuild(&mut self) {
        let needle = self.filter.to_lowercase();
        let procs = &self.sampler.procs;
        let mut order: Vec<usize> = (0..self.apps.len())
            .filter(|&i| {
                needle.is_empty()
                    || self.apps[i].name.to_lowercase().contains(&needle)
                    || self.apps[i].ports.iter().any(|p| p.to_string() == needle)
                    || self.apps[i]
                        .procs
                        .iter()
                        .any(|&p| procs[p].name.to_lowercase().contains(&needle))
            })
            .collect();
        let apps = &self.apps;
        order.sort_by(|&a, &b| {
            let (a, b) = (&apps[a], &apps[b]);
            match self.sort {
                Sort::Cpu => b.cpu.total_cmp(&a.cpu).then(b.mem.cmp(&a.mem)),
                Sort::Mem => b.mem.cmp(&a.mem),
                Sort::Io => (b.read_rate + b.write_rate)
                    .total_cmp(&(a.read_rate + a.write_rate))
                    .then(b.mem.cmp(&a.mem)),
                Sort::Procs => b.procs.len().cmp(&a.procs.len()),
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });
        self.lines.clear();
        for i in order {
            self.lines.push(Line::App(i));
            if self.expanded.contains(&self.apps[i].name) {
                self.lines
                    .extend(self.apps[i].procs.iter().map(|&p| Line::Proc(i, p)));
            }
        }
        // Keep the cursor on the same app/process if it still exists.
        if let Some(key) = &self.selected_key
            && let Some(pos) = self.lines.iter().position(|l| &self.key_of(*l) == key)
        {
            self.selected = pos;
        }
        self.selected = self.selected.min(self.lines.len().saturating_sub(1));
    }

    fn key_of(&self, line: Line) -> (String, Option<u32>) {
        match line {
            Line::App(i) => (self.apps[i].name.clone(), None),
            Line::Proc(i, p) => (self.apps[i].name.clone(), Some(self.sampler.procs[p].pid)),
        }
    }

    fn select(&mut self, idx: usize) {
        self.selected = idx.min(self.lines.len().saturating_sub(1));
        self.selected_key = self.lines.get(self.selected).map(|&l| self.key_of(l));
    }

    fn selected_app(&self) -> Option<usize> {
        self.lines.get(self.selected).map(|l| match *l {
            Line::App(i) | Line::Proc(i, _) => i,
        })
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), Instant::now()));
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match &self.mode {
            Mode::Filter => self.on_filter_key(key.code),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Confirm { pids, name } => {
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) {
                    let sent = self.sampler.terminate(pids);
                    let msg = format!(
                        "sent SIGTERM to {sent} process{} of {name}",
                        if sent == 1 { "" } else { "es" }
                    );
                    self.notify(msg);
                }
                self.mode = Mode::Normal;
            }
            Mode::Normal => self.on_normal_key(key.code),
        }
    }

    fn on_filter_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Char(c) => self.filter.push(c),
            KeyCode::Up | KeyCode::Down => {
                self.mode = Mode::Normal;
                self.on_normal_key(code);
                return;
            }
            _ => return,
        }
        self.select(0);
        self.rebuild();
    }

    fn on_normal_key(&mut self, code: KeyCode) {
        let page = 10;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if !self.filter.is_empty() && code == KeyCode::Esc {
                    self.filter.clear();
                    self.rebuild();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.select(self.selected.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.select(self.selected + 1),
            KeyCode::PageUp => self.select(self.selected.saturating_sub(page)),
            KeyCode::PageDown => self.select(self.selected + page),
            KeyCode::Home | KeyCode::Char('g') => self.select(0),
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Left => {
                if let Some(i) = self.selected_app() {
                    let name = self.apps[i].name.clone();
                    let open = self.expanded.contains(&name);
                    if code != KeyCode::Right && (open || code == KeyCode::Left) {
                        self.expanded.remove(&name);
                    } else {
                        self.expanded.insert(name.clone());
                    }
                    self.selected_key = Some((name, None));
                    self.rebuild();
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.rebuild();
            }
            KeyCode::Char('a') if self.agents_enabled() => self.show_agents = !self.show_agents,
            KeyCode::Char('t') => self.cycle_theme(1),
            KeyCode::Char('T') => self.cycle_theme(-1),
            KeyCode::Char('?') | KeyCode::Char('h') => self.mode = Mode::Help,
            KeyCode::Char('x') | KeyCode::Delete => self.confirm_kill(),
            _ => {}
        }
    }

    fn confirm_kill(&mut self) {
        let Some(&line) = self.lines.get(self.selected) else {
            return;
        };
        let procs = &self.sampler.procs;
        let (name, pids) = match line {
            Line::App(i) => (self.apps[i].name.clone(), self.apps[i].pids(procs)),
            Line::Proc(_, p) => (
                format!("{} ({})", procs[p].name, procs[p].pid),
                vec![procs[p].pid],
            ),
        };
        self.mode = Mode::Confirm { name, pids };
    }

    fn cycle_theme(&mut self, step: isize) {
        let names = config::theme_names();
        let at = names
            .iter()
            .position(|n| *n == self.theme.name)
            .unwrap_or(0) as isize;
        let next = &names[(at + step).rem_euclid(names.len() as isize) as usize];
        match self.cfg.resolve_theme(next) {
            Ok(t) => {
                self.theme = t;
                self.notify(format!(
                    "theme: {next}  (set `theme = \"{next}\"` in config to keep it)"
                ));
            }
            Err(e) => self.notify(format!("theme {next}: {e:#}")),
        }
    }
}
