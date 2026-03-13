use std::cmp::Ordering;
use std::collections::HashMap;

use anyhow::{Context, Result};
use regex::Regex;

use crate::cli::{Args, SortBy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Ip,
    Path,
    UserAgent,
}

impl Dimension {
    pub fn title(self) -> &'static str {
        match self {
            Self::Ip => "IP",
            Self::Path => "Path",
            Self::UserAgent => "User-Agent",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Ip => Self::Path,
            Self::Path => Self::UserAgent,
            Self::UserAgent => Self::Ip,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Ip => Self::UserAgent,
            Self::Path => Self::Ip,
            Self::UserAgent => Self::Path,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Counter {
    pub visits: u64,
    pub traffic_bytes: u64,
}

impl Counter {
    pub fn add_hit(&mut self, bytes: u64) {
        self.visits += 1;
        self.traffic_bytes += bytes;
    }

    pub fn merge(&mut self, other: Counter) {
        self.visits += other.visits;
        self.traffic_bytes += other.traffic_bytes;
    }
}

#[derive(Default)]
pub struct Aggregates {
    pub total_visits: u64,
    pub total_traffic_bytes: u64,
    pub parse_errors: u64,
    ip: HashMap<String, Counter>,
    path: HashMap<String, Counter>,
    user_agent: HashMap<String, Counter>,
}

impl Aggregates {
    pub fn merge(&mut self, other: Aggregates) {
        self.total_visits += other.total_visits;
        self.total_traffic_bytes += other.total_traffic_bytes;
        self.parse_errors += other.parse_errors;
        merge_map(&mut self.ip, other.ip);
        merge_map(&mut self.path, other.path);
        merge_map(&mut self.user_agent, other.user_agent);
    }

    pub fn record(&mut self, record: ParsedRecord, rules: &GroupingRules) {
        self.total_visits += 1;
        self.total_traffic_bytes += record.traffic_bytes;

        let grouped_ip = rules.ip.apply(&record.ip);
        let grouped_path = rules.path.apply(&record.path);
        let grouped_ua = rules.user_agent.apply(&record.user_agent);

        self.ip.entry(grouped_ip).or_default().add_hit(record.traffic_bytes);
        self.path
            .entry(grouped_path)
            .or_default()
            .add_hit(record.traffic_bytes);
        self.user_agent
            .entry(grouped_ua)
            .or_default()
            .add_hit(record.traffic_bytes);
    }

    pub fn selected_map(&self, dimension: Dimension) -> &HashMap<String, Counter> {
        match dimension {
            Dimension::Ip => &self.ip,
            Dimension::Path => &self.path,
            Dimension::UserAgent => &self.user_agent,
        }
    }
}

fn merge_map(target: &mut HashMap<String, Counter>, source: HashMap<String, Counter>) {
    for (key, value) in source {
        target.entry(key).or_default().merge(value);
    }
}

#[derive(Debug)]
pub struct ParsedRecord {
    pub ip: String,
    pub path: String,
    pub user_agent: String,
    pub traffic_bytes: u64,
}

#[derive(Clone)]
pub struct GroupRule {
    regex: Regex,
    replace: String,
}

impl GroupRule {
    pub fn apply(&self, input: &str) -> String {
        if self.regex.is_match(input) {
            self.regex
                .replace_all(input, self.replace.as_str())
                .to_string()
        } else {
            input.to_owned()
        }
    }
}

#[derive(Clone)]
pub struct GroupingRules {
    pub ip: GroupRule,
    pub path: GroupRule,
    pub user_agent: GroupRule,
}

impl GroupingRules {
    pub fn from_args(args: &Args) -> Result<Self> {
        Ok(Self {
            ip: GroupRule {
                regex: Regex::new(&args.group_ip_regex)
                    .with_context(|| "Invalid --group-ip-regex")?,
                replace: args.group_ip_replace.clone(),
            },
            path: GroupRule {
                regex: Regex::new(&args.group_path_regex)
                    .with_context(|| "Invalid --group-path-regex")?,
                replace: args.group_path_replace.clone(),
            },
            user_agent: GroupRule {
                regex: Regex::new(&args.group_ua_regex)
                    .with_context(|| "Invalid --group-ua-regex")?,
                replace: args.group_ua_replace.clone(),
            },
        })
    }
}

#[derive(Debug)]
pub struct MetricRow {
    pub key: String,
    pub visits: u64,
    pub traffic_bytes: u64,
    pub visit_pct: f64,
    pub traffic_pct: f64,
}

pub fn compare_rows(a: &MetricRow, b: &MetricRow, sort_by: SortBy) -> Ordering {
    let primary = match sort_by {
        SortBy::Visits => b.visits.cmp(&a.visits),
        SortBy::Traffic => b.traffic_bytes.cmp(&a.traffic_bytes),
    };
    if primary == Ordering::Equal {
        b.visits.cmp(&a.visits).then_with(|| a.key.cmp(&b.key))
    } else {
        primary
    }
}

pub fn pct(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (value as f64 / total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_grouping_can_extract_first_path_segment() {
        let rule = GroupRule {
            regex: Regex::new(r"^(/[^/?]+).*$").expect("rule regex should compile"),
            replace: "$1".to_owned(),
        };
        let grouped = rule.apply("/products/list?page=2");
        assert_eq!(grouped, "/products");
    }
}
