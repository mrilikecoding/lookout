use crate::card::{ChartKind, ChartSeries};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, BarChart, BarGroup, Block, Borders, Chart, Dataset, GraphType, Sparkline};
use ratatui::Frame;

pub fn render(
    f: &mut Frame,
    area: Rect,
    kind: ChartKind,
    series: &[ChartSeries],
    x_label: Option<&str>,
    y_label: Option<&str>,
) {
    match kind {
        ChartKind::Sparkline => {
            let data: Vec<u64> = series
                .iter()
                .flat_map(|s| s.points.iter().map(|p| p.1.max(0.0) as u64))
                .collect();
            let sp = Sparkline::default()
                .block(Block::default().borders(Borders::ALL).title("Sparkline"))
                .data(data.as_slice());
            f.render_widget(sp, area);
        }
        ChartKind::Bar => {
            // Collect owned names and values first so we can borrow &str from them.
            let named: Vec<(String, u64)> = series
                .iter()
                .map(|s| {
                    let total = s.points.iter().map(|p| p.1.max(0.0)).sum::<f64>() as u64;
                    (s.name.clone(), total)
                })
                .collect();
            let refs: Vec<(&str, u64)> = named.iter().map(|(n, v)| (n.as_str(), *v)).collect();
            let bc = BarChart::default()
                .block(Block::default().borders(Borders::ALL).title("Bar"))
                .data(BarGroup::from(refs.as_slice()))
                .bar_width(8);
            f.render_widget(bc, area);
        }
        ChartKind::Hist => {
            // Bin all series points into 10 equal-width bins (across all series).
            let mut all: Vec<f64> =
                series.iter().flat_map(|s| s.points.iter().map(|p| p.1)).collect();
            if all.is_empty() {
                return;
            }
            all.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let min = all[0];
            let max = all[all.len() - 1];
            let bin = ((max - min) / 10.0).max(f64::EPSILON);
            let mut counts = [0u64; 10];
            for v in &all {
                let i = ((v - min) / bin).floor() as usize;
                counts[i.min(9)] += 1;
            }
            // Build owned labels, then borrow &str from them.
            let labels: Vec<String> =
                (0..10).map(|i| format!("{:.1}", min + i as f64 * bin)).collect();
            let refs: Vec<(&str, u64)> = labels
                .iter()
                .map(String::as_str)
                .zip(counts.iter().copied())
                .collect();
            let bc = BarChart::default()
                .block(Block::default().borders(Borders::ALL).title("Histogram"))
                .data(BarGroup::from(refs.as_slice()))
                .bar_width(4);
            f.render_widget(bc, area);
        }
        ChartKind::Line | ChartKind::Scatter => {
            // Keep the point slices alive for the lifetime of `datasets`.
            let graph_type = if matches!(kind, ChartKind::Scatter) {
                GraphType::Scatter
            } else {
                GraphType::Line
            };
            let datasets: Vec<Dataset> = series
                .iter()
                .map(|s| {
                    Dataset::default()
                        .name(s.name.clone())
                        .marker(Marker::Braille)
                        .graph_type(graph_type)
                        .data(&s.points)
                        .style(Style::default().fg(Color::Cyan))
                })
                .collect();
            let (xmin, xmax, ymin, ymax) = bounds(series);
            let title = if matches!(kind, ChartKind::Scatter) { "Scatter" } else { "Line" };
            let chart = Chart::new(datasets)
                .block(Block::default().borders(Borders::ALL).title(title))
                .x_axis(
                    Axis::default()
                        .title(x_label.unwrap_or("x").to_string())
                        .bounds([xmin, xmax])
                        .labels([format!("{xmin:.1}"), format!("{xmax:.1}")]),
                )
                .y_axis(
                    Axis::default()
                        .title(y_label.unwrap_or("y").to_string())
                        .bounds([ymin, ymax])
                        .labels([format!("{ymin:.1}"), format!("{ymax:.1}")]),
                );
            f.render_widget(chart, area);
        }
    }
}

fn bounds(series: &[ChartSeries]) -> (f64, f64, f64, f64) {
    let mut xs = (f64::INFINITY, f64::NEG_INFINITY);
    let mut ys = (f64::INFINITY, f64::NEG_INFINITY);
    for s in series {
        for (x, y) in &s.points {
            xs.0 = xs.0.min(*x);
            xs.1 = xs.1.max(*x);
            ys.0 = ys.0.min(*y);
            ys.1 = ys.1.max(*y);
        }
    }
    if !xs.0.is_finite() {
        xs = (0.0, 1.0);
    }
    if !ys.0.is_finite() {
        ys = (0.0, 1.0);
    }
    (xs.0, xs.1, ys.0, ys.1)
}
