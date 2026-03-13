use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Bar, BarChart, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Table,
    Tabs,
};

use crate::cli::SortBy;
use crate::domain::{Aggregates, DateGranularity, Dimension, MetricRow, compare_rows, pct};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
    Dimension(Dimension),
    Trend,
}

impl AppTab {
    fn next(self) -> Self {
        match self {
            Self::Dimension(Dimension::Ip) => Self::Dimension(Dimension::Path),
            Self::Dimension(Dimension::Path) => Self::Dimension(Dimension::UserAgent),
            Self::Dimension(Dimension::UserAgent) => Self::Dimension(Dimension::StatusCode),
            Self::Dimension(Dimension::StatusCode) => Self::Trend,
            Self::Trend => Self::Dimension(Dimension::Ip),
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Dimension(Dimension::Ip) => Self::Trend,
            Self::Dimension(Dimension::Path) => Self::Dimension(Dimension::Ip),
            Self::Dimension(Dimension::UserAgent) => Self::Dimension(Dimension::Path),
            Self::Dimension(Dimension::StatusCode) => Self::Dimension(Dimension::UserAgent),
            Self::Trend => Self::Dimension(Dimension::StatusCode),
        }
    }

    fn selected_index(self) -> usize {
        match self {
            Self::Dimension(Dimension::Ip) => 0,
            Self::Dimension(Dimension::Path) => 1,
            Self::Dimension(Dimension::UserAgent) => 2,
            Self::Dimension(Dimension::StatusCode) => 3,
            Self::Trend => 4,
        }
    }
}

pub struct App {
    pub aggregates: Aggregates,
    pub files_count: usize,
    pub tab: AppTab,
    pub sort_by: SortBy,
    pub top: usize,
    pub graph_items: usize,
    pub scroll: usize,
    pub trend_granularity: DateGranularity,
}

impl App {
    fn rows(&self) -> Vec<MetricRow> {
        let dimension = match self.tab {
            AppTab::Dimension(d) => d,
            AppTab::Trend => return vec![],
        };
        let mut rows: Vec<MetricRow> = self
            .aggregates
            .selected_map(dimension)
            .iter()
            .map(|(key, value)| MetricRow {
                key: key.clone(),
                visits: value.visits,
                traffic_bytes: value.traffic_bytes,
                visit_pct: pct(value.visits, self.aggregates.total_visits),
                traffic_pct: pct(value.traffic_bytes, self.aggregates.total_traffic_bytes),
            })
            .collect();

        rows.sort_by(|a, b| compare_rows(a, b, self.sort_by));
        rows.truncate(self.top);
        rows
    }
}

fn render_ui(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(frame.area());

    let summary = Paragraph::new(Line::from(format!(
        "visits={}  traffic={} bytes  parse_errors={}  files={}  sort={:?}",
        app.aggregates.total_visits,
        app.aggregates.total_traffic_bytes,
        app.aggregates.parse_errors,
        app.files_count,
        app.sort_by,
    )))
    .block(Block::default().borders(Borders::ALL).title("Summary"));
    frame.render_widget(summary, areas[0]);

    let titles = ["IP", "Path", "User-Agent", "Status", "Trend by Date"];
    let tabs = Tabs::new(titles)
        .select(app.tab.selected_index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Tab/←/→ switch  s metric  g granularity  q quit"),
        );
    frame.render_widget(tabs, areas[1]);

    match app.tab {
        AppTab::Trend => {
            render_trend(frame, app, areas[2]);
        }
        AppTab::Dimension(dimension) => {
            render_dimension(frame, app, dimension, areas[2]);
        }
    }
}

fn render_dimension(
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

fn render_trend(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
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

    let n = series.len();

    let data_visits: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, (_, c))| (i as f64, c.visits as f64))
        .collect();
    let data_traffic: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, (_, c))| (i as f64, c.traffic_bytes as f64))
        .collect();

    let (chart_data, metric_label): (&[(f64, f64)], &str) = match app.sort_by {
        SortBy::Visits => (&data_visits, "Visits"),
        SortBy::Traffic => (&data_traffic, "Traffic"),
    };

    let max_y = chart_data
        .iter()
        .map(|(_, y)| *y)
        .fold(0.0_f64, f64::max)
        .max(1.0);

    // Pick up to 6 evenly-spaced x-axis labels.
    let label_count = n.min(6);
    let step = if label_count <= 1 { 1 } else { (n - 1) / (label_count - 1) };
    let x_labels: Vec<Span<'static>> = (0..label_count)
        .map(|i| {
            let idx = (i * step).min(n - 1);
            Span::raw(series[idx].0.to_owned())
        })
        .collect();

    let y_labels: Vec<Span<'static>> = vec![
        Span::raw("0"),
        Span::raw(format_y_label(max_y * 0.5, app.sort_by)),
        Span::raw(format_y_label(max_y, app.sort_by)),
    ];

    let dataset = Dataset::default()
        .name(metric_label)
        .data(chart_data)
        .graph_type(GraphType::Line)
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan));

    let chart = Chart::new(vec![dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Trend by Date – {} – {}  [g=granularity  s=metric]",
                    app.trend_granularity.label(),
                    metric_label,
                ))
                .title_style(Style::default().fg(Color::White)),
        )
        .x_axis(
            Axis::default()
                .title(app.trend_granularity.label())
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (n.saturating_sub(1)) as f64])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title(metric_label)
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, max_y * 1.1])
                .labels(y_labels),
        );

    frame.render_widget(chart, area);
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

fn chart_value_text(sort_by: SortBy, row: &MetricRow) -> String {
    match sort_by {
        SortBy::Visits => row.visits.to_string(),
        SortBy::Traffic => format_bytes(row.traffic_bytes),
    }
}

fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = value as f64;
    let mut idx = 0usize;
    while size >= 1024.0 && idx < UNITS.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", value, UNITS[idx])
    } else {
        format!("{:.2} {}", size, UNITS[idx])
    }
}

fn format_y_label(value: f64, sort_by: SortBy) -> String {
    match sort_by {
        SortBy::Visits => format!("{}", value as u64),
        SortBy::Traffic => format_bytes(value as u64),
    }
}

pub fn run_tui(app: &mut App) -> Result<()> {
    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal backend")?;

    let run_result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| render_ui(f, app)).context("TUI draw error")?;

            if event::poll(Duration::from_millis(200)).context("Event poll error")?
                && let Event::Key(key) = event::read().context("Event read error")?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab | KeyCode::Right => {
                        app.tab = app.tab.next();
                        app.scroll = 0;
                    }
                    KeyCode::Left => {
                        app.tab = app.tab.previous();
                        app.scroll = 0;
                    }
                    KeyCode::Char('s') => {
                        app.sort_by = match app.sort_by {
                            SortBy::Visits => SortBy::Traffic,
                            SortBy::Traffic => SortBy::Visits,
                        };
                    }
                    KeyCode::Char('g') => {
                        app.trend_granularity = app.trend_granularity.next();
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        app.scroll = app.scroll.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.scroll = app.scroll.saturating_sub(1);
                    }
                    KeyCode::Home => app.scroll = 0,
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode().context("Failed to disable terminal raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to restore cursor")?;

    run_result
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
