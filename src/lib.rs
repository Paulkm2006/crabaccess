use std::sync::Arc;

use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub mod cli;
pub mod domain;
pub mod parser;
pub mod storage;
pub mod tui;

use cli::Args;
use domain::{DateGranularity, Dimension, GroupingRules};
use parser::{build_line_regex, parse_files_parallel};
use storage::{export_csv, load_database, save_database};
use tui::{App, AppTab, run_tui};

pub fn run(args: Args) -> Result<()> {
    let (aggregates, files_count) = if let Some(load_path) = args.load_db.as_ref() {
        load_database(load_path.as_path())?
    } else {
        let files = args.resolve_input_files()?;
        let rules = Arc::new(GroupingRules::from_args(&args)?);
        let line_regex = Arc::new(build_line_regex()?);

        let multi = MultiProgress::new();
        let status_pb = multi.add(ProgressBar::new(0));
        status_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.green/black}] {pos}/{len} lines  {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );

        let files_pb = multi.add(ProgressBar::new(files.len() as u64));
        files_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files",
            )
            .unwrap()
            .progress_chars("=>-"),
        );

        let aggregates = parse_files_parallel(
            &files,
            line_regex,
            rules,
            &files_pb,
            Some(&status_pb),
        )?;

        status_pb.finish_and_clear();
        files_pb.finish_and_clear();

        (aggregates, files.len())
    };

    if let Some(save_path) = args.save_db.as_ref() {
        save_database(save_path.as_path(), &aggregates, files_count)?;
    }

    if let Some(csv_path) = args.export_csv.as_ref() {
        export_csv(csv_path.as_path(), &aggregates)?;
    }

    let mut app = App {
        aggregates,
        files_count,
        tab: AppTab::Dimension(Dimension::Ip),
        sort_by: args.sort_by,
        top: args.top,
        graph_items: args.graph_items,
        scroll: 0,
        trend_granularity: DateGranularity::Day,
    };

    run_tui(&mut app)
}
