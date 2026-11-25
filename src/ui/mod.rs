//! Screen layout. Each region lives in its own module; this file decides sizes.

mod agents;
mod apps;
mod fmt;
mod graph;
mod header;
mod overlay;
mod panels;

use crate::app::App;
use crate::config::PanelStyle;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let t = &app.theme;
    Block::new()
        .style(Style::new().bg(t.bg).fg(t.text))
        .render(area, f.buffer_mut());

    let outer = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
    let gap = if app.cfg.panel_style == PanelStyle::Filled {
        1
    } else {
        0
    };
    let agent_rows = app.agent_reports.as_ref().map_or(1, |r| r.len().max(1)) as u16 + 3;

    // Give the apps table at least APPS_MIN rows; shed the less essential sections
    // (summary cards first, then the agents panel, then graph height) to get there.
    const APPS_MIN: u16 = 12;
    let mut graphs = 12u16;
    let mut show_agents = app.show_agents;
    let mut show_cards = true;
    let used = |cards: bool, agents: bool, graphs: u16| {
        let sections = 3 + cards as u16 + agents as u16;
        2 + 1
            + graphs
            + if cards { 4 } else { 0 }
            + if agents { agent_rows } else { 0 }
            + gap * (sections - 1)
    };
    let fits = |c, a, g| used(c, a, g) + APPS_MIN <= outer.height;
    if !fits(show_cards, show_agents, graphs) {
        graphs = 10;
    }
    if !fits(show_cards, show_agents, graphs) {
        show_cards = false;
    }
    if !fits(show_cards, show_agents, graphs) {
        graphs = 8;
    }
    if !fits(show_cards, show_agents, graphs) {
        show_agents = false;
    }

    let mut rows = vec![Constraint::Length(2)];
    if show_cards {
        rows.push(Constraint::Length(4));
    }
    rows.push(Constraint::Length(graphs));
    if show_agents {
        rows.push(Constraint::Length(agent_rows));
    }
    rows.push(Constraint::Min(5));
    rows.push(Constraint::Length(1));
    let areas = Layout::vertical(rows).spacing(gap).split(outer);

    let mut i = 0;
    let mut next = || {
        i += 1;
        areas[i - 1]
    };
    header::header(f, next(), app);
    if show_cards {
        header::cards(f, next(), app);
    }
    panels::row(f, next(), app);
    if show_agents {
        agents::draw(f, next(), app);
    }
    apps::draw(f, next(), app);
    overlay::footer(f, next(), app);
    overlay::modal(f, area, app);
}

/// Paint a panel and return its content area.
///
/// Both styles reserve the same space: a title row on top, one blank/border row at the
/// bottom and two columns of padding, so layouts don't shift when switching styles.
pub fn panel<'a>(
    buf: &mut Buffer,
    r: Rect,
    app: &App,
    title: impl Into<Line<'a>>,
    right: impl Into<Line<'a>>,
) -> Rect {
    let t = &app.theme;
    if r.height < 2 || r.width < 6 {
        return Rect::default();
    }
    match app.cfg.panel_style {
        PanelStyle::Filled => {
            Block::new().style(Style::new().bg(t.panel)).render(r, buf);
        }
        PanelStyle::Outline => {
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(t.border))
                .render(r, buf);
        }
    }
    let title_row = Rect::new(r.x + 2, r.y, r.width.saturating_sub(4), 1);
    let right: Line = right.into();
    let rw = right.width() as u16;
    if rw > 0 && rw + 4 < title_row.width {
        right
            .right_aligned()
            .render(Rect::new(title_row.right() - rw - 1, r.y, rw + 1, 1), buf);
    }
    let mut title: Line = title.into();
    title.spans.insert(0, Span::raw(" "));
    title.spans.push(Span::raw(" "));
    Paragraph::new(title).render(
        Rect::new(
            title_row.x - 1,
            r.y,
            title_row.width.saturating_sub(rw + 1),
            1,
        ),
        buf,
    );
    Rect::new(
        r.x + 2,
        r.y + 1,
        r.width.saturating_sub(4),
        r.height.saturating_sub(2),
    )
}

/// `• label` title with a colored dot.
pub fn dot_title(t: &Theme, label: &str, dot: ratatui::style::Color) -> Vec<Span<'static>> {
    vec![
        Span::styled("• ", Style::new().fg(dot)),
        Span::styled(label.to_string(), Style::new().fg(t.dim)),
    ]
}

pub fn bold(s: impl Into<String>, t: &Theme) -> Span<'static> {
    Span::styled(
        s.into(),
        Style::new().fg(t.text).add_modifier(Modifier::BOLD),
    )
}

pub fn dim(s: impl Into<String>, t: &Theme) -> Span<'static> {
    Span::styled(s.into(), Style::new().fg(t.dim))
}

pub fn faint(s: impl Into<String>, t: &Theme) -> Span<'static> {
    Span::styled(s.into(), Style::new().fg(t.faint))
}
