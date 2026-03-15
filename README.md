# Crab Access

A high-performance Nginx access log analyzer with TUI visualization.

## Features

- **Parallel Log Parsing**: Uses Rayon for multi-threaded log processing
- **Interactive TUI**: Visualize metrics with a terminal-based user interface
- **Multiple Dimensions**: Analyze by IP, Path, User-Agent, or Status Code
- **Trend Analysis**: View traffic trends over time (hour/day/month granularity)
- **Database Support**: Save and load parsed results for fast re-analysis
- **CSV Export**: Export aggregated data for external analysis
- **Flexible Grouping**: Use regex to group IPs, paths, or user agents
- **Fast HashMap**: Use GxHash to store data maps.

## Installation

```bash
git clone https://github.com/Paulkm2006/crabaccess.git
cd crabaccess
RUSTFLAGS="-C target-cpu=native" cargo install --path .
```

Or build from source:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

## Usage

### Basic Usage

Analyze a single log file:

```bash
crabaccess /var/log/nginx/access.log
```

Analyze multiple log files:

```bash
crabaccess access.log access.log.1 access.log.2
```

Analyze all access logs in a directory:

```bash
crabaccess /var/log/nginx/
```

### Save and Load Database

Save parsed results for faster subsequent analysis:

```bash
crabaccess /var/log/nginx/access.log --save-db parsed.json
```

Load from previously saved database:

```bash
crabaccess --load-db parsed.json
```

### Export to CSV

Export aggregated data to CSV format:

```bash
crabaccess /var/log/nginx/access.log --export_csv results.csv
```

### Grouping Options

Group IPs by subnet (e.g., /24):

```bash
crabaccess access.log --group-ip-regex '^(\d+\.\d+\.\d+)\.' --group-ip-replace '$1.0/24'
```

Group paths by removing query strings:

```bash
crabaccess access.log --group-path-regex '\?.*$' --group-ip-replace ''
```

### TUI Controls

- `Tab` / `Shift+Tab`: Navigate between tabs (IP, Path, User-Agent, Status, Trend)
- `Up/Down`: Scroll through the list
- `t`: Toggle sort by (Visits / Traffic)
- `h/d/m`: Change trend granularity (Hour / Day / Month)
- `q`: Quit

### Command Line Options

| Option | Description | Default |
| -------- | ------------- | --------- |
| `LOG_FILE` | Input log file(s) or directory | Required |
| `--load-db FILE` | Load from database file | - |
| `--save-db FILE` | Save to database file | - |
| `--export-csv FILE` | Export results to CSV | - |
| `--top N` | Number of items to show | 30 |
| `--graph-items N` | Number of items in trend graph | 0 |
| `--sort-by` | Sort by visits or traffic | Visits |
| `--group-ip-regex` | Regex for IP grouping | `^(.*)$` |
| `--group-ip-replace` | Replacement for IP grouping | `$1` |
| `--group-path-regex` | Regex for path grouping | `^(.*)$` |
| `--group-path-replace` | Replacement for path grouping | `$1` |
| `--group-ua-regex` | Regex for UA grouping | `^(.*)$` |
| `--group-ua-replace` | Replacement for UA grouping | `$1` |

## Log Format

Expects Nginx combined log format by default:

```text
$remote_addr - $remote_user [$time_local] "$request" $status $body_bytes_sent "$http_referer" "$http_user_agent"
```

Example:

```text
192.168.1.1 - - [15/Mar/2026:10:30:45 +0000] "GET /api/users HTTP/1.1" 200 1234 "https://example.com" "Mozilla/5.0"
```

## Performance

crabaccess uses memory-mapped files and parallel processing to achieve high throughput:

- Memory-mapped I/O for efficient file reading
- Parallel parsing with Rayon
- Fast hashing with gxhash

## License

MIT
