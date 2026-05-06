use crate::card::{Card, CardKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

pub struct FeedView<'a> {
    pub cards: &'a [Card],
    pub focused: usize,
}

pub fn render(f: &mut Frame, area: Rect, view: FeedView) {
    let items: Vec<ListItem> = view
        .cards
        .iter()
        .rev() // newest at top
        .map(|c| {
            let kind = card_kind_label(&c.kind);
            let title = c.title.as_deref().unwrap_or("");
            let ts = c.created_at.format("%H:%M:%S").to_string();
            let line = Line::from(vec![
                Span::raw(format!("[{ts}] ")),
                Span::styled(format!("●{} ", c.session), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{kind:<8} "), Style::default().fg(Color::Blue)),
                Span::raw("▸ "),
                Span::raw(title.to_string()),
            ]);
            ListItem::new(line)
        })
        .collect();
    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(view.focused.min(items.len().saturating_sub(1))));
    }
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Feed"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut list_state);
}

pub fn card_kind_label(k: &CardKind) -> &'static str {
    match k {
        CardKind::Text { .. } => "text",
        CardKind::Table { .. } => "table",
        CardKind::Chart { .. } => "chart",
        CardKind::Tree { .. } => "tree",
        CardKind::Diff { .. } => "diff",
        CardKind::Log { .. } => "log",
        CardKind::Image { .. } => "image",
        CardKind::Progress { .. } => "progress",
        CardKind::Status { .. } => "status",
        CardKind::Question { .. } => "question",
    }
}

/// Render the most recent feed cards as a compact ticker — one card per line,
/// newest at top, capped at `max_lines`. No focus highlight; this is glance-only.
pub fn render_compact(f: &mut Frame, area: Rect, cards: &[Card], max_lines: usize) {
    let items: Vec<ListItem> = cards
        .iter()
        .rev() // newest first
        .take(max_lines)
        .map(|c| {
            let kind = card_kind_label(&c.kind);
            let title = c.title.as_deref().unwrap_or("");
            let ts = c.created_at.format("%H:%M:%S").to_string();
            let line = Line::from(vec![
                Span::raw(format!("[{ts}] ")),
                Span::styled(
                    format!("●{} ", c.session),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{kind:<8} "),
                    Style::default().fg(Color::Blue),
                ),
                Span::raw("▸ "),
                Span::raw(title.to_string()),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Feed (g to expand)"));
    f.render_widget(list, area);
}
