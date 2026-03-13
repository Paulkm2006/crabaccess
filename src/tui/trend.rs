use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::text::Span;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};

use crate::domain::DateGranularity;

use super::{App, format_bytes};

pub(super) fn render_trend(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let series = app.aggregates.date_series(app.trend_granularity);

    if series.is_empty() {
        let msg = Paragraph::new(
            "No timestamped data found.\nEnsure logs use standard nginx combined format.",
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Trend by Date"),
        );
        frame.render_widget(msg, area);
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let title_suffix = app.trend_granularity.label().to_owned();
    let n = series.len();
    let x_max = if n <= 1 { 1.0 } else { (n - 1) as f64 };
    let x_bounds = [0.0, x_max];

    let visit_data: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, (_, c))| (i as f64, c.visits as f64))
        .collect();
    let traffic_data: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, (_, c))| (i as f64, c.traffic_bytes as f64))
        .collect();

    let x_labels: Vec<Span<'_>> = {
        let first = Span::raw(trend_label(series[0].0, app.trend_granularity));
        let last = Span::raw(trend_label(series[n - 1].0, app.trend_granularity));
        if n >= 3 {
            let mid = Span::raw(trend_label(series[n / 2].0, app.trend_granularity));
            vec![first, mid, last]
        } else {
            vec![first, last]
        }
    };

    // Visits line chart
    let max_visits = visit_data
        .iter()
        .map(|&(_, v)| v as u64)
        .max()
        .unwrap_or(1)
        .max(1);
    let visit_dataset = Dataset::default()
        .name("Visits")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&visit_data);
    let visits_chart = Chart::new(vec![visit_dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Visits - {}", title_suffix)),
        )
        .x_axis(
            Axis::default()
                .bounds(x_bounds)
                .labels(x_labels.clone()),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_visits as f64 * 1.1])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{}", max_visits / 2)),
                    Span::raw(format!("{}", max_visits)),
                ]),
        );
    frame.render_widget(visits_chart, sections[0]);

    // Traffic line chart
    let max_traffic = traffic_data
        .iter()
        .map(|&(_, v)| v as u64)
        .max()
        .unwrap_or(1)
        .max(1);
    let traffic_dataset = Dataset::default()
        .name("Traffic")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Green))
        .data(&traffic_data);
    let traffic_chart = Chart::new(vec![traffic_dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Traffic - {}", title_suffix)),
        )
        .x_axis(
            Axis::default()
                .bounds(x_bounds)
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_traffic as f64 * 1.1])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format_bytes(max_traffic / 2)),
                    Span::raw(format_bytes(max_traffic)),
                ]),
        );
    frame.render_widget(traffic_chart, sections[1]);
}

pub(crate) fn trend_label(label: &str, granularity: DateGranularity) -> String {
    match granularity {
        DateGranularity::Hour => label.get(5..13).unwrap_or(label).to_owned(),
        DateGranularity::Day => label.get(5..10).unwrap_or(label).to_owned(),
        DateGranularity::Month => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trend_label_shortens_hour_keys() {
        assert_eq!(trend_label("2026-03-13 09", DateGranularity::Hour), "03-13 09");
    }

    #[test]
    fn trend_label_shortens_day_keys() {
        assert_eq!(trend_label("2026-03-13", DateGranularity::Day), "03-13");
    }
}
