use std::sync::Arc;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

pub mod cli;
pub mod domain;
pub mod parser;
pub mod tui;

use cli::Args;
use domain::{Dimension, GroupingRules};
use parser::{build_line_regex, parse_files_parallel};
use tui::{App, run_tui};

pub fn run(args: Args) -> Result<()> {
    let rules = Arc::new(GroupingRules::from_args(&args)?);
    let line_regex = Arc::new(build_line_regex()?);

    let pb = ProgressBar::new(args.files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("=>-"),
    );

    let aggregates = parse_files_parallel(&args.files, line_regex, rules, &pb)?;
    pb.finish_and_clear();

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
