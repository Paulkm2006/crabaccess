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
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

use crate::cli::SortBy;
use crate::domain::{Aggregates, DateGranularity, Dimension, MetricRow, compare_rows, pct};

mod dimension;
mod trend;

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
    pub(crate) fn rows(&self) -> Vec<MetricRow> {
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
                .title("Tab/←/→ switch  s sort  g granularity  q quit"),
        );
    frame.render_widget(tabs, areas[1]);

    match app.tab {
        AppTab::Trend => {
            trend::render_trend(frame, app, areas[2]);
        }
        AppTab::Dimension(dimension) => {
            dimension::render_dimension(frame, app, dimension, areas[2]);
        }
    }
}

pub(crate) fn format_bytes(value: u64) -> String {
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
