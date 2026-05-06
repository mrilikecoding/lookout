use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::app::UiSnapshot;

pub fn render(f: &mut Frame, area: Rect, snap: &UiSnapshot) {
    let session_count = snap
        .feed
        .iter()
        .map(|c| c.session.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let line = Line::from(vec![
        Span::styled("lookout ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("— "),
        Span::styled(&snap.url, Style::default().add_modifier(Modifier::UNDERLINED)),
        Span::raw(format!(" — {session_count} sessions seen — feed: {}", snap.feed.len())),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
