use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortBy {
    Visits,
    Traffic,
}

#[derive(Parser, Debug)]
#[command(
    name = "crabaccess",
    version,
    about = "Parse nginx access logs in parallel and visualize metrics in a TUI"
)]
pub struct Args {
    #[arg(required = true, value_name = "LOG_FILE")]
    pub files: Vec<PathBuf>,

    #[arg(long, default_value_t = 30)]
    pub top: usize,

    #[arg(long, default_value_t = 10)]
    pub graph_items: usize,

    #[arg(long, value_enum, default_value_t = SortBy::Visits)]
    pub sort_by: SortBy,

    #[arg(long, default_value = "^(.*)$")]
    pub group_ip_regex: String,

    #[arg(long, default_value = "$1")]
    pub group_ip_replace: String,

    #[arg(long, default_value = "^(.*)$")]
    pub group_path_regex: String,

    #[arg(long, default_value = "$1")]
    pub group_path_replace: String,

    #[arg(long, default_value = "^(.*)$")]
    pub group_ua_regex: String,

    #[arg(long, default_value = "$1")]
    pub group_ua_replace: String,
}
