use crate::card::{StatusField, StatusStyle, Trend};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, fields: &[StatusField]) {
    let lines: Vec<Line> = fields
        .iter()
        .map(|fld| {
            let trend = match fld.trend {
                Some(Trend::Up) => " ▲",
                Some(Trend::Down) => " ▼",
                Some(Trend::Flat) => " —",
                None => "",
            };
            let value_color = match fld.style {
                Some(StatusStyle::Good) => Color::Green,
                Some(StatusStyle::Warn) => Color::Yellow,
                Some(StatusStyle::Bad) => Color::Red,
                None => Color::White,
            };
            Line::from(vec![
                Span::styled(
                    format!("{:<10}", fld.label),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(": "),
                Span::styled(fld.value.clone(), Style::default().fg(value_color)),
                Span::raw(trend),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}
