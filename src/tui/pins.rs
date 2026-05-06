use crate::card::Card;
use crate::tui::render::render_body;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, pins: &[(String, Card)]) {
    let outer = Block::default().borders(Borders::ALL).title("Pinned");
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    if pins.is_empty() {
        let p = Paragraph::new("(no pins)").style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return;
    }

    // Allocate vertical slots, one per pin (cap at 6 visible).
    let visible: Vec<&(String, Card)> = pins.iter().take(6).collect();
    let constraints: Vec<Constraint> =
        visible.iter().map(|_| Constraint::Min(3)).collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);
    for (idx, (slot, card)) in visible.iter().enumerate() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(slot.clone())
            .border_style(Style::default().fg(Color::Magenta));
        let body_area = block.inner(chunks[idx]);
        f.render_widget(block, chunks[idx]);
        render_body(f, body_area, card);
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
