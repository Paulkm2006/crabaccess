use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cli::{Args, SortBy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Ip,
    Path,
    UserAgent,
    StatusCode,
}

impl Dimension {
    pub fn title(self) -> &'static str {
        match self {
            Self::Ip => "IP",
            Self::Path => "Path",
            Self::UserAgent => "User-Agent",
            Self::StatusCode => "Status",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Ip => Self::Path,
            Self::Path => Self::UserAgent,
            Self::UserAgent => Self::StatusCode,
            Self::StatusCode => Self::Ip,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Ip => Self::StatusCode,
            Self::Path => Self::Ip,
            Self::UserAgent => Self::Path,
            Self::StatusCode => Self::UserAgent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateGranularity {
    Hour,
    #[default]
    Day,
    Month,
}

impl DateGranularity {
    pub fn next(self) -> Self {
        match self {
            Self::Hour => Self::Day,
            Self::Day => Self::Month,
            Self::Month => Self::Hour,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "Hour",
            Self::Day => "Day",
            Self::Month => "Month",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

#[derive(Default, Serialize, Deserialize)]
pub struct Aggregates {
    pub total_visits: u64,
    pub total_traffic_bytes: u64,
    pub parse_errors: u64,
    ip: HashMap<String, Counter>,
    path: HashMap<String, Counter>,
    user_agent: HashMap<String, Counter>,
    status_code: HashMap<String, Counter>,
    pub by_day: BTreeMap<String, Counter>,
    pub by_hour: BTreeMap<String, Counter>,
    pub by_month: BTreeMap<String, Counter>,
}

impl Aggregates {
    pub fn merge(&mut self, other: Aggregates) {
        self.total_visits += other.total_visits;
        self.total_traffic_bytes += other.total_traffic_bytes;
        self.parse_errors += other.parse_errors;
        merge_map(&mut self.ip, other.ip);
        merge_map(&mut self.path, other.path);
        merge_map(&mut self.user_agent, other.user_agent);
        merge_map(&mut self.status_code, other.status_code);
        merge_btree(&mut self.by_day, other.by_day);
        merge_btree(&mut self.by_hour, other.by_hour);
        merge_btree(&mut self.by_month, other.by_month);
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
        self.status_code
            .entry(record.status_code)
            .or_default()
            .add_hit(record.traffic_bytes);

        if let Some(ref ts) = record.timestamp_str {
            if let Some((day_key, hour_key, month_key)) = parse_timestamp(ts) {
                self.by_day.entry(day_key).or_default().add_hit(record.traffic_bytes);
                self.by_hour.entry(hour_key).or_default().add_hit(record.traffic_bytes);
                self.by_month.entry(month_key).or_default().add_hit(record.traffic_bytes);
            }
        }
    }

    pub fn date_series(&self, granularity: DateGranularity) -> Vec<(&str, Counter)> {
        let map = match granularity {
            DateGranularity::Hour => &self.by_hour,
            DateGranularity::Day => &self.by_day,
            DateGranularity::Month => &self.by_month,
        };
        map.iter().map(|(k, v)| (k.as_str(), *v)).collect()
    }

    pub fn selected_map(&self, dimension: Dimension) -> &HashMap<String, Counter> {
        match dimension {
            Dimension::Ip => &self.ip,
            Dimension::Path => &self.path,
            Dimension::UserAgent => &self.user_agent,
            Dimension::StatusCode => &self.status_code,
        }
    }
}

fn merge_map(target: &mut HashMap<String, Counter>, source: HashMap<String, Counter>) {
    for (key, value) in source {
        target.entry(key).or_default().merge(value);
    }
}

fn merge_btree(target: &mut BTreeMap<String, Counter>, source: BTreeMap<String, Counter>) {
    for (key, value) in source {
        target.entry(key).or_default().merge(value);
    }
}

fn parse_timestamp(s: &str) -> Option<(String, String, String)> {
    // Format: "13/Mar/2026:09:22:11 +0000"
    if s.len() < 14 {
        return None;
    }
    let day = s.get(0..2)?;
    let mon_str = s.get(3..6)?;
    let year = s.get(7..11)?;
    let hour = s.get(12..14)?;
    let mon_num = match mon_str {
        "Jan" => "01", "Feb" => "02", "Mar" => "03", "Apr" => "04",
        "May" => "05", "Jun" => "06", "Jul" => "07", "Aug" => "08",
        "Sep" => "09", "Oct" => "10", "Nov" => "11", "Dec" => "12",
        _ => return None,
    };
    Some((
        format!("{}-{}-{}", year, mon_num, day),
        format!("{}-{}-{} {}:00", year, mon_num, day, hour),
        format!("{}-{}", year, mon_num),
    ))
}

#[derive(Debug)]
pub struct ParsedRecord {
    pub ip: String,
    pub path: String,
    pub user_agent: String,
    pub status_code: String,
    pub traffic_bytes: u64,
    pub timestamp_str: Option<String>,
}

#[derive(Clone)]
pub struct GroupRule {
    kind: GroupRuleKind,
}

#[derive(Clone)]
enum GroupRuleKind {
    Passthrough,
    Regex { regex: Regex, replace: String },
}

impl GroupRule {
    fn from_parts(regex: Regex, replace: String) -> Self {
        if regex.as_str() == "^(.*)$" && replace == "$1" {
            return Self {
                kind: GroupRuleKind::Passthrough,
            };
        }

        Self {
            kind: GroupRuleKind::Regex { regex, replace },
        }
    }

    pub fn apply(&self, input: &str) -> String {
        match &self.kind {
            GroupRuleKind::Passthrough => input.to_owned(),
            GroupRuleKind::Regex { regex, replace } => {
                regex.replace_all(input, replace.as_str()).to_string()
            }
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
            ip: GroupRule::from_parts(
                Regex::new(&args.group_ip_regex).with_context(|| "Invalid --group-ip-regex")?,
                args.group_ip_replace.clone(),
            ),
            path: GroupRule::from_parts(
                Regex::new(&args.group_path_regex)
                    .with_context(|| "Invalid --group-path-regex")?,
                args.group_path_replace.clone(),
            ),
            user_agent: GroupRule::from_parts(
                Regex::new(&args.group_ua_regex).with_context(|| "Invalid --group-ua-regex")?,
                args.group_ua_replace.clone(),
            ),
        })
    }

    pub fn passthrough() -> Result<Self> {
        let regex = Regex::new("^(.*)$").with_context(|| "Invalid passthrough regex")?;
        Ok(Self {
            ip: GroupRule::from_parts(regex.clone(), "$1".to_owned()),
            path: GroupRule::from_parts(regex.clone(), "$1".to_owned()),
            user_agent: GroupRule::from_parts(regex, "$1".to_owned()),
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
        let rule = GroupRule::from_parts(
            Regex::new(r"^(/[^/?]+).*$").expect("rule regex should compile"),
            "$1".to_owned(),
        );
        let grouped = rule.apply("/products/list?page=2");
        assert_eq!(grouped, "/products");
    }
}
