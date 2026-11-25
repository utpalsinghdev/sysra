//! System sampling: one `Sampler` owns the sysinfo handles and keeps short
//! histories for the graphs.

mod diskio;
pub mod ports;

use std::collections::VecDeque;
use std::time::Instant;
use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, Signal, System, ThreadKind, UpdateKind, Users,
};

/// Samples kept for graphs (wide terminals show ~2 samples per column).
pub const HISTORY: usize = 600;

/// One process, flattened from sysinfo into plain data for grouping.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub exe: String,
    pub cmd: Vec<String>,
    pub user: String,
    pub cwd: String,
    pub kernel: bool,
    /// Share of total machine CPU, 0..=100.
    pub cpu: f32,
    pub mem: u64,
    pub read_rate: f64,
    pub write_rate: f64,
}

#[derive(Debug, Clone)]
pub struct Mount {
    pub path: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Default)]
pub struct History {
    pub cpu: VecDeque<f64>,
    pub mem: VecDeque<f64>,
    pub rx: VecDeque<f64>,
    pub tx: VecDeque<f64>,
}

pub struct Sampler {
    sys: System,
    networks: Option<Networks>,
    disks: Option<Disks>,
    users: Option<Users>,
    disk_io: diskio::DiskIo,
    ports: ports::PortScanner,
    last: Instant,
    pub host: String,
    pub os: String,
    pub kernel: String,
    pub uptime: u64,
    pub cpu: f64,
    pub cores: Vec<f64>,
    pub freq_ghz: f64,
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_cache: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub rx_total: u64,
    /// Busiest physical interface, for labeling.
    pub iface: String,
    pub tx_total: u64,
    pub read_rate: f64,
    pub write_rate: f64,
    pub load: [f64; 3],
    pub mounts: Vec<Mount>,
    pub procs: Vec<Proc>,
    pub history: History,
}

fn push(buf: &mut VecDeque<f64>, v: f64) {
    if buf.len() == HISTORY {
        buf.pop_front();
    }
    buf.push_back(v);
}

impl Sampler {
    pub fn new() -> Self {
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());
        let sys = System::new_with_specifics(refresh)
            .or_else(|_| System::new())
            .expect("sysinfo is unavailable on this platform");
        let mut s = Self {
            sys,
            networks: Networks::new_with_refreshed_list().ok(),
            disks: Disks::new_with_refreshed_list().ok(),
            users: Users::new_with_refreshed_list().ok(),
            disk_io: diskio::DiskIo::default(),
            ports: ports::PortScanner::default(),
            last: Instant::now(),
            host: System::host_name().unwrap_or_else(|_| "localhost".into()),
            os: match (System::name(), System::os_version()) {
                (Ok(name), Ok(ver)) if !name.contains(&ver) => format!("{name} {ver}"),
                (Ok(name), _) => name,
                _ => String::new(),
            },
            kernel: System::kernel_version().unwrap_or_default(),
            uptime: 0,
            cpu: 0.0,
            cores: Vec::new(),
            freq_ghz: 0.0,
            mem_total: 0,
            mem_used: 0,
            mem_cache: 0,
            swap_total: 0,
            swap_used: 0,
            rx_rate: 0.0,
            tx_rate: 0.0,
            rx_total: 0,
            iface: String::new(),
            tx_total: 0,
            read_rate: 0.0,
            write_rate: 0.0,
            load: [0.0; 3],
            mounts: Vec::new(),
            procs: Vec::new(),
            history: History::default(),
        };
        s.refresh_processes();
        s
    }

    fn refresh_processes(&mut self) {
        let kind = ProcessRefreshKind::nothing()
            .without_tasks()
            .with_cpu()
            .with_memory()
            .with_disk_usage()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet);
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }

    /// Take a fresh sample. Call roughly once per `refresh_ms`.
    pub fn tick(&mut self) {
        let dt = self.last.elapsed().as_secs_f64().max(0.05);
        self.last = Instant::now();

        self.sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        self.sys.refresh_memory();
        self.refresh_processes();

        let ncpu = self.sys.cpus().len().max(1);
        self.cpu = self.sys.global_cpu_usage() as f64;
        self.cores = self.sys.cpus().iter().map(|c| c.usage() as f64).collect();
        self.freq_ghz = self
            .sys
            .cpus()
            .iter()
            .map(|c| c.frequency() as f64)
            .sum::<f64>()
            / ncpu as f64
            / 1000.0;

        self.mem_total = self.sys.total_memory();
        self.mem_used = self.sys.used_memory();
        let avail = self.sys.available_memory();
        self.mem_cache = avail.saturating_sub(self.sys.free_memory());
        self.swap_total = self.sys.total_swap();
        self.swap_used = self.sys.used_swap();
        self.uptime = System::uptime().unwrap_or(0);
        if let Ok(l) = System::load_average() {
            self.load = [l.one, l.five, l.fifteen];
        }

        if let Some(nets) = self.networks.as_mut() {
            nets.refresh(true);
            let (mut rx, mut tx) = (0, 0);
            let (mut rx_t, mut tx_t) = (0, 0);
            let mut busiest = (0, String::new());
            for (name, data) in nets.iter() {
                let name = name.to_string_lossy();
                if is_virtual_iface(&name) {
                    continue;
                }
                let total = data.total_received() + data.total_transmitted();
                if total >= busiest.0 {
                    busiest = (total, name.into_owned());
                }
                rx += data.received();
                tx += data.transmitted();
                rx_t += data.total_received();
                tx_t += data.total_transmitted();
            }
            self.rx_rate = rx as f64 / dt;
            self.tx_rate = tx as f64 / dt;
            self.rx_total = rx_t;
            self.iface = busiest.1;
            self.tx_total = tx_t;
        }

        self.collect_procs(ncpu, dt);
        let (r, w) = self.disk_io.sample(dt).unwrap_or_else(|| {
            self.procs
                .iter()
                .fold((0.0, 0.0), |(r, w), p| (r + p.read_rate, w + p.write_rate))
        });
        self.read_rate = r;
        self.write_rate = w;

        self.refresh_mounts();

        push(&mut self.history.cpu, self.cpu);
        push(
            &mut self.history.mem,
            self.mem_used as f64 / self.mem_total.max(1) as f64 * 100.0,
        );
        push(&mut self.history.rx, self.rx_rate);
        push(&mut self.history.tx, self.tx_rate);
    }

    fn collect_procs(&mut self, ncpu: usize, dt: f64) {
        let users = self.users.as_ref();
        self.procs = self
            .sys
            .processes()
            .values()
            .filter(|p| p.thread_kind() != Some(ThreadKind::Userland))
            .map(|p| {
                let pid = p.pid().as_u32();
                let ppid = p.parent().map(Pid::as_u32);
                let disk = p.disk_usage();
                Proc {
                    pid,
                    ppid,
                    name: p.name().to_string_lossy().into_owned(),
                    exe: p
                        .exe()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    cmd: split_retitled(
                        p.cmd()
                            .iter()
                            .map(|a| a.to_string_lossy().into_owned())
                            .collect(),
                    ),
                    user: p
                        .user_id()
                        .and_then(|uid| users?.get_user_by_id(uid))
                        .map(|u| u.name().to_string())
                        .unwrap_or_default(),
                    cwd: p
                        .cwd()
                        .map(|c| c.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    kernel: p.thread_kind() == Some(ThreadKind::Kernel)
                        || pid == 2
                        || ppid == Some(2),
                    cpu: p.cpu_usage() / ncpu as f32,
                    mem: p.memory(),
                    read_rate: disk.read_bytes as f64 / dt,
                    write_rate: disk.written_bytes as f64 / dt,
                }
            })
            .collect();
    }

    fn refresh_mounts(&mut self) {
        let Some(disks) = self.disks.as_mut() else {
            return;
        };
        disks.refresh(true);
        let mut seen = std::collections::HashSet::new();
        self.mounts = disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .filter(|d| {
                let fs = d.file_system().to_string_lossy();
                let mount = d.mount_point().to_string_lossy();
                // Real storage only: skip RAM disks, AppImage/FUSE mounts and container layers.
                !matches!(
                    fs.as_ref(),
                    "tmpfs" | "devtmpfs" | "overlay" | "squashfs" | "efivarfs" | "ramfs"
                ) && !fs.starts_with("fuse")
                    && ![
                        "/tmp",
                        "/run",
                        "/snap",
                        "/var/lib/docker",
                        "/var/lib/containers",
                        "/proc",
                        "/sys",
                    ]
                    .iter()
                    .any(|p| mount.starts_with(p))
            })
            .filter(|d| seen.insert((d.total_space(), d.available_space())))
            .map(|d| Mount {
                path: d.mount_point().to_string_lossy().into_owned(),
                used: d.total_space() - d.available_space(),
                total: d.total_space(),
            })
            .collect();
        self.mounts
            .sort_by(|a, b| a.path.len().cmp(&b.path.len()).then(a.path.cmp(&b.path)));
    }

    /// Listening TCP ports per pid (Linux only; empty elsewhere).
    pub fn ports(&mut self) -> &std::collections::HashMap<u32, Vec<u16>> {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| !p.kernel)
            .map(|p| p.pid)
            .collect();
        self.ports.scan(&pids)
    }

    pub fn core_count(&self) -> usize {
        self.cores.len()
    }

    /// Send SIGTERM to each pid. Returns how many signals were delivered.
    pub fn terminate(&self, pids: &[u32]) -> usize {
        let me = std::process::id();
        pids.iter()
            .filter(|&&pid| pid != me)
            .filter_map(|&pid| self.sys.process(Pid::from_u32(pid)))
            .filter(|p| p.kill_with(Signal::Term).unwrap_or_else(|| p.kill()))
            .count()
    }
}

/// Processes that rewrite their title (`npm run dev`) expose one space-joined argv entry.
fn split_retitled(cmd: Vec<String>) -> Vec<String> {
    match cmd.as_slice() {
        [only] if only.contains(' ') && !std::path::Path::new(only).exists() => {
            only.split_whitespace().map(String::from).collect()
        }
        _ => cmd,
    }
}

/// Loopback, container bridges and VPN tunnels double-count physical traffic; skip them.
fn is_virtual_iface(name: &str) -> bool {
    name == "lo"
        || [
            "docker",
            "veth",
            "br-",
            "virbr",
            "lxc",
            "cni",
            "flannel",
            "vmnet",
            "utun",
            "awdl",
            "llw",
            "bridge",
            "tailscale",
            "wg",
            "tun",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}
