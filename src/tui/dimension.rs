use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Bar, BarChart, Block, Borders, Cell, Row, Table};

use crate::cli::SortBy;
use crate::domain::{Dimension, MetricRow};

use super::{App, format_bytes};

pub(super) fn render_dimension(
    frame: &mut ratatui::Frame<'_>,
    app: &mut App,
    dimension: Dimension,
    area: Rect,
) {
    let rows = app.rows();

    let sub = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Min(8)])
        .split(area);

    let bars: Vec<Bar<'static>> = rows
        .iter()
        .take(app.graph_items)
        .map(|row| {
            let value = match app.sort_by {
                SortBy::Visits => row.visits,
                SortBy::Traffic => row.traffic_bytes,
            };
            Bar::with_label(truncate_label(&row.key, 14), value)
                .text_value(chart_value_text(app.sort_by, row))
        })
        .collect();

    let bar_chart = BarChart::new(bars)
        .block(
            Block::default().borders(Borders::ALL).title(format!(
                "Top {} {} by {:?}",
                app.graph_items,
                dimension.title(),
                app.sort_by
            )),
        )
        .bar_width(9)
        .bar_gap(1)
        .value_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(Color::White))
        .bar_style(Style::default().fg(Color::Blue));
    frame.render_widget(bar_chart, sub[0]);

    let visible_rows = sub[1].height.saturating_sub(3) as usize;
    let max_scroll = rows.len().saturating_sub(visible_rows);
    app.scroll = app.scroll.min(max_scroll);
    let scroll = app.scroll;

    let table_rows: Vec<Row<'_>> = rows
        .iter()
        .skip(scroll)
        .take(visible_rows)
        .map(|r| {
            Row::new(vec![
                Cell::from(r.key.clone()),
                Cell::from(r.visits.to_string()),
                Cell::from(format_bytes(r.traffic_bytes)),
                Cell::from(format!("{:.2}%", r.visit_pct)),
                Cell::from(format!("{:.2}%", r.traffic_pct)),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Percentage(42),
            Constraint::Percentage(14),
            Constraint::Percentage(18),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
        ],
    )
    .header(
        Row::new(vec!["Key", "Visits", "Traffic", "Visit %", "Traffic %"])
            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title(format!(
        "Rows {}-{} of {} (j/k ↑↓ scroll)",
        scroll + 1,
        (scroll + visible_rows).min(rows.len()),
        rows.len()
    )));
    frame.render_widget(table, sub[1]);
}

fn truncate_label(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut out = text.chars().take(max.saturating_sub(1)).collect::<String>();
        out.push('~');
        out
    }
}

pub(crate) fn chart_value_text(sort_by: SortBy, row: &MetricRow) -> String {
    match sort_by {
        SortBy::Visits => row.visits.to_string(),
        SortBy::Traffic => format_bytes(row.traffic_bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric_row(visits: u64, traffic_bytes: u64) -> MetricRow {
        MetricRow {
            key: "example".to_owned(),
            visits,
            traffic_bytes,
            visit_pct: 0.0,
            traffic_pct: 0.0,
        }
    }

    #[test]
    fn chart_value_text_uses_visit_count_for_visit_sort() {
        let row = metric_row(42, 3_145_728);
        assert_eq!(chart_value_text(SortBy::Visits, &row), "42");
    }

    #[test]
    fn chart_value_text_uses_human_readable_bytes_for_traffic_sort() {
        let row = metric_row(42, 3_145_728);
        assert_eq!(chart_value_text(SortBy::Traffic, &row), "3.00 MB");
    }
}
