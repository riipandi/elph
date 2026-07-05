use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use elph_agent::LogRotation;
use time::{Duration, OffsetDateTime, Time};

pub struct RollingWriter {
    inner: Mutex<RollingWriterInner>,
}

const ROTATION_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

struct RollingWriterInner {
    directory: PathBuf,
    app: String,
    rotation: LogRotation,
    max_files: Option<usize>,
    period: String,
    file: File,
    last_period_check: Instant,
}

impl RollingWriter {
    pub fn new(logs_dir: &Path, app: &str, rotation: LogRotation, max_files: Option<usize>) -> io::Result<Self> {
        fs::create_dir_all(logs_dir)?;
        let now = OffsetDateTime::now_utc();
        let period = period_key(rotation, now);
        let file = open_log_file(logs_dir, app, &period)?;
        if let Some(max) = max_files {
            prune_old_logs(logs_dir, app, max)?;
        }

        Ok(Self {
            inner: Mutex::new(RollingWriterInner {
                directory: logs_dir.to_path_buf(),
                app: app.to_string(),
                rotation,
                max_files,
                period,
                file,
                last_period_check: Instant::now(),
            }),
        })
    }
}

impl Write for RollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().expect("rolling log mutex poisoned");
        if inner.last_period_check.elapsed() >= ROTATION_CHECK_INTERVAL {
            let now = OffsetDateTime::now_utc();
            let period = period_key(inner.rotation, now);
            inner.last_period_check = Instant::now();
            if period != inner.period {
                inner.file = open_log_file(&inner.directory, &inner.app, &period)?;
                inner.period = period;
                if let Some(max) = inner.max_files {
                    prune_old_logs(&inner.directory, &inner.app, max)?;
                }
            }
        }
        inner.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().expect("rolling log mutex poisoned").file.flush()
    }
}

pub fn build_writer(
    logs_dir: &Path,
    app: &str,
    rotation: LogRotation,
    max_files: Option<usize>,
) -> io::Result<RollingWriter> {
    RollingWriter::new(logs_dir, app, rotation, max_files)
}

fn period_key(rotation: LogRotation, now: OffsetDateTime) -> String {
    let rounded = round_timestamp(rotation, now);
    format!(
        "{:04}{:02}{:02}_{:02}",
        rounded.year(),
        u8::from(rounded.month()),
        rounded.day(),
        rounded.hour()
    )
}

fn round_timestamp(rotation: LogRotation, now: OffsetDateTime) -> OffsetDateTime {
    match rotation {
        LogRotation::Hourly => {
            let time = Time::from_hms(now.hour(), 0, 0).expect("valid hourly time");
            now.replace_time(time)
        }
        LogRotation::Daily => now.replace_time(Time::MIDNIGHT),
        LogRotation::Weekly => {
            let days_since_sunday = now.weekday().number_days_from_sunday();
            let date = now.date() - Duration::days(days_since_sunday.into());
            date.with_time(Time::MIDNIGHT).assume_utc()
        }
    }
}

fn log_filename(app: &str, period: &str) -> String {
    format!("{app}-{period}.jsonl")
}

fn open_log_file(logs_dir: &Path, app: &str, period: &str) -> io::Result<File> {
    let path = logs_dir.join(log_filename(app, period));
    OpenOptions::new().create(true).append(true).open(path)
}

fn prune_old_logs(logs_dir: &Path, app: &str, max_files: usize) -> io::Result<()> {
    if max_files == 0 {
        return Ok(());
    }

    let prefix = format!("{app}-");
    let suffix = ".jsonl";
    let mut files = Vec::new();

    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(suffix) {
            continue;
        }
        if entry.file_type()?.is_file() {
            files.push(name.to_string());
        }
    }

    if files.len() < max_files {
        return Ok(());
    }

    files.sort_unstable();
    for name in files.iter().take(files.len().saturating_sub(max_files - 1)) {
        fs::remove_file(logs_dir.join(name))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_key_uses_compact_timestamp() {
        use time::{Date, Month};
        let timestamp = Date::from_calendar_date(2024, Month::July, 15)
            .expect("valid date")
            .with_hms(13, 45, 0)
            .expect("valid time")
            .assume_utc();
        assert_eq!(period_key(LogRotation::Hourly, timestamp), "20240715_13");
    }

    #[test]
    fn log_filename_matches_expected_pattern() {
        assert_eq!(log_filename("elph", "20240715_13"), "elph-20240715_13.jsonl");
    }
}
