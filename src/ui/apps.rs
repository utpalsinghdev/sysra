//! The apps table: one line per application, expandable into its processes.

use super::{dim, faint, fmt, graph, panel};
use crate::app::{App, Line as Row, Sort};
use crate::apps::identify::Kind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

struct Cols {
    name: usize,
    bars: bool,
    /// Bar width when shown; grows on wide terminals.
    bar: usize,
    trend: bool,
    io: bool,
    ports: bool,
    /// Ports column width when shown.
    ports_w: usize,
    user: bool,
}

const PROCS: usize = 7;
const VALUE: usize = 7;
const BAR: usize = 8;
const NAME_MAX: usize = 34;
const TREND: usize = 14;
const IO: usize = 11;
const PORTS: usize = 16;
const USER: usize = 10;

impl Cols {
    fn fit(width: usize) -> Self {
        let mut c = Self {
            name: 0,
            bars: true,
            bar: BAR,
            trend: true,
            io: true,
            ports: true,
            ports_w: PORTS,
            user: true,
        };
        let fixed = |c: &Self| {
            2 + PROCS
                + 2 * VALUE
                + if c.bars { 2 * (BAR + 1) } else { 0 }
                + if c.trend { TREND } else { 0 }
                + if c.io { IO } else { 0 }
                + if c.ports { PORTS } else { 0 }
                + if c.user { USER } else { 0 }
        };
        let drops: [fn(&mut Self); 5] = [
            |c| c.user = false,
            |c| c.io = false,
            |c| c.trend = false,
            |c| c.bars = false,
            |c| c.ports = false,
        ];
        for drop in drops {
            if fixed(&c) + 20 <= width {
                break;
            }
            drop(&mut c);
        }
        c.name = width.saturating_sub(fixed(&c) + 1).max(8);
        // Past a comfortable name width, spend the space on bars and ports instead.
        if c.name > NAME_MAX {
            let mut extra = c.name - NAME_MAX;
            if c.bars {
                let grow = (extra / 3).min(12);
                c.bar += grow;
                extra -= grow * 2;
            }
            if c.ports {
                c.ports_w += extra.min(14);
                extra -= extra.min(14);
            }
            c.name = NAME_MAX + extra;
        }
        c
    }
}

pub fn draw(f: &mut Frame, r: Rect, app: &mut App) {
    let t = app.theme.clone();
    let title = vec![
        Span::styled("• ", Style::new().fg(t.accent)),
        dim("apps", &t),
        Span::raw(" "),
        faint(format!("{}", app.apps.len()), &t),
    ];
    let filter = if app.filter.is_empty() {
        String::new()
    } else {
        format!("filter “{}”  ", app.filter)
    };
    let right = Line::from(vec![
        Span::styled(filter, Style::new().fg(t.accent)),
        faint(format!("sorted by {}", app.sort.label()), &t),
    ]);
    let inner = panel(f.buffer_mut(), r, app, title, right);
    if inner.height < 2 {
        return;
    }
    let cols = Cols::fit(inner.width as usize - 1);
    header(f, Rect { height: 1, ..inner }, app, &cols);

    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    let visible = body.height as usize;
    if app.selected < app.scroll {
        app.scroll = app.selected;
    } else if app.selected >= app.scroll + visible {
        app.scroll = app.selected + 1 - visible;
    }
    app.scroll = app.scroll.min(app.lines.len().saturating_sub(visible));

    let buf = f.buffer_mut();
    for (n, &row) in app.lines.iter().skip(app.scroll).take(visible).enumerate() {
        let y = body.y + n as u16;
        let selected = app.scroll + n == app.selected;
        let line = match row {
            Row::App(i) => app_line(app, i, &cols),
            Row::Proc(_, p) => proc_line(app, p, &cols),
        };
        let area = Rect::new(body.x, y, body.width - 1, 1);
        if selected {
            buf.set_style(
                Rect::new(body.x - 1, y, body.width + 1, 1),
                Style::new().bg(t.select),
            );
            buf[(body.x - 1, y)].set_char('▌').set_fg(t.accent);
        }
        line.render(area, buf);
    }
    scrollbar(
        buf,
        Rect::new(body.right() - 1, body.y, 1, body.height),
        app.scroll,
        visible,
        app.lines.len(),
        &t,
    );
}

fn header(f: &mut Frame, r: Rect, app: &App, c: &Cols) {
    let t = &app.theme;
    let col = |label: &str, w: usize, sort: Option<Sort>, right: bool| {
        let active = sort == Some(app.sort);
        let text = if active {
            format!("{label}↓")
        } else {
            label.to_string()
        };
        let text = if right {
            fmt::rfit(&text, w)
        } else {
            fmt::fit(&text, w)
        };
        Span::styled(
            text,
            Style::new().fg(if active { t.accent } else { t.faint }),
        )
    };
    let bar_w = if c.bars { c.bar + 1 } else { 0 };
    let mut spans = vec![Span::raw("  "), col("app", c.name, Some(Sort::Name), false)];
    spans.push(col("procs", PROCS, Some(Sort::Procs), true));
    spans.push(col("cpu", VALUE, Some(Sort::Cpu), true));
    spans.push(Span::raw(" ".repeat(bar_w)));
    if c.trend {
        spans.push(col("  trend", TREND, None, false));
    }
    spans.push(col("mem", VALUE, Some(Sort::Mem), true));
    spans.push(Span::raw(" ".repeat(bar_w)));
    if c.io {
        spans.push(col("disk io", IO, Some(Sort::Io), true));
    }
    if c.ports {
        spans.push(col("  ports", c.ports_w, None, false));
    }
    if c.user {
        spans.push(col("user", USER, None, false));
    }
    Line::from(spans).render(r, f.buffer_mut());
}

fn kind_color(app: &App, kind: Kind) -> Color {
    let t = &app.theme;
    match kind {
        Kind::Agent => t.accent,
        Kind::App => t.cpu_mid,
        Kind::Terminal => t.ok,
        Kind::System => t.dim,
        Kind::Kernel => t.faint,
    }
}

fn app_line(app: &App, i: usize, c: &Cols) -> Line<'static> {
    let t = &app.theme;
    let a = &app.apps[i];
    let open = app.expanded.contains(&a.name);
    let mut spans = vec![
        Span::styled(if open { "▼ " } else { "▶ " }, Style::new().fg(t.faint)),
        Span::styled("● ", Style::new().fg(kind_color(app, a.kind))),
    ];
    // Kernel threads and system services each count as a launch; that number is noise.
    let show_inst = a.instances > 1 && matches!(a.kind, Kind::App | Kind::Agent | Kind::Terminal);
    let inst = if show_inst {
        format!(" ×{}", a.instances)
    } else {
        String::new()
    };
    let name_w = c.name.saturating_sub(2);
    let name = fmt::fit(&a.name, name_w.saturating_sub(inst.chars().count()).max(4));
    let pad = name_w.saturating_sub(name.trim_end().chars().count() + inst.chars().count());
    spans.push(Span::styled(
        name.trim_end().to_string(),
        Style::new().fg(t.text).add_modifier(Modifier::BOLD),
    ));
    spans.push(faint(inst, t));
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(dim(fmt::rfit(&a.procs.len().to_string(), PROCS), t));
    let total_mem = app.sampler.mem_total.max(1) as f64;
    let mem_frac = a.mem as f64 / total_mem;
    metric(
        &mut spans,
        app,
        format!("{:.1}", a.cpu),
        a.cpu as f64 / 100.0,
        c,
        t.heat(a.cpu as f64 / 50.0),
    );
    if c.trend {
        let samples: Vec<f64> = app
            .trends
            .get(&a.name)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default();
        let max = samples.iter().copied().fold(5.0, f64::max);
        spans.push(Span::raw("  "));
        spans.extend(graph::sparkline(
            &samples,
            TREND - 2,
            max,
            t.faint,
            t.cpu_mid,
        ));
    }
    metric(&mut spans, app, fmt::bytes(a.mem), mem_frac, c, t.mem);
    if c.io {
        let io = a.read_rate + a.write_rate;
        spans.push(if io >= 1024.0 {
            dim(fmt::rfit(&fmt::rate(io), IO), t)
        } else {
            faint(fmt::rfit("—", IO), t)
        });
    }
    if c.ports {
        let ports: Vec<String> = a.ports.iter().map(|p| format!(":{p}")).collect();
        spans.push(Span::styled(
            fmt::fit(&format!("  {}", ports.join(" ")), c.ports_w),
            Style::new().fg(t.rx),
        ));
    }
    if c.user {
        let mut users = a.users.iter();
        let user = match (users.next(), a.users.len()) {
            (Some(u), 1) => u.clone(),
            (Some(u), n) => format!("{u}+{}", n - 1),
            (None, _) => String::new(),
        };
        spans.push(faint(fmt::fit(&format!(" {user}"), USER), t));
    }
    Line::from(spans)
}

/// Value right-aligned, then (optionally) a small bar.
fn metric(
    spans: &mut Vec<Span<'static>>,
    app: &App,
    value: String,
    frac: f64,
    c: &Cols,
    color: Color,
) {
    let t = &app.theme;
    let strong = frac >= 0.01;
    spans.push(Span::styled(
        fmt::rfit(&value, VALUE),
        Style::new().fg(if strong { t.text } else { t.dim }),
    ));
    if c.bars {
        spans.push(Span::raw(" "));
        let shown = frac;
        spans.extend(graph::bar(c.bar, shown, color, t.track()));
    }
}

fn proc_line(app: &App, p: usize, c: &Cols) -> Line<'static> {
    let t = &app.theme;
    let pr = &app.sampler.procs[p];
    let label = describe(pr);
    let mut spans = vec![
        Span::raw("  "),
        faint("└ ", t),
        dim(fmt::rfit(&pr.pid.to_string(), 7), t),
        Span::raw(" "),
        Span::styled(
            fmt::fit(&label, c.name.saturating_sub(10)),
            Style::new().fg(t.dim),
        ),
        Span::raw(" ".repeat(PROCS)),
    ];
    let total_mem = app.sampler.mem_total.max(1) as f64;
    metric(
        &mut spans,
        app,
        format!("{:.1}", pr.cpu),
        pr.cpu as f64 / 100.0,
        c,
        t.heat(pr.cpu as f64 / 50.0),
    );
    if c.trend {
        spans.push(Span::raw(" ".repeat(TREND)));
    }
    metric(
        &mut spans,
        app,
        fmt::bytes(pr.mem),
        pr.mem as f64 / total_mem,
        c,
        t.cache,
    );
    if c.io {
        let io = pr.read_rate + pr.write_rate;
        spans.push(if io >= 1024.0 {
            dim(fmt::rfit(&fmt::rate(io), IO), t)
        } else {
            faint(fmt::rfit("", IO), t)
        });
    }
    if c.ports {
        let ports = app.ports.get(&pr.pid).map(|v| {
            v.iter()
                .map(|p| format!(":{p}"))
                .collect::<Vec<_>>()
                .join(" ")
        });
        spans.push(Span::styled(
            fmt::fit(&format!("  {}", ports.unwrap_or_default()), c.ports_w),
            Style::new().fg(t.rx),
        ));
    }
    if c.user {
        spans.push(faint(fmt::fit(&format!(" {}", pr.user), USER), t));
    }
    Line::from(spans)
}

fn scrollbar(
    buf: &mut ratatui::buffer::Buffer,
    r: Rect,
    scroll: usize,
    visible: usize,
    total: usize,
    t: &crate::theme::Theme,
) {
    if total <= visible || r.height == 0 {
        return;
    }
    let h = r.height as usize;
    let thumb = (h * visible / total).max(1);
    let start = (h - thumb) * scroll / (total - visible).max(1);
    for i in 0..h {
        let (ch, color) = if (start..start + thumb).contains(&i) {
            ('┃', t.dim)
        } else {
            ('│', t.faint)
        };
        buf[(r.x, r.y + i as u16)].set_char(ch).set_fg(color);
    }
}

/// A readable one-liner for a process: Chromium-style helpers by role
/// (`brave · renderer`), everything else by name plus its non-flag arguments.
fn describe(p: &crate::sys::Proc) -> String {
    let flag = |key: &str| p.cmd.iter().find_map(|a| a.strip_prefix(key));
    if let Some(kind) = flag("--type=") {
        let sub = flag("--utility-sub-type=").map(|s| s.split('.').next().unwrap_or(s));
        return match sub {
            Some(sub) => format!("{} · {kind} · {sub}", p.name),
            None => format!("{} · {kind}", p.name),
        };
    }
    let args: Vec<&str> = p
        .cmd
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            if a.len() > 24 {
                a.rsplit('/').next().unwrap_or(a)
            } else {
                a.as_str()
            }
        })
        .take(4)
        .collect();
    if args.is_empty() {
        p.name.clone()
    } else {
        format!("{} {}", p.name, args.join(" "))
    }
}
