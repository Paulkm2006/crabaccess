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

    let (bar_count, bar_width) = chart_geometry(sub[0], rows.len(), app.graph_items);
    let bars: Vec<Bar<'static>> = rows
        .iter()
        .take(bar_count)
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
                "Top {} of {} {} by {:?}",
                bar_count,
                rows.len(),
                dimension.title(),
                app.sort_by
            )),
        )
        .bar_width(bar_width)
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

fn chart_geometry(chart_area: Rect, rows_len: usize, graph_items_cap: usize) -> (usize, u16) {
    const MIN_BAR_WIDTH: usize = 3;
    const PREFERRED_BAR_WIDTH: usize = 9;
    const BAR_GAP: usize = 1;

    if rows_len == 0 {
        return (0, PREFERRED_BAR_WIDTH as u16);
    }

    let inner_width = chart_area.width.saturating_sub(2) as usize;
    if inner_width == 0 {
        return (1, MIN_BAR_WIDTH as u16);
    }

    let width_limited = ((inner_width + BAR_GAP) / (MIN_BAR_WIDTH + BAR_GAP)).max(1);
    let cap_limited = if graph_items_cap == 0 {
        width_limited
    } else {
        width_limited.min(graph_items_cap)
    };
    let bar_count = rows_len.min(cap_limited).max(1);

    let gaps_width = BAR_GAP * bar_count.saturating_sub(1);
    let bars_width = inner_width.saturating_sub(gaps_width);
    let bar_width = (bars_width / bar_count)
        .clamp(MIN_BAR_WIDTH, PREFERRED_BAR_WIDTH)
        as u16;

    (bar_count, bar_width)
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

    #[test]
    fn chart_geometry_uses_available_width_for_capacity() {
        let area = Rect::new(0, 0, 80, 12);
        let (bar_count, bar_width) = chart_geometry(area, 100, 0);

        assert_eq!(bar_count, 19);
        assert_eq!(bar_width, 3);
    }

    #[test]
    fn chart_geometry_honors_graph_items_cap_when_set() {
        let area = Rect::new(0, 0, 120, 12);
        let (bar_count, bar_width) = chart_geometry(area, 100, 7);

        assert_eq!(bar_count, 7);
        assert!(bar_width >= 3);
    }
}
