//! Top of the screen: host line with clock, then the five summary cards.

use super::{bold, dim, dot_title, faint, fmt, graph, panel};
use crate::app::App;
use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub fn header(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let s = &app.sampler;
    let now = Local::now();
    let [left, right] = Layout::horizontal([Constraint::Min(10), Constraint::Length(16)]).areas(r);

    let top = Line::from(vec![
        Span::styled("● ", Style::new().fg(t.accent)),
        Span::styled(
            s.host.clone(),
            Style::new().fg(t.text).add_modifier(Modifier::BOLD),
        ),
        faint("  ·  ", t),
        dim(s.os.clone(), t),
    ]);
    let kernel = s.kernel.split('-').next().unwrap_or(&s.kernel);
    let bottom = Line::from(vec![
        faint("  ", t),
        dim(format!("kernel {kernel}"), t),
        faint("  /  ", t),
        dim(format!("up {}", fmt::duration(s.uptime)), t),
        faint("  /  ", t),
        dim(
            format!("{} apps · {} processes", app.apps.len(), s.procs.len()),
            t,
        ),
    ]);
    Paragraph::new(vec![top, bottom]).render(left, f.buffer_mut());

    let clock = vec![
        Line::from(bold(now.format("%H:%M:%S").to_string(), t)).right_aligned(),
        Line::from(dim(now.format("%a %d %b").to_string(), t)).right_aligned(),
    ];
    Paragraph::new(clock).render(right, f.buffer_mut());
}

/// One summary card: a big value, a detail line and an optional sparkline.
struct Card {
    label: &'static str,
    value: String,
    sub: String,
    /// Drives the title dot's severity color.
    pct: f64,
    /// History and its scale maximum.
    spark: Option<(Vec<f64>, f64)>,
}

pub fn cards(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let s = &app.sampler;
    let h = &s.history;
    let cols = Layout::horizontal([Constraint::Ratio(1, 5); 5])
        .spacing(1)
        .split(r);
    let mem_pct = s.mem_used as f64 / s.mem_total.max(1) as f64 * 100.0;
    let load_pct = s.load[0] / s.core_count().max(1) as f64 * 100.0;
    let net: Vec<f64> = h.rx.iter().zip(h.tx.iter()).map(|(a, b)| a + b).collect();
    let net_max = net.iter().copied().fold(4096.0, f64::max);
    let cpu: Vec<f64> = h.cpu.iter().copied().collect();
    let mem: Vec<f64> = h.mem.iter().copied().collect();

    let specs = [
        Card {
            label: "cpu",
            value: format!("{:.1}%", s.cpu),
            sub: format!("{} cores / {:.2} ghz", s.core_count(), s.freq_ghz),
            pct: s.cpu,
            spark: Some((cpu, 100.0)),
        },
        Card {
            label: "memory",
            value: format!("{mem_pct:.0}%"),
            sub: format!("{} / {}", fmt::bytes(s.mem_used), fmt::bytes(s.mem_total)),
            pct: mem_pct,
            spark: Some((mem, 100.0)),
        },
        Card {
            label: "network",
            value: fmt::rate(s.rx_rate + s.tx_rate),
            sub: format!("↓ {}  ↑ {}", fmt::rate(s.rx_rate), fmt::rate(s.tx_rate)),
            pct: 0.0,
            spark: Some((net, net_max)),
        },
        Card {
            label: "disk",
            value: fmt::rate(s.read_rate + s.write_rate),
            sub: format!(
                "r {}  w {}",
                fmt::rate(s.read_rate),
                fmt::rate(s.write_rate)
            ),
            pct: 0.0,
            spark: None,
        },
        Card {
            label: "load",
            value: format!("{:.2}", s.load[0]),
            sub: format!("5m {:.2} / 15m {:.2}", s.load[1], s.load[2]),
            pct: load_pct,
            spark: None,
        },
    ];

    for (
        area,
        Card {
            label,
            value,
            sub,
            pct,
            spark,
        },
    ) in cols.iter().zip(specs)
    {
        let inner = panel(
            f.buffer_mut(),
            *area,
            app,
            dot_title(t, label, t.severity(pct)),
            "",
        );
        if inner.height < 2 {
            continue;
        }
        let value_w = value.chars().count() as u16 + 2;
        let buf = f.buffer_mut();
        Paragraph::new(Line::from(bold(value, t))).render(Rect { height: 1, ..inner }, buf);
        if let Some((samples, max)) = spark {
            let w = inner.width.saturating_sub(value_w).min(16) as usize;
            if w >= 4 {
                let line = Line::from(graph::sparkline(&samples, w, max, t.faint, t.accent));
                let x = inner.right() - w as u16;
                line.render(Rect::new(x, inner.y, w as u16, 1), buf);
            }
        }
        let sub = fmt::fit(&sub, inner.width as usize);
        Paragraph::new(Line::from(dim(sub, t))).render(
            Rect {
                y: inner.y + 1,
                height: 1,
                ..inner
            },
            buf,
        );
    }
}
