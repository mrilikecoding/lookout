use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

pub fn render(
    f: &mut Frame,
    area: Rect,
    label: &str,
    current: f64,
    total: Option<f64>,
    status: Option<&str>,
) {
    match total {
        Some(t) if t > 0.0 => {
            let ratio = (current / t).clamp(0.0, 1.0);
            let pct = (ratio * 100.0).round() as u16;
            let g = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(label.to_string()))
                .gauge_style(Style::default().fg(Color::Green))
                .percent(pct);
            f.render_widget(g, area);
        }
        _ => {
            let body = format!(
                "{label}\n{}",
                status.unwrap_or("running…")
            );
            f.render_widget(Paragraph::new(body), area);
        }
    }
}
