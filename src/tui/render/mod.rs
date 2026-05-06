pub mod chart;
pub mod diff;
pub mod image;
pub mod log;
pub mod progress;
pub mod question;
pub mod status;
pub mod table;
pub mod text;
pub mod tree;

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
        CardKind::Tree { root } => tree::render(f, area, root),
        CardKind::Diff { before, after, language } => diff::render(f, area, before, after, language.as_deref()),
        CardKind::Image { bytes, mime, .. } => image::render(f, area, bytes, mime.as_deref()),
        CardKind::Progress { label, current, total, status, .. } => {
            progress::render(f, area, label, *current, *total, status.as_deref())
        }
    }
}
