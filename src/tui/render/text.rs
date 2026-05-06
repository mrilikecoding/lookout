use crate::card::TextFormat;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(
    f: &mut Frame,
    area: Rect,
    content: &str,
    _format: TextFormat,
    _language: Option<&str>,
) {
    // V1: render plain wrapped text. Markdown / syntax highlighting are a follow-up.
    let p = Paragraph::new(content.to_string()).wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(p, area);
}
