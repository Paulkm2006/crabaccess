use std::fs::File;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::Arc;

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use memmap2::Mmap;
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

fn parse_chunk(lines: &[&str], line_regex: &Regex, rules: &GroupingRules) -> Aggregates {
    let mut aggregates = Aggregates::default();

    for line in lines {
        match parse_line(line, line_regex) {
            Some(record) => aggregates.record(record, rules),
            None => aggregates.parse_errors += 1,
        }
    }

    aggregates
}

fn flush_chunk(
    chunk: &mut Vec<&str>,
    aggregate: &mut Aggregates,
    line_regex: &Regex,
    rules: &GroupingRules,
    status_pb: Option<&ProgressBar>,
) {
    if chunk.is_empty() {
        return;
    }

    let chunk_agg = parse_chunk(chunk, line_regex, rules);
    aggregate.merge(chunk_agg);

    if let Some(status_pb) = status_pb {
        status_pb.inc(chunk.len() as u64);
    }

    chunk.clear();
}

fn parse_mapped_line<'a>(line_bytes: &'a [u8], path: &Path) -> Result<&'a str> {
    let line_bytes = match line_bytes.last() {
        Some(b'\r') => &line_bytes[..line_bytes.len() - 1],
        _ => line_bytes,
    };

    str::from_utf8(line_bytes)
        .with_context(|| format!("Unable to decode UTF-8 log line from: {}", path.display()))
}

fn parse_file(
    path: &Path,
    line_regex: &Regex,
    rules: &GroupingRules,
    status_pb: Option<&ProgressBar>,
) -> Result<Aggregates> {
    let file = File::open(path)
        .with_context(|| format!("Unable to open log file: {}", path.display()))?;

    let file_len = file
        .metadata()
        .with_context(|| format!("Unable to read metadata for: {}", path.display()))?
        .len();

    if file_len == 0 {
        return Ok(Aggregates::default());
    }

    let mmap = unsafe {
        Mmap::map(&file)
    }
    .with_context(|| format!("Unable to memory-map log file: {}", path.display()))?;
    let bytes = &mmap[..];

    let mut chunk = Vec::with_capacity(CHUNK_LINES);
    let mut aggregate = Aggregates::default();
    let mut line_start = 0;

    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }

        let line = parse_mapped_line(&bytes[line_start..index], path)?;
        chunk.push(line);

        if chunk.len() == CHUNK_LINES {
            flush_chunk(&mut chunk, &mut aggregate, line_regex, rules, status_pb);
        }

        line_start = index + 1;
    }

    if line_start < bytes.len() {
        let line = parse_mapped_line(&bytes[line_start..], path)?;
        chunk.push(line);
    }

    flush_chunk(&mut chunk, &mut aggregate, line_regex, rules, status_pb);

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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::{Args, SortBy};
    use crate::domain::GroupingRules;

    fn make_temp_file(test_name: &str, contents: &[u8]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("crabaccess-{test_name}-{unique}.log"));
        fs::write(&path, contents).expect("temp log file should be written");
        path
    }

    fn default_grouping_rules() -> GroupingRules {
        GroupingRules::from_args(&Args {
            files: Vec::new(),
            load_db: None,
            save_db: None,
            export_csv: None,
            top: 30,
            graph_items: 0,
            sort_by: SortBy::Visits,
            group_ip_regex: "^(.*)$".to_owned(),
            group_ip_replace: "$1".to_owned(),
            group_path_regex: "^(.*)$".to_owned(),
            group_path_replace: "$1".to_owned(),
            group_ua_regex: "^(.*)$".to_owned(),
            group_ua_replace: "$1".to_owned(),
        })
        .expect("default grouping rules should compile")
    }

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

    #[test]
    fn parse_file_handles_crlf_and_trailing_newline_without_extra_error() {
        let regex = build_line_regex().expect("regex should compile");
        let rules = default_grouping_rules();
        let log_path = make_temp_file(
            "mmap-crlf",
            b"127.0.0.1 - - [13/Mar/2026:09:22:11 +0000] \"GET /ok HTTP/1.1\" 200 10 \"-\" \"curl/8.5\"\r\n127.0.0.2 - - [13/Mar/2026:09:22:12 +0000] \"GET /next HTTP/1.1\" 404 20 \"-\" \"curl/8.5\"\r\n",
        );

        let aggregates = parse_file(log_path.as_path(), &regex, &rules, None)
            .expect("mapped file should parse");

        assert_eq!(aggregates.total_visits, 2);
        assert_eq!(aggregates.parse_errors, 0);

        fs::remove_file(log_path).expect("temp log file should be removed");
    }
}
