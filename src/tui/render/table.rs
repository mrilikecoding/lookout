use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;
use serde_json::Value as JsonValue;

pub fn render(
    f: &mut Frame,
    area: Rect,
    columns: &[String],
    rows: &[Vec<JsonValue>],
) {
    let header = Row::new(
        columns.iter().map(|c| Cell::from(c.clone())),
    )
    .style(Style::default().add_modifier(Modifier::BOLD));
    let body: Vec<Row> = rows
        .iter()
        .map(|r| {
            Row::new(
                r.iter().map(|v| Cell::from(json_to_cell(v))),
            )
        })
        .collect();
    let widths: Vec<ratatui::layout::Constraint> = columns
        .iter()
        .map(|_| ratatui::layout::Constraint::Length(20))
        .collect();
    let table = Table::new(body, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Table"));
    f.render_widget(table, area);
}

fn json_to_cell(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn renders_header_and_rows() {
        let cols = vec!["id".into(), "name".into()];
        let rows = vec![
            vec![JsonValue::from(1), JsonValue::from("alpha")],
            vec![JsonValue::from(2), JsonValue::from("beta")],
        ];
        let mut term = Terminal::new(TestBackend::new(60, 6)).unwrap();
        let area = ratatui::layout::Rect::new(0, 0, 60, 6);
        term.draw(|f| render(f, area, &cols, &rows)).unwrap();
        let snap = term.backend().buffer();
        let snap_str: String = snap
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();
        assert!(snap_str.contains("id"));
        assert!(snap_str.contains("alpha"));
    }
}
