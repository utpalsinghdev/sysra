//! The graph row: CPU history with per-core bars, memory + storage, network.

use super::{bold, dim, dot_title, faint, fmt, graph, panel};
use crate::app::App;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub fn row(f: &mut Frame, r: Rect, app: &App) {
    let [cpu, mem, net] = Layout::horizontal([
        Constraint::Fill(2),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .spacing(1)
    .areas(r);
    draw_cpu(f, cpu, app);
    draw_memory(f, mem, app);
    draw_network(f, net, app);
}

fn draw_cpu(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let s = &app.sampler;
    let mut title = dot_title(t, "cpu", t.severity(s.cpu));
    title.push(Span::raw(" "));
    title.push(bold(format!("{:.1}%", s.cpu), t));
    let right = dim(
        format!("{} cores / {:.2} ghz", s.core_count(), s.freq_ghz),
        t,
    );
    let inner = panel(f.buffer_mut(), r, app, title, right);
    if inner.height == 0 {
        return;
    }

    // Per-core pill bars in up to 6 columns, using at most half the height. When that
    // can't fit every core legibly, fall back to one row of tiny per-core blocks.
    let n = s.cores.len().max(1);
    let max_cols = (inner.width / 20).clamp(1, 6) as usize;
    let core_rows = n.div_ceil(max_cols);
    let compact = core_rows > (inner.height as usize / 2).max(1);
    let core_rows = if compact { 1 } else { core_rows };
    let cols = n.div_ceil(core_rows);
    let graph_h = inner.height.saturating_sub(core_rows as u16 + 1);

    let samples: Vec<f64> = s.history.cpu.iter().copied().collect();
    let g = Rect {
        height: graph_h,
        ..inner
    };
    // Scale to recent peaks (min 10%) so an idle machine still shows its texture.
    let visible = &samples[samples.len().saturating_sub(g.width as usize * 2)..];
    let peak = visible.iter().copied().fold(10.0, f64::max);
    graph::line(
        f.buffer_mut(),
        g,
        &samples,
        (peak * 1.15).min(100.0),
        t.accent,
        t.tint(t.accent),
    );

    let bars = Rect {
        y: inner.y + graph_h + 1,
        height: core_rows as u16,
        ..inner
    };
    if compact {
        let mut spans = vec![faint("cores ", t)];
        spans.extend(graph::sparkline(
            &s.cores,
            n.min(bars.width as usize - 6),
            100.0,
            t.cpu_low,
            t.cpu_high,
        ));
        Line::from(spans).render(bars, f.buffer_mut());
        return;
    }
    let cell_w = (bars.width as usize / cols.max(1)).max(12);
    let buf = f.buffer_mut();
    for (i, usage) in s.cores.iter().enumerate() {
        let (row, col) = (i / cols, i % cols);
        let x = bars.x + (col * cell_w) as u16;
        // Index (3) + bar + percent (6) + gutter (3).
        let bar_w = cell_w.saturating_sub(12);
        let mut spans = vec![faint(format!("{i:<3}"), t)];
        let color = if *usage >= 70.0 {
            t.severity(*usage)
        } else {
            t.accent
        };
        spans.extend(graph::bar(bar_w, usage / 100.0, color, t.track()));
        spans.push(Span::styled(
            format!("{:>5.0}%", usage),
            ratatui::style::Style::new().fg(if *usage >= 50.0 { t.text } else { t.dim }),
        ));
        Line::from(spans).render(Rect::new(x, bars.y + row as u16, cell_w as u16, 1), buf);
    }
}

fn draw_memory(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let s = &app.sampler;
    let pct = s.mem_used as f64 / s.mem_total.max(1) as f64 * 100.0;
    let inner = panel(
        f.buffer_mut(),
        r,
        app,
        dot_title(t, "memory", t.severity(pct)),
        bold(format!("{pct:.0}%"), t),
    );
    if inner.height == 0 {
        return;
    }
    let w = inner.width as usize;
    let total = s.mem_total.max(1) as f64;
    let kv = |k: &str, v: String| {
        let pad = w.saturating_sub(k.len() + v.chars().count());
        Line::from(vec![
            dim(k.to_string(), t),
            Span::raw(" ".repeat(pad)),
            Span::raw(v),
        ])
    };
    let mut lines = vec![
        Line::from(graph::stacked(
            w,
            &[
                (s.mem_used as f64 / total, t.mem),
                (s.mem_cache as f64 / total, t.cache),
            ],
            t.track(),
        )),
        kv("used", fmt::bytes(s.mem_used)),
        kv("cache", fmt::bytes(s.mem_cache)),
        kv(
            "swap",
            format!("{} / {}", fmt::bytes(s.swap_used), fmt::bytes(s.swap_total)),
        ),
    ];
    let room = (inner.height as usize).saturating_sub(lines.len() + 1);
    if room > 0 && !s.mounts.is_empty() {
        lines.push(Line::default());
        let label_w = 9.min(w / 3);
        for m in s.mounts.iter().take(room) {
            let frac = m.used as f64 / m.total.max(1) as f64;
            let mut spans = vec![dim(fmt::fit(&m.path, label_w), t), Span::raw(" ")];
            let color = if frac >= 0.7 {
                t.severity(frac * 100.0)
            } else {
                t.mem
            };
            spans.extend(graph::bar(
                w.saturating_sub(label_w + 7),
                frac,
                color,
                t.track(),
            ));
            spans.push(Span::raw(format!("{:>5.0}%", frac * 100.0)));
            lines.push(Line::from(spans));
        }
    }
    Paragraph::new(lines).render(inner, f.buffer_mut());
}

fn draw_network(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let s = &app.sampler;
    let inner = panel(
        f.buffer_mut(),
        r,
        app,
        dot_title(t, "network", t.rx),
        bold(fmt::rate(s.rx_rate + s.tx_rate), t),
    );
    if inner.height < 4 {
        return;
    }
    let buf = f.buffer_mut();
    let summary = format!("rx {} / tx {}", fmt::rate(s.rx_rate), fmt::rate(s.tx_rate));
    Line::from(dim(summary, t)).render(Rect { height: 1, ..inner }, buf);

    // rx and tx share one scale so their heights compare honestly.
    let rx = smooth(&s.history.rx);
    let tx = smooth(&s.history.tx);
    let max = rx.iter().chain(&tx).copied().fold(4096.0, f64::max) * 1.15;
    let graphs_h = inner.height - 2;
    let rx_h = graphs_h.div_ceil(2);
    let label_w = 4;
    let plot =
        |y: u16, h: u16| Rect::new(inner.x + label_w, y, inner.width.saturating_sub(label_w), h);
    for (label, data, color, y, h) in [
        ("rx", &rx, t.rx, inner.y + 1, rx_h),
        ("tx", &tx, t.tx, inner.y + 1 + rx_h, graphs_h - rx_h),
    ] {
        if h == 0 {
            continue;
        }
        Line::from(dim(label, t)).render(Rect::new(inner.x, y + h / 2, label_w, 1), buf);
        graph::line(buf, plot(y, h), data, max, color, t.tint(color));
    }

    let iface = if s.iface.is_empty() {
        String::new()
    } else {
        format!("{} / ", s.iface)
    };
    let total = format!("{iface}{} total", fmt::bytes(s.rx_total + s.tx_total));
    Line::from(faint(total, t)).render(
        Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        },
        buf,
    );
}

/// Exponential moving average: bursty traffic reads as steps instead of noise.
fn smooth(samples: &std::collections::VecDeque<f64>) -> Vec<f64> {
    let mut acc = None;
    samples
        .iter()
        .map(|&v| {
            let next = acc.map_or(v, |a: f64| a + (v - a) * 0.35);
            acc = Some(next);
            next
        })
        .collect()
}
