//! AI agents panel: live activity, token burn and plan limits per agent.

use super::{dim, dot_title, faint, fmt, graph, panel};
use crate::agents::{AgentReport, Limit};
use crate::app::App;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Column widths; optional columns drop out right-to-left priority when space is short.
const NAME: usize = 15;
const ACTIVITY: usize = 16;
const LIMIT: usize = 28;
const TODAY: usize = 9;
const WEEK: usize = 9;
const SPARK: usize = 9;
const COST: usize = 9;

struct Columns {
    activity: bool,
    limits: usize,
    /// Width of one limit column; grows with spare room.
    limit_w: usize,
    spark: bool,
    cost: bool,
}

impl Columns {
    fn fit(width: usize) -> Self {
        let mut c = Self {
            activity: true,
            limits: 2,
            limit_w: LIMIT,
            spark: true,
            cost: true,
        };
        let need = |c: &Self| {
            NAME + TODAY
                + WEEK
                + if c.activity { ACTIVITY } else { 0 }
                + c.limits * LIMIT
                + if c.spark { SPARK } else { 0 }
                + if c.cost { COST } else { 0 }
        };
        let drops: [fn(&mut Self); 4] = [
            |c| c.cost = false,
            |c| c.activity = false,
            |c| c.spark = false,
            |c| c.limits = 1,
        ];
        for drop in drops {
            if need(&c) <= width {
                break;
            }
            drop(&mut c);
        }
        let spare = width.saturating_sub(need(&c));
        c.limit_w += (spare / c.limits.max(1)).min(16);
        c
    }
}

pub fn draw(f: &mut Frame, r: Rect, app: &App) {
    let t = &app.theme;
    let inner = panel(
        f.buffer_mut(),
        r,
        app,
        dot_title(t, "agents", t.accent),
        dim("tokens · plan limits", t),
    );
    if inner.height == 0 {
        return;
    }
    let Some(reports) = &app.agent_reports else {
        Paragraph::new(faint("reading agent logs…", t)).render(inner, f.buffer_mut());
        return;
    };
    if reports.is_empty() {
        let msg = "no supported agents found (claude, codex, pi, opencode, hermes)";
        Paragraph::new(faint(msg, t)).render(inner, f.buffer_mut());
        return;
    }
    let cols = Columns::fit(inner.width as usize);
    let now = chrono::Local::now().timestamp();

    let mut head = vec![faint(fmt::fit("agent", NAME), t)];
    if cols.activity {
        head.push(faint(fmt::fit("running", ACTIVITY), t));
    }
    head.push(faint(
        fmt::fit("plan limits · resets in", cols.limit_w * cols.limits),
        t,
    ));
    head.push(faint(fmt::rfit("today", TODAY), t));
    head.push(faint(fmt::rfit("7 days", WEEK), t));
    if cols.spark {
        head.push(faint(fmt::rfit("trend", SPARK), t));
    }
    if cols.cost {
        head.push(faint(fmt::rfit("cost", COST), t));
    }
    let mut lines = vec![Line::from(head)];
    lines.extend(reports.iter().map(|rep| agent_line(rep, &cols, app, now)));
    Paragraph::new(lines).render(inner, f.buffer_mut());
}

fn agent_line(rep: &AgentReport, cols: &Columns, app: &App, now: i64) -> Line<'static> {
    let t = &app.theme;
    let running = app.app_by_name(rep.id.label());
    let mut spans = vec![
        Span::styled(
            if running.is_some() { "● " } else { "○ " },
            Style::new().fg(if running.is_some() { t.ok } else { t.faint }),
        ),
        Span::styled(
            fmt::fit(rep.id.label(), NAME - 2),
            Style::new().fg(t.text).add_modifier(Modifier::BOLD),
        ),
    ];
    if cols.activity {
        let text = running.map_or_else(
            || "idle".to_string(),
            |a| format!("{:.1}% · {}", a.cpu, fmt::bytes(a.mem)),
        );
        spans.push(Span::styled(
            fmt::fit(&text, ACTIVITY),
            Style::new().fg(if running.is_some() { t.dim } else { t.faint }),
        ));
    }

    let width = cols.limit_w * cols.limits;
    if rep.limits.is_empty() {
        let note = rep.note.clone().unwrap_or_else(|| "no plan limits".into());
        spans.push(faint(fmt::fit(&note, width), t));
    } else {
        for i in 0..cols.limits {
            match rep.limits.get(i) {
                Some(l) => spans.extend(limit_spans(l, cols.limit_w, t, now)),
                None => spans.push(Span::raw(" ".repeat(cols.limit_w))),
            }
        }
    }

    let tok = |n: u64, w: usize| {
        let style = Style::new().fg(if n > 0 { t.text } else { t.faint });
        Span::styled(fmt::rfit(&fmt::tokens(n), w), style)
    };
    spans.push(tok(rep.tokens_today, TODAY));
    spans.push(tok(rep.tokens_week, WEEK));
    if cols.spark {
        let daily: Vec<f64> = rep.daily.iter().map(|&d| d as f64).collect();
        let max = daily.iter().copied().fold(1.0, f64::max);
        spans.push(Span::raw("  "));
        spans.extend(graph::sparkline(&daily, SPARK - 2, max, t.faint, t.accent));
    }
    if cols.cost {
        let cost = rep
            .cost_today
            .map_or_else(|| "—".into(), |c| format!("${c:.2}"));
        spans.push(dim(fmt::rfit(&cost, COST), t));
    }
    Line::from(spans)
}

/// `5h ━━━━────── 29%  3h12m` in exactly `width` columns.
fn limit_spans(l: &Limit, width: usize, t: &Theme, now: i64) -> Vec<Span<'static>> {
    let bar_w = width - 20;
    // A snapshot from before the window rolled over no longer applies.
    let expired = l.resets_at.is_some_and(|ts| ts <= now);
    let l = &Limit {
        label: l.label.clone(),
        used_pct: if expired { 0.0 } else { l.used_pct },
        resets_at: if expired { None } else { l.resets_at },
    };
    let left = (100.0 - l.used_pct).max(0.0);
    let mut spans = vec![dim(fmt::fit(&l.label, 5), t)];
    spans.extend(graph::bar(
        bar_w,
        l.used_pct / 100.0,
        t.severity(l.used_pct),
        t.track(),
    ));
    spans.push(Span::styled(
        format!(" {:>3.0}%", l.used_pct),
        Style::new().fg(if left < 20.0 { t.crit } else { t.text }),
    ));
    let reset = l
        .resets_at
        .map(|ts| format!("  {}", fmt::until(ts, now)))
        .unwrap_or_default();
    spans.push(faint(fmt::fit(&reset, width - 5 - bar_w - 5), t));
    spans
}
