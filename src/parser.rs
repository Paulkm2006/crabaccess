use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use rayon::prelude::*;
use regex::Regex;

use crate::domain::{Aggregates, GroupingRules, ParsedRecord};

pub fn build_line_regex() -> Result<Regex> {
    Regex::new(
        r#"^(\S+)\s+\S+\s+\S+\s+\[([^\]]+)\]\s+\"([^\"]*)\"\s+(\d{3})\s+(\S+)\s+\"[^\"]*\"\s+\"([^\"]*)\""#,
    )
    .context("Failed to compile nginx access log regex")
}

pub fn parse_line(line: &str, line_regex: &Regex) -> Option<ParsedRecord> {
    let captures = line_regex.captures(line)?;

    let ip = captures.get(1)?.as_str().to_owned();
    let timestamp_str = captures.get(2).map(|m| m.as_str().to_owned());
    let request = captures.get(3)?.as_str();
    let status_code = captures.get(4).map_or("-", |m| m.as_str()).to_owned();
    let traffic_bytes = captures
        .get(5)
        .map_or(0, |m| m.as_str().parse::<u64>().unwrap_or(0));
    let user_agent = captures.get(6).map_or("-", |m| m.as_str()).to_owned();

    let path = request
        .split_whitespace()
        .nth(1)
        .map_or_else(|| "-".to_owned(), std::borrow::ToOwned::to_owned);

    Some(ParsedRecord {
        ip,
        path,
        user_agent,
        status_code,
        traffic_bytes,
        timestamp_str,
    })
}

const CHUNK_LINES: usize = 20_000;
const READ_BUFFER_CAPACITY: usize = 1024 * 1024;

fn parse_chunk(lines: &[String], line_regex: &Regex, rules: &GroupingRules) -> Aggregates {
    let mut aggregates = Aggregates::default();

    for line in lines {
        match parse_line(line, line_regex) {
            Some(record) => aggregates.record(record, rules),
            None => aggregates.parse_errors += 1,
        }
    }

    aggregates
}

fn parse_file(
    path: &Path,
    line_regex: &Regex,
    rules: &GroupingRules,
    status_pb: Option<&ProgressBar>,
) -> Result<Aggregates> {
    let file = File::open(path)
        .with_context(|| format!("Unable to open log file: {}", path.display()))?;
    let reader = BufReader::with_capacity(READ_BUFFER_CAPACITY, file);

    let mut chunk = Vec::with_capacity(CHUNK_LINES);
    let mut aggregate = Aggregates::default();

    for line_result in reader.lines() {
        let line = line_result
            .with_context(|| format!("Unable to read a line from: {}", path.display()))?;
        chunk.push(line);

        if chunk.len() == CHUNK_LINES {
            let chunk_agg = parse_chunk(&chunk, line_regex, rules);
            aggregate.merge(chunk_agg);
            if let Some(status_pb) = status_pb {
                status_pb.inc(chunk.len() as u64);
            }
            chunk.clear();
        }
    }

    if !chunk.is_empty() {
        let chunk_agg = parse_chunk(&chunk, line_regex, rules);
        aggregate.merge(chunk_agg);
        if let Some(status_pb) = status_pb {
            status_pb.inc(chunk.len() as u64);
        }
    }

    Ok(aggregate)
}

pub fn parse_files_parallel(
    files: &[PathBuf],
    line_regex: Arc<Regex>,
    rules: Arc<GroupingRules>,
    files_pb: &ProgressBar,
    status_pb: Option<&ProgressBar>,
) -> Result<Aggregates> {
    if let Some(pb) = status_pb {
        pb.set_message("processing lines");
    }

    let partials: Vec<Result<Aggregates>> = files
        .par_iter()
        .map(|file| {
            let part = parse_file(file.as_path(), &line_regex, &rules, status_pb)?;
            files_pb.inc(1);
            Ok(part)
        })
        .collect();

    let mut acc = Aggregates::default();
    for partial in partials {
        acc.merge(partial?);
    }

    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_nginx_line() {
        let regex = build_line_regex().expect("regex should compile");
        let line = "127.0.0.1 - - [13/Mar/2026:09:22:11 +0000] \"GET /api/v1/users?id=1 HTTP/1.1\" 200 532 \"-\" \"curl/8.5\"";
        let rec = parse_line(line, &regex).expect("line should parse");
        assert_eq!(rec.ip, "127.0.0.1");
        assert_eq!(rec.path, "/api/v1/users?id=1");
        assert_eq!(rec.user_agent, "curl/8.5");
        assert_eq!(rec.traffic_bytes, 532);
    }
}
