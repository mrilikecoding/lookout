use crate::card::Card;
use crate::tui::render::render_body;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// What the caller wants the pin region to render. Carries focus + zoom state
/// so the renderer doesn't have to know about FocusRegion.
pub struct PinView<'a> {
    pub pins: &'a [(String, Card)],
    /// Focused pin index in `pins` (pre-zoom). `None` means no pin is focused
    /// (e.g. focus is on the feed).
    pub focused: Option<usize>,
    /// If set, the pin with this slot name fills the whole region.
    pub zoomed: Option<&'a str>,
}

pub fn render(f: &mut Frame, area: Rect, view: PinView) {
    // Empty state.
    if view.pins.is_empty() {
        let outer = Block::default().borders(Borders::ALL).title("Pinned");
        let inner = outer.inner(area);
        f.render_widget(outer, area);
        let p = Paragraph::new("(no pinned cards) — agent will pin its key signals here")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return;
    }

    // Zoomed mode: a single pin fills the area.
    if let Some(slot) = view.zoomed {
        if let Some((_, card)) = view.pins.iter().find(|(s, _)| s == slot) {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!("▾ {slot}"))
                .border_style(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD));
            let inner = block.inner(area);
            f.render_widget(block, area);
            render_body(f, inner, card);
            return;
        }
        // Zoomed slot was removed — fall through to grid.
    }

    // Grid mode: lay out pins in a responsive column count.
    let cols = layout_columns(area.width).max(1);
    let n = view.pins.len();
    let rows = n.div_ceil(cols);

    let row_constraints: Vec<Constraint> = (0..rows).map(|_| Constraint::Min(5)).collect();
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for row_idx in 0..rows {
        let col_constraints: Vec<Constraint> =
            (0..cols).map(|_| Constraint::Ratio(1, cols as u32)).collect();
        let cell_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_areas[row_idx]);

        for col_idx in 0..cols {
            let pin_idx = row_idx * cols + col_idx;
            if pin_idx >= n {
                break;
            }
            let (slot, card) = &view.pins[pin_idx];
            let is_focused = view.focused == Some(pin_idx);
            let border_style = if is_focused {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::Magenta)
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title(slot.clone())
                .border_style(border_style);
            let inner = block.inner(cell_areas[col_idx]);
            f.render_widget(block, cell_areas[col_idx]);
            render_body(f, inner, card);
        }
    }
}

/// Returns the number of pin columns to render at the given terminal width.
/// Thresholds match the design: 1 col below 80, 2 cols 80–119, 3 cols ≥120.
pub fn layout_columns(width: u16) -> usize {
    if width < 80 {
        1
    } else if width < 120 {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_column_below_80() {
        assert_eq!(layout_columns(0), 1);
        assert_eq!(layout_columns(79), 1);
    }

    #[test]
    fn two_columns_at_80_through_119() {
        assert_eq!(layout_columns(80), 2);
        assert_eq!(layout_columns(100), 2);
        assert_eq!(layout_columns(119), 2);
    }

    #[test]
    fn three_columns_at_120_and_up() {
        assert_eq!(layout_columns(120), 3);
        assert_eq!(layout_columns(200), 3);
        assert_eq!(layout_columns(u16::MAX), 3);
    }
}
