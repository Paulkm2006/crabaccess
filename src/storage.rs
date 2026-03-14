use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use anyhow::{Context, Result};
use csv::Writer;
use serde::{Deserialize, Serialize};

use crate::domain::{Aggregates, DateGranularity, Dimension};

#[derive(Serialize, Deserialize)]
struct PersistedData {
    files_count: usize,
    aggregates: Aggregates,
}

#[derive(Serialize)]
struct PersistedDataRef<'a> {
    files_count: usize,
    aggregates: &'a Aggregates,
}

pub fn save_database(path: &Path, aggregates: &Aggregates, files_count: usize) -> Result<()> {
    let file = File::create(path)
        .with_context(|| format!("Unable to create database file: {}", path.display()))?;
    let writer = BufWriter::new(file);

    let payload = PersistedDataRef {
        files_count,
        aggregates,
    };

    serde_json::to_writer(writer, &payload)
        .with_context(|| format!("Unable to write database file: {}", path.display()))
}

pub fn load_database(path: &Path) -> Result<(Aggregates, usize)> {
    let file = File::open(path)
        .with_context(|| format!("Unable to open database file: {}", path.display()))?;
    let reader = BufReader::new(file);
    let payload: PersistedData = serde_json::from_reader(reader)
        .with_context(|| format!("Unable to parse database file: {}", path.display()))?;
    Ok((payload.aggregates, payload.files_count))
}

pub fn export_csv(path: &Path, aggregates: &Aggregates) -> Result<()> {
    let mut writer = Writer::from_path(path)
        .with_context(|| format!("Unable to create CSV file: {}", path.display()))?;

    writer.write_record(["kind", "bucket", "visits", "traffic_bytes"])?;

    for dimension in [
        Dimension::Ip,
        Dimension::Path,
        Dimension::UserAgent,
        Dimension::StatusCode,
    ] {
        let kind = format!("dimension:{}", dimension.title().to_lowercase().replace(' ', "_"));
        for (bucket, counter) in aggregates.selected_map(dimension) {
            writer.write_record([
                kind.as_str(),
                bucket.as_str(),
                &counter.visits.to_string(),
                &counter.traffic_bytes.to_string(),
            ])?;
        }
    }

    write_trend_rows(&mut writer, "trend:hour", aggregates, DateGranularity::Hour)?;
    write_trend_rows(&mut writer, "trend:day", aggregates, DateGranularity::Day)?;
    write_trend_rows(&mut writer, "trend:month", aggregates, DateGranularity::Month)?;

    writer.flush()?;
    Ok(())
}

fn write_trend_rows(
    writer: &mut Writer<File>,
    kind: &str,
    aggregates: &Aggregates,
    granularity: DateGranularity,
) -> Result<()> {
    for (bucket, counter) in aggregates.date_series(granularity) {
        writer.write_record([
            kind,
            bucket,
            &counter.visits.to_string(),
            &counter.traffic_bytes.to_string(),
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::domain::ParsedRecord;

    fn make_temp_file(name: &str, extension: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("crabaccess-{name}-{unique}.{extension}"))
    }

    fn sample_aggregates() -> Aggregates {
        let rules = crate::domain::GroupingRules::passthrough()
            .expect("passthrough rules should compile");

        let mut aggregates = Aggregates::default();
        aggregates.record(
            ParsedRecord {
                ip: "127.0.0.1".to_owned(),
                path: "/api".to_owned(),
                user_agent: "curl/8.5".to_owned(),
                status_code: "200".to_owned(),
                traffic_bytes: 1000,
                timestamp_str: Some("13/Mar/2026:09:22:11 +0000".to_owned()),
            },
            &rules,
        );
        aggregates.record(
            ParsedRecord {
                ip: "127.0.0.1".to_owned(),
                path: "/api".to_owned(),
                user_agent: "curl/8.5".to_owned(),
                status_code: "200".to_owned(),
                traffic_bytes: 234,
                timestamp_str: Some("13/Mar/2026:09:30:00 +0000".to_owned()),
            },
            &rules,
        );
        aggregates.parse_errors = 1;
        aggregates
    }

    #[test]
    fn database_roundtrip_preserves_aggregates() {
        let file = make_temp_file("db-roundtrip", "json");
        let aggregates = sample_aggregates();

        save_database(&file, &aggregates, 7).expect("database save should succeed");
        let (loaded, files_count) = load_database(&file).expect("database load should succeed");

        assert_eq!(files_count, 7);
        assert_eq!(loaded.total_visits, aggregates.total_visits);
        assert_eq!(loaded.total_traffic_bytes, aggregates.total_traffic_bytes);
        assert_eq!(loaded.parse_errors, aggregates.parse_errors);
        assert_eq!(
            loaded
                .selected_map(Dimension::Ip)
                .get("127.0.0.1")
                .expect("ip row should exist")
                .visits,
            2
        );

        std::fs::remove_file(file).expect("temp file should be removed");
    }

    #[test]
    fn csv_export_writes_rows() {
        let file = make_temp_file("csv-export", "csv");
        let aggregates = sample_aggregates();

        export_csv(&file, &aggregates).expect("csv export should succeed");

        let csv_content = std::fs::read_to_string(&file).expect("csv file should be readable");
        assert!(csv_content.contains("kind,bucket,visits,traffic_bytes"));
        assert!(csv_content.contains("dimension:ip,127.0.0.1,2,1234"));
        assert!(csv_content.contains("trend:day,2026-03-13,2,1234"));

        std::fs::remove_file(file).expect("temp file should be removed");
    }
}
