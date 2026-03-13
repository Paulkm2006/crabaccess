use std::sync::Arc;

use anyhow::Result;

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

    let aggregates = parse_files_parallel(&args.files, line_regex, rules)?;

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
