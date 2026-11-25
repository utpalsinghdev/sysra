//! Group raw processes into apps: everything Claude Code spawns counts as Claude Code,
//! every Brave renderer counts as Brave, and so on.
//!
//! Each process walks up its parent chain until it reaches a *launcher* (init, a desktop
//! session, a terminal, an interactive shell). The process right below the launcher is
//! the app's root, and the root's identity names the whole subtree. Roots with the same
//! name merge into one row (three Claude sessions -> one "Claude Code" line, 3 instances).

pub mod identify;

use crate::sys::Proc;
use identify::Kind;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct AppRow {
    pub name: String,
    pub kind: Kind,
    /// Separate launches merged into this row.
    pub instances: usize,
    /// Indices into the process list this grouping was built from.
    pub procs: Vec<usize>,
    pub cpu: f32,
    pub mem: u64,
    pub read_rate: f64,
    pub write_rate: f64,
    pub ports: BTreeSet<u16>,
    pub users: BTreeSet<String>,
}

impl AppRow {
    fn new(name: String, kind: Kind) -> Self {
        Self {
            name,
            kind,
            instances: 0,
            procs: Vec::new(),
            cpu: 0.0,
            mem: 0,
            read_rate: 0.0,
            write_rate: 0.0,
            ports: BTreeSet::new(),
            users: BTreeSet::new(),
        }
    }

    pub fn pids(&self, procs: &[Proc]) -> Vec<u32> {
        self.procs.iter().map(|&i| procs[i].pid).collect()
    }
}

/// Which app a process belongs to: the index of its root process.
struct Resolver<'a> {
    procs: &'a [Proc],
    by_pid: HashMap<u32, usize>,
    root_of: HashMap<usize, usize>,
    launcher: HashMap<usize, bool>,
    ident: HashMap<usize, (String, Kind)>,
}

impl<'a> Resolver<'a> {
    fn new(procs: &'a [Proc]) -> Self {
        let by_pid = procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        Self {
            procs,
            by_pid,
            root_of: HashMap::new(),
            launcher: HashMap::new(),
            ident: HashMap::new(),
        }
    }

    fn identity(&mut self, i: usize) -> &(String, Kind) {
        self.ident
            .entry(i)
            .or_insert_with(|| identify::identify(&self.procs[i]))
    }

    fn parent(&self, i: usize) -> Option<usize> {
        let ppid = self.procs[i].ppid?;
        self.by_pid.get(&ppid).copied().filter(|&p| p != i)
    }

    /// Children of a launcher start their own app.
    fn is_launcher(&mut self, i: usize) -> bool {
        if let Some(&v) = self.launcher.get(&i) {
            return v;
        }
        self.launcher.insert(i, false); // cycle guard
        let p = &self.procs[i];
        let v = identify::is_session_root(p)
            || identify::is_interactive_shell(p)
            || self.identity(i).1 == Kind::Terminal
            || (identify::is_passthrough(p)
                && self.parent(i).is_some_and(|par| self.is_launcher(par)));
        self.launcher.insert(i, v);
        v
    }

    fn root(&mut self, i: usize) -> usize {
        if let Some(&r) = self.root_of.get(&i) {
            return r;
        }
        self.root_of.insert(i, i); // cycle guard
        let r = self.resolve(i);
        self.root_of.insert(i, r);
        r
    }

    fn resolve(&mut self, i: usize) -> usize {
        let p = &self.procs[i];
        if p.kernel {
            return i;
        }
        let Some(par) = self.parent(i) else { return i };
        if !self.is_launcher(par) {
            let parent_root = self.root(par);
            // An agent started by another app (Pi inside an editor) still gets its own row.
            let (name, kind) = self.identity(i).clone();
            if kind == Kind::Agent && self.identity(parent_root).0 != name {
                return i;
            }
            return parent_root;
        }
        // A shell or `sudo` inside a terminal belongs to that terminal; under init it's its own.
        let parent_is_init =
            self.procs[par].pid == 1 || identify::basename(&self.procs[par]) == "systemd";
        if identify::is_passthrough(p) && !parent_is_init {
            return self.root(par);
        }
        i
    }
}

/// Build app rows from a process list. `ports` maps pid -> listening ports.
pub fn group(procs: &[Proc], ports: &HashMap<u32, Vec<u16>>) -> Vec<AppRow> {
    let mut resolver = Resolver::new(procs);
    let mut rows: Vec<AppRow> = Vec::new();
    let mut row_of_name: HashMap<String, usize> = HashMap::new();
    let mut row_of_root: HashMap<usize, usize> = HashMap::new();

    for (i, p) in procs.iter().enumerate() {
        let root = resolver.root(i);
        let row = *row_of_root.entry(root).or_insert_with(|| {
            let (name, kind) = resolver.identity(root).clone();
            let idx = *row_of_name.entry(name.clone()).or_insert_with(|| {
                rows.push(AppRow::new(name, kind));
                rows.len() - 1
            });
            rows[idx].instances += 1;
            idx
        });
        let r = &mut rows[row];
        r.procs.push(i);
        r.cpu += p.cpu;
        r.mem += p.mem;
        r.read_rate += p.read_rate;
        r.write_rate += p.write_rate;
        if let Some(ps) = ports.get(&p.pid) {
            r.ports.extend(ps);
        }
        if !p.user.is_empty() {
            r.users.insert(p.user.clone());
        }
    }
    for r in &mut rows {
        r.procs.sort_by(|&a, &b| {
            procs[b]
                .cpu
                .total_cmp(&procs[a].cpu)
                .then(procs[b].mem.cmp(&procs[a].mem))
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, name: &str, cmd: &[&str], cpu: f32) -> Proc {
        Proc {
            pid,
            ppid: Some(ppid),
            name: name.into(),
            exe: format!("/usr/bin/{name}"),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            user: "me".into(),
            cwd: String::new(),
            kernel: false,
            cpu,
            mem: 100,
            read_rate: 0.0,
            write_rate: 0.0,
        }
    }

    fn row<'a>(rows: &'a [AppRow], name: &str) -> &'a AppRow {
        rows.iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no row {name}: {rows:#?}"))
    }

    #[test]
    fn agent_children_roll_up_but_terminal_launches_split() {
        let procs = vec![
            p(1, 0, "systemd", &["/sbin/init"], 0.0),
            p(10, 1, "Hyprland", &["Hyprland"], 1.0),
            p(20, 10, "foot", &["foot"], 0.5),
            p(21, 20, "bash", &["bash"], 0.0),
            p(30, 21, "claude", &["claude"], 2.0),
            p(31, 30, "bash", &["/bin/bash", "-c", "cargo build"], 0.0),
            p(32, 31, "cargo", &["cargo", "build"], 4.0),
            p(40, 21, "claude", &["claude"], 1.0),
            p(50, 10, "sh", &["/bin/sh", "-c", "brave"], 0.0),
            p(51, 50, "brave", &["brave"], 3.0),
            p(52, 51, "brave", &["brave", "--type=renderer"], 3.0),
        ];
        let rows = group(&procs, &HashMap::new());
        let claude = row(&rows, "Claude Code");
        assert_eq!(claude.instances, 2);
        assert_eq!(claude.procs.len(), 4);
        assert!((claude.cpu - 7.0).abs() < 1e-6);
        assert_eq!(
            row(&rows, "foot").procs.len(),
            2,
            "terminal owns its interactive shell"
        );
        assert_eq!(row(&rows, "Brave").procs.len(), 2);
        assert_eq!(
            row(&rows, "Hyprland").procs.len(),
            2,
            "exec shell belongs to compositor"
        );
    }

    #[test]
    fn kernel_threads_collapse() {
        let mut k1 = p(100, 2, "kworker/0:1", &[], 0.5);
        k1.kernel = true;
        let mut k2 = p(101, 2, "kworker/1:1", &[], 0.5);
        k2.kernel = true;
        let rows = group(&[k1, k2], &HashMap::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "kernel");
    }
}
