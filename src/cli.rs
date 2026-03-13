use std::path::PathBuf;
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
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
    #[arg(value_name = "LOG_FILE", required_unless_present = "load_db")]
    pub files: Vec<PathBuf>,

    #[arg(long, value_name = "FILE", conflicts_with = "files")]
    pub load_db: Option<PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub save_db: Option<PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub export_csv: Option<PathBuf>,

    #[arg(long, default_value_t = 30)]
    pub top: usize,

    #[arg(long, default_value_t = 0)]
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

impl Args {
    pub fn resolve_input_files(&self) -> Result<Vec<PathBuf>> {
        let mut resolved = Vec::new();

        for input in &self.files {
            if input.is_dir() {
                let mut discovered = discover_access_logs(input)?;
                if discovered.is_empty() {
                    bail!(
                        "No files beginning with 'access.log' were found in {}",
                        input.display()
                    );
                }
                resolved.append(&mut discovered);
            } else {
                resolved.push(input.clone());
            }
        }

        if resolved.is_empty() {
            bail!("No input log files were resolved from the provided paths");
        }

        Ok(resolved)
    }
}

fn discover_access_logs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = fs::read_dir(dir)
        .with_context(|| format!("Unable to read input directory: {}", dir.display()))?
        .filter_map(|entry_result| match entry_result {
            Ok(entry) => Some(Ok(entry)),
            Err(error) => Some(Err(error)),
        })
        .map(|entry_result| {
            let entry = entry_result
                .with_context(|| format!("Unable to read an entry from: {}", dir.display()))?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();

            Ok((path, file_name.starts_with("access.log")))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(path, matches)| if matches && path.is_file() { Some(path) } else { None })
        .collect::<Vec<_>>();

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("crabaccess-{test_name}-{unique}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn resolve_input_files_expands_directories_to_access_logs() {
        let dir = make_temp_dir("expand-dir");
        let nested_dir = dir.join("child");
        fs::create_dir_all(&nested_dir).expect("nested dir should be created");
        fs::write(dir.join("access.log"), b"first").expect("access.log should be created");
        fs::write(dir.join("access.log.1"), b"second").expect("access.log.1 should be created");
        fs::write(dir.join("error.log"), b"ignored").expect("error.log should be created");
        fs::write(nested_dir.join("access.log.2"), b"ignored").expect("nested log should be created");

        let args = Args {
            files: vec![dir.clone()],
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
        };

        let resolved = args.resolve_input_files().expect("directory should resolve");

        assert_eq!(
            resolved,
            vec![dir.join("access.log"), dir.join("access.log.1")]
        );

        fs::remove_dir_all(&dir).expect("temp dir should be removed");
    }

    #[test]
    fn resolve_input_files_errors_when_directory_has_no_access_logs() {
        let dir = make_temp_dir("empty-dir");
        fs::write(dir.join("error.log"), b"ignored").expect("error.log should be created");

        let args = Args {
            files: vec![dir.clone()],
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
        };

        let error = args
            .resolve_input_files()
            .expect_err("directory without access logs should fail");

        assert!(
            error
                .to_string()
                .contains("No files beginning with 'access.log' were found")
        );

        fs::remove_dir_all(&dir).expect("temp dir should be removed");
    }
}
