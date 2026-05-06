pub mod chart;
pub mod log;
pub mod question;
pub mod status;
pub mod table;
pub mod text;
// Future: tree, diff, image, progress.

use crate::card::{Card, CardKind};
use ratatui::layout::Rect;
use ratatui::Frame;

pub fn render_body(f: &mut Frame, area: Rect, card: &Card) {
    match &card.kind {
        CardKind::Text { content, format, language } => {
            text::render(f, area, content, *format, language.as_deref())
        }
        CardKind::Log { entries } => log::render(f, area, entries),
        CardKind::Status { fields } => status::render(f, area, fields),
        CardKind::Question { question, options, context } => {
            question::render(f, area, question, options, context.as_deref())
        }
        CardKind::Table { columns, rows } => table::render(f, area, columns, rows),
        CardKind::Chart { kind, series, x_label, y_label } => chart::render(
            f, area, *kind, series, x_label.as_deref(), y_label.as_deref(),
        ),
        // Other variants render a placeholder until later tasks fill them in.
        _ => {
            use ratatui::text::Line;
            use ratatui::widgets::Paragraph;
            let kind = match &card.kind {
                CardKind::Tree { .. } => "tree",
                CardKind::Diff { .. } => "diff",
                CardKind::Image { .. } => "image",
                CardKind::Progress { .. } => "progress",
                _ => "?",
            };
            let p = Paragraph::new(Line::from(format!(
                "[{kind} renderer not yet implemented]"
            )));
            f.render_widget(p, area);
        }
    }
}
