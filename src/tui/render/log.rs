use crate::card::LogEntry;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, entries: &[LogEntry]) {
    let items: Vec<ListItem> = entries
        .iter()
        .map(|e| {
            let mut spans = Vec::new();
            if let Some(ts) = e.ts {
                spans.push(Span::raw(ts.format("%H:%M:%S ").to_string()));
            }
            if let Some(level) = &e.level {
                let color = match level.to_ascii_uppercase().as_str() {
                    "ERROR" | "ERR" => Color::Red,
                    "WARN" | "WARNING" => Color::Yellow,
                    "INFO" => Color::Cyan,
                    _ => Color::Gray,
                };
                spans.push(Span::styled(format!("{level:<5} "), Style::default().fg(color)));
            }
            if let Some(src) = &e.source {
                spans.push(Span::styled(format!("[{src}] "), Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::raw(e.msg.clone()));
            ListItem::new(Line::from(spans))
        })
        .collect();
    f.render_widget(List::new(items), area);
}
