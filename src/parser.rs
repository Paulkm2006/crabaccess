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
        r#"^(?P<ip>\S+)\s+\S+\s+\S+\s+\[[^\]]+\]\s+\"(?P<request>[^\"]*)\"\s+(?P<status>\d{3})\s+(?P<bytes>\S+)\s+\"[^\"]*\"\s+\"(?P<ua>[^\"]*)\""#,
    )
    .context("Failed to compile nginx access log regex")
}

pub fn parse_line(line: &str, line_regex: &Regex) -> Option<ParsedRecord> {
    let captures = line_regex.captures(line)?;

    let ip = captures.name("ip")?.as_str().to_owned();
    let request = captures.name("request")?.as_str();
    let user_agent = captures
        .name("ua")
        .map_or("-", |m| m.as_str())
        .to_owned();
    let status_code = captures
        .name("status")
        .map_or("-", |m| m.as_str())
        .to_owned();
    let traffic_bytes = captures
        .name("bytes")
        .map_or(0, |m| m.as_str().parse::<u64>().unwrap_or(0));

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
    })
}

fn parse_file(path: &Path, line_regex: &Regex, rules: &GroupingRules) -> Result<Aggregates> {
    let file = File::open(path)
        .with_context(|| format!("Unable to open log file: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut aggregates = Aggregates::default();

    for line_result in reader.lines() {
        let line = line_result
            .with_context(|| format!("Unable to read a line from: {}", path.display()))?;
        match parse_line(&line, line_regex) {
            Some(record) => aggregates.record(record, rules),
            None => aggregates.parse_errors += 1,
        }
    }

    Ok(aggregates)
}

pub fn parse_files_parallel(
    files: &[PathBuf],
    line_regex: Arc<Regex>,
    rules: Arc<GroupingRules>,
    pb: &ProgressBar,
) -> Result<Aggregates> {
    files
        .par_iter()
        .try_fold(Aggregates::default, |mut acc, file| {
            let part = parse_file(file.as_path(), &line_regex, &rules)?;
            acc.merge(part);
            pb.inc(1);
            Ok(acc)
        })
        .try_reduce(Aggregates::default, |mut a, b| {
            a.merge(b);
            Ok(a)
        })
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
