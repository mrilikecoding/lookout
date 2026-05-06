use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(
    f: &mut Frame,
    area: Rect,
    question: &str,
    options: &[String],
    context: Option<&str>,
) {
    let mut lines = Vec::new();
    if let Some(ctx) = context {
        lines.push(Line::from(Span::styled(
            ctx.to_string(),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        question.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if !options.is_empty() {
        lines.push(Line::raw(""));
        for (i, opt) in options.iter().enumerate() {
            lines.push(Line::from(format!("  {}. {opt}", (b'A' + i as u8) as char)));
        }
    }
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Question")
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(p, area);
}
