//! Async JSON Lines file I/O shared by the file-backed session stores.
//!
//! Encoding and decoding are delegated to [`serde_jsonlines`]; this module only
//! adds the file handling the session stores need (append-and-flush, tolerant
//! open of a not-yet-created log).
//!
//! Every line must carry a JSON value: the writers here always emit exactly one
//! value per line, so a blank line means the file was truncated or edited
//! outside Elph and is reported as a malformed line rather than skipped.

use std::io;
use std::path::Path;

use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_jsonlines::AsyncJsonLinesReader;
use serde_jsonlines::AsyncJsonLinesWriter;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::BufReader;

/// Append `value` to `path` as a single JSON line, then flush.
///
/// The file (but not its parent directory) is created when missing.
pub async fn append<T>(path: &Path, value: &T) -> io::Result<()>
where
    T: ?Sized + Serialize,
{
    let file = OpenOptions::new().create(true).append(true).open(path).await?;
    let mut writer = AsyncJsonLinesWriter::new(file);
    writer.write(value).await?;
    writer.flush().await
}

/// Decode every line of `path` as `T`, one result per line in file order.
///
/// A missing file yields an empty vector. Malformed lines are reported as
/// individual `Err` items so callers can decide whether to skip or fail; the
/// outer `Err` is reserved for failing to open the file at all.
pub async fn read_lines<T>(path: &Path) -> io::Result<Vec<io::Result<T>>>
where
    T: DeserializeOwned,
{
    let file = match File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut stream = AsyncJsonLinesReader::new(BufReader::new(file)).read_all::<T>();
    let mut lines = Vec::new();
    while let Some(line) = stream.next().await {
        lines.push(line);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Row {
        id: u32,
    }

    #[tokio::test]
    async fn append_then_read_roundtrips_every_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rows.jsonl");

        for id in 0..3 {
            append(&path, &Row { id }).await.expect("append");
        }

        let rows: Vec<Row> = read_lines::<Row>(&path)
            .await
            .expect("read")
            .into_iter()
            .map(|line| line.expect("line"))
            .collect();
        assert_eq!(rows, vec![Row { id: 0 }, Row { id: 1 }, Row { id: 2 }]);
    }

    #[tokio::test]
    async fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let rows = read_lines::<Row>(&dir.path().join("absent.jsonl")).await.expect("read");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn malformed_line_is_reported_per_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rows.jsonl");
        tokio::fs::write(&path, "{\"id\":1}\nnot json\n{\"id\":3}\n")
            .await
            .expect("write");

        let lines = read_lines::<Row>(&path).await.expect("read");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].as_ref().expect("first"), &Row { id: 1 });
        assert!(lines[1].is_err());
        assert_eq!(lines[2].as_ref().expect("third"), &Row { id: 3 });
    }
}
