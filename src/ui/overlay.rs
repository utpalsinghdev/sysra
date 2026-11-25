//! Footer key hints, flash messages, and the help / kill-confirm dialogs.

use super::{bold, dim, faint};
use crate::app::{App, Mode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};
use std::time::Duration;

const KEYS: &[(&str, &str)] = &[
    ("↑↓", "select"),
    ("⏎", "expand"),
    ("/", "filter"),
    ("s", "sort"),
    ("x", "kill"),
    ("t", "theme"),
    ("a", "agents"),
    ("?", "help"),
    ("q", "quit"),
];

pub fn footer(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let key = |k: &str| Span::styled(k.to_string(), Style::new().fg(t.accent));

    let left: Line = match &app.mode {
        Mode::Filter => Line::from(vec![
            key("/ "),
            bold(app.filter.clone(), t),
            key("▏"),
            faint("  enter keep · esc clear", t),
        ]),
        _ => match &app.flash {
            Some((msg, at)) if at.elapsed() < Duration::from_secs(4) => {
                Line::from(Span::styled(msg.clone(), Style::new().fg(t.warn)))
            }
            _ => {
                // Only whole hints that fit beside the right-hand status.
                let room = r.width.saturating_sub(right_width(app) + 2) as usize;
                let mut used = 0;
                let spans = KEYS
                    .iter()
                    .take_while(|(k, v)| {
                        used += k.chars().count() + v.len() + 4;
                        used <= room
                    })
                    .flat_map(|(k, v)| [key(k), faint(format!(" {v}   "), t)]);
                Line::from(spans.collect::<Vec<_>>())
            }
        },
    };
    let right = right_status(app);
    let rw = right.width() as u16;
    let [l, rr] = Layout::horizontal([Constraint::Min(0), Constraint::Length(rw)]).areas(r);
    left.render(l, f.buffer_mut());
    right.render(rr, f.buffer_mut());
}

fn right_status(app: &App) -> Line<'static> {
    let t = &app.theme;
    Line::from(vec![
        faint(
            format!("{} · {:.1}s  ", t.name, app.cfg.refresh_ms as f64 / 1000.0),
            t,
        ),
        dim(concat!("sysra ", env!("CARGO_PKG_VERSION")), t),
    ])
}

fn right_width(app: &App) -> u16 {
    right_status(app).width() as u16
}

pub fn modal(f: &mut Frame, screen: Rect, app: &App) {
    let t = &app.theme;
    let (title, lines): (&str, Vec<Line>) = match &app.mode {
        Mode::Help => ("keys", help_lines(app)),
        Mode::Confirm { name, pids } => (
            "terminate",
            vec![
                Line::from(vec![dim("Send SIGTERM to ", t), bold(name.clone(), t)]),
                Line::from(dim(
                    format!(
                        "{} process{}",
                        pids.len(),
                        if pids.len() == 1 { "" } else { "es" }
                    ),
                    t,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("y", Style::new().fg(t.crit)),
                    faint(" confirm    ", t),
                    Span::styled("any key", Style::new().fg(t.accent)),
                    faint(" cancel", t),
                ]),
            ],
        ),
        _ => return,
    };
    let w = lines.iter().map(Line::width).max().unwrap_or(20) as u16 + 6;
    let h = lines.len() as u16 + 2;
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(w) / 2,
        screen.y + screen.height.saturating_sub(h) / 2,
        w.min(screen.width),
        h.min(screen.height),
    );
    Clear.render(area, f.buffer_mut());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.accent))
        .title(Span::styled(format!(" {title} "), Style::new().fg(t.text)))
        .style(Style::new().bg(t.panel).fg(t.text));
    let inner = block.inner(area);
    block.render(area, f.buffer_mut());
    Paragraph::new(lines).render(
        inner.inner(ratatui::layout::Margin {
            horizontal: 2,
            vertical: 0,
        }),
        f.buffer_mut(),
    );
}

fn help_lines(app: &App) -> Vec<Line<'static>> {
    let t = &app.theme;
    let rows: &[(&str, &str)] = &[
        ("↑ ↓  j k", "move selection"),
        ("pgup pgdn  g G", "jump"),
        ("enter  space  → ←", "expand app into its processes"),
        ("/", "filter by app, process or port"),
        ("s", "cycle sort: cpu, mem, io, procs, name"),
        ("x  del", "terminate selected app or process"),
        ("t  T", "next / previous theme"),
        ("a", "toggle agents panel"),
        ("q  esc", "quit"),
    ];
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:<20}"), Style::new().fg(t.accent)),
                dim(v.to_string(), t),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines.push(Line::from(faint(
        format!("config  {}", crate::config::default_config_path().display()),
        t,
    )));
    lines
}
