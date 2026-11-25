//! Drawing primitives: braille line graphs, pill bars and block sparklines.

use crate::theme::lerp;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// Braille dot bits for a cell, indexed `[column][row from top]`.
const DOTS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// A thin line graph, newest sample at the right edge, two samples per cell.
///
/// Consecutive samples are joined vertically so spikes read as one continuous stroke.
/// Cells fully under the line get `fill` as background: a soft area tint rather than
/// a wall of dots.
pub fn line(
    buf: &mut Buffer,
    r: Rect,
    samples: &[f64],
    max: f64,
    stroke: Color,
    fill: Option<Color>,
) {
    if r.width == 0 || r.height == 0 {
        return;
    }
    let rows = r.height as usize * 4;
    let slots = r.width as usize * 2;
    let visible = &samples[samples.len().saturating_sub(slots)..];
    let offset = slots - visible.len();
    // Dot row from the top for a value; the baseline is the bottom dot row.
    let level = |v: f64| -> usize {
        let t = (v / max.max(f64::EPSILON)).clamp(0.0, 1.0);
        rows - 1 - (t * (rows - 1) as f64).round() as usize
    };

    let mut bits = vec![0u8; r.width as usize * r.height as usize];
    // Highest point (smallest dot row) touched in each cell column.
    let mut top = vec![rows; r.width as usize];
    let mut prev: Option<usize> = None;
    for (i, &v) in visible.iter().enumerate() {
        let slot = offset + i;
        let (cx, sub) = (slot / 2, slot % 2);
        let y = level(v);
        let (from, to) = match prev {
            Some(p) => (p.min(y), p.max(y)),
            None => (y, y),
        };
        for dy in from..=to {
            bits[(dy / 4) * r.width as usize + cx] |= DOTS[sub][dy % 4];
        }
        top[cx] = top[cx].min(y);
        prev = Some(y);
    }

    for cx in 0..r.width as usize {
        for row in 0..r.height as usize {
            let cell = &mut buf[(r.x + cx as u16, r.y + row as u16)];
            let b = bits[row * r.width as usize + cx];
            if b != 0 {
                cell.set_char(char::from_u32(0x2800 + b as u32).unwrap_or(' '))
                    .set_fg(stroke);
            }
            if let Some(fill) = fill
                && top[cx] < rows
                && row * 4 > top[cx]
            {
                cell.set_bg(fill);
            }
        }
    }
}

/// A pill-shaped progress bar: bright fill on a dim solid track. Tiny non-zero values
/// show as a dot so activity never disappears.
pub fn bar(width: usize, frac: f64, fill: Color, track: Color) -> Vec<Span<'static>> {
    stacked(width, &[(frac, fill)], track)
}

/// Several fills laid end to end (e.g. used memory, then cache) on one track.
pub fn stacked(width: usize, parts: &[(f64, Color)], track: Color) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut used = 0usize;
    let mut acc = 0.0;
    for &(frac, color) in parts {
        let frac = frac.clamp(0.0, 1.0);
        acc = (acc + frac).min(1.0);
        let end = ((acc * width as f64).round() as usize).min(width);
        let cells = end.saturating_sub(used);
        if cells > 0 {
            spans.push(Span::styled("━".repeat(cells), Style::new().fg(color)));
            used += cells;
        } else if frac > 0.0 && used < width && spans.is_empty() {
            spans.push(Span::styled("•", Style::new().fg(color)));
            used += 1;
        }
    }
    if used < width {
        spans.push(Span::styled(
            "━".repeat(width - used),
            Style::new().fg(track),
        ));
    }
    spans
}

/// Block-character sparkline of the last `width` samples, scaled to `max`.
pub fn sparkline(
    samples: &[f64],
    width: usize,
    max: f64,
    low: Color,
    high: Color,
) -> Vec<Span<'static>> {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let start = samples.len().saturating_sub(width);
    let mut spans: Vec<Span> = Vec::with_capacity(width);
    if samples.len() < width {
        spans.push(Span::raw(" ".repeat(width - samples.len())));
    }
    for &v in &samples[start..] {
        let t = (v / max.max(f64::EPSILON)).clamp(0.0, 1.0);
        let i = ((t * 7.0).round() as usize).min(7);
        spans.push(Span::styled(
            BLOCKS[i].to_string(),
            Style::new().fg(lerp(low, high, t)),
        ));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(spans: &[Span]) -> usize {
        spans.iter().map(|s| s.content.chars().count()).sum()
    }

    #[test]
    fn bars_keep_their_width() {
        for w in [1, 5, 10, 13] {
            for f in [0.0, 0.01, 0.05, 0.33, 0.5, 0.99, 1.0, 1.5] {
                assert_eq!(width(&bar(w, f, Color::Red, Color::Gray)), w, "w={w} f={f}");
                let parts = [(f / 2.0, Color::Red), (f / 3.0, Color::Blue)];
                assert_eq!(width(&stacked(w, &parts, Color::Gray)), w);
            }
        }
    }

    #[test]
    fn line_joins_samples_and_fills_below() {
        let r = Rect::new(0, 0, 1, 2);
        let mut buf = Buffer::empty(r);
        // Left sample at the top, right sample at the bottom: a full vertical stroke on
        // the right column, from the top dot row down to the baseline.
        line(&mut buf, r, &[1.0, 0.0], 1.0, Color::Red, Some(Color::Blue));
        assert_eq!(buf[(0, 0)].symbol(), "⢹");
        assert_eq!(buf[(0, 1)].symbol(), "⢸");
        assert_eq!(buf[(0, 1)].bg, Color::Blue);
        assert_eq!(buf[(0, 0)].bg, Color::Reset);
    }
}
