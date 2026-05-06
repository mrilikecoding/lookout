use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use similar::{ChangeTag, TextDiff};

pub fn render(f: &mut Frame, area: Rect, before: &str, after: &str, _language: Option<&str>) {
    let diff = TextDiff::from_lines(before, after);
    let mut lines = Vec::new();
    for change in diff.iter_all_changes() {
        let (prefix, color) = match change.tag() {
            ChangeTag::Delete => ("-", Color::Red),
            ChangeTag::Insert => ("+", Color::Green),
            ChangeTag::Equal => (" ", Color::Reset),
        };
        let text = change.value().trim_end_matches('\n').to_string();
        lines.push(Line::from(vec![Span::styled(
            format!("{prefix} {text}"),
            Style::default().fg(color),
        )]));
    }
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Diff"));
    f.render_widget(p, area);
}
