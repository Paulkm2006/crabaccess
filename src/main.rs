use anyhow::Result;
use clap::Parser;

use crabaccess::cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    crabaccess::run(args)
}
