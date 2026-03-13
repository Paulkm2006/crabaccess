use std::sync::Arc;

use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub mod cli;
pub mod domain;
pub mod parser;
pub mod tui;

use cli::Args;
use domain::{Dimension, GroupingRules};
use parser::{build_line_regex, count_file_lines, parse_files_parallel};
use tui::{App, run_tui};

pub fn run(args: Args) -> Result<()> {
    let rules = Arc::new(GroupingRules::from_args(&args)?);
    let line_regex = Arc::new(build_line_regex()?);

    let multi = MultiProgress::new();

    let files_pb = multi.add(ProgressBar::new(args.files.len() as u64));
    files_pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("=>-"),
    );

    let status_pb = if args.files.len() == 1 {
        let total_lines = count_file_lines(&args.files[0])?;
        let status = multi.add(ProgressBar::new(total_lines));
        status.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.green/black}] {pos}/{len} lines  {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        Some(status)
    } else {
        None
    };

    let aggregates = parse_files_parallel(
        &args.files,
        line_regex,
        rules,
        &files_pb,
        status_pb.as_ref(),
    )?;

    files_pb.finish_and_clear();
    if let Some(status) = status_pb {
        status.finish_and_clear();
    }

    let mut app = App {
        aggregates,
        files_count: args.files.len(),
        dimension: Dimension::Ip,
        sort_by: args.sort_by,
        top: args.top,
        graph_items: args.graph_items,
        scroll: 0,
    };

    run_tui(&mut app)
}
