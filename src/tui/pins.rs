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
