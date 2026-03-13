use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{BarChart, Block, Borders, Cell, Paragraph, Row, Table, Tabs};

use crate::cli::SortBy;
use crate::domain::{Aggregates, Dimension, MetricRow, compare_rows, pct};

pub struct App {
    pub aggregates: Aggregates,
    pub files_count: usize,
    pub dimension: Dimension,
    pub sort_by: SortBy,
    pub top: usize,
    pub graph_items: usize,
    pub scroll: usize,
}

impl App {
    fn rows(&self) -> Vec<MetricRow> {
        let mut rows: Vec<MetricRow> = self
            .aggregates
            .selected_map(self.dimension)
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

fn render_ui(frame: &mut ratatui::Frame<'_>, app: &App) {
    let rows = app.rows();

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(12),
            Constraint::Min(8),
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

    let titles = ["IP", "Path", "User-Agent", "Status"];
    let selected = match app.dimension {
        Dimension::Ip => 0,
        Dimension::Path => 1,
        Dimension::UserAgent => 2,
        Dimension::StatusCode => 3,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Dimension (Tab/Left/Right)"),
        );
    frame.render_widget(tabs, areas[1]);

    let bar_labels: Vec<String> = rows
        .iter()
        .take(app.graph_items)
        .map(|r| truncate_label(&r.key, 14))
        .collect();
    let bar_data: Vec<(&str, u64)> = bar_labels
        .iter()
        .zip(rows.iter().take(app.graph_items))
        .map(|(label, row)| {
            let value = match app.sort_by {
                SortBy::Visits => row.visits,
                SortBy::Traffic => row.traffic_bytes,
            };
            (label.as_str(), value)
        })
        .collect();

    let chart = BarChart::default()
        .block(
            Block::default().borders(Borders::ALL).title(format!(
                "Top {} {} by {:?}",
                app.graph_items,
                app.dimension.title(),
                app.sort_by
            )),
        )
        .data(&bar_data)
        .bar_width(9)
        .bar_gap(1)
        .value_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(Color::White))
        .bar_style(Style::default().fg(Color::Blue));
    frame.render_widget(chart, areas[2]);

    let visible_rows = areas[3].height.saturating_sub(3) as usize;
    let max_scroll = rows.len().saturating_sub(visible_rows);
    let scroll = app.scroll.min(max_scroll);

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
        "Rows {}-{} of {} (j/k or Up/Down to scroll, s toggles sort, q quits)",
        scroll + 1,
        (scroll + visible_rows).min(rows.len()),
        rows.len()
    )));
    frame.render_widget(table, areas[3]);
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
                        app.dimension = app.dimension.next();
                        app.scroll = 0;
                    }
                    KeyCode::Left => {
                        app.dimension = app.dimension.previous();
                        app.scroll = 0;
                    }
                    KeyCode::Char('s') => {
                        app.sort_by = match app.sort_by {
                            SortBy::Visits => SortBy::Traffic,
                            SortBy::Traffic => SortBy::Visits,
                        };
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
