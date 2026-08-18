use std::fs::{self};
use std::fs::{File, OpenOptions};
use std::io::{self};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

use fastrace::collector::{EventRecord, Reporter, SpanRecord};
use serde_json::json;
use serde_jsonlines::JsonLinesWriter;

const TRACE_QUEUE_CAP: usize = 256;

enum TraceWrite {
    Spans(Vec<SpanRecord>),
    Flush(SyncSender<()>),
}

static ACTIVE_TX: Mutex<Option<SyncSender<TraceWrite>>> = Mutex::new(None);

/// Wait until the background writer has flushed queued spans (or 1s timeout).
pub fn flush_writer() {
    let tx = { ACTIVE_TX.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let Some(tx) = tx else {
        return;
    };
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx.send(TraceWrite::Flush(ack_tx)).is_err() {
        return;
    }
    match ack_rx.recv_timeout(Duration::from_secs(1)) {
        Ok(()) | Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
    }
}

/// Writes collected span trees as JSON lines under the application logs directory.
///
/// `report` is non-blocking: spans are queued to a dedicated writer thread.
/// Incoming batches are dropped when the queue is full.
pub struct JsonlReporter {
    path: PathBuf,
    tx: Option<SyncSender<TraceWrite>>,
    writer: Option<JoinHandle<()>>,
}

impl JsonlReporter {
    pub fn new(logs_dir: &std::path::Path, app_name: &str) -> io::Result<Self> {
        fs::create_dir_all(logs_dir)?;
        let path = logs_dir.join(format!("{app_name}-traces.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let (tx, rx) = mpsc::sync_channel(TRACE_QUEUE_CAP);
        let writer = std::thread::Builder::new()
            .name(format!("{app_name}-trace-writer"))
            .spawn(move || writer_loop(file, rx))
            .map_err(io::Error::other)?;
        *ACTIVE_TX.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx.clone());
        Ok(Self {
            path,
            tx: Some(tx),
            writer: Some(writer),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for JsonlReporter {
    fn drop(&mut self) {
        *ACTIVE_TX.lock().unwrap_or_else(|e| e.into_inner()) = None;
        drop(self.tx.take());
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }
    }
}

impl Reporter for JsonlReporter {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        match tx.try_send(TraceWrite::Spans(spans)) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

fn writer_loop(file: File, rx: mpsc::Receiver<TraceWrite>) {
    let mut writer = JsonLinesWriter::new(file);
    while let Ok(cmd) = rx.recv() {
        match cmd {
            TraceWrite::Spans(spans) => {
                for span in spans {
                    if writer.write(&span_to_json(&span)).is_err() {
                        return;
                    }
                }
                let _ = writer.flush();
            }
            TraceWrite::Flush(ack) => {
                let _ = writer.flush();
                let _ = ack.send(());
            }
        }
    }
}

fn span_to_json(span: &SpanRecord) -> serde_json::Value {
    json!({
        "trace_id": span.trace_id.to_string(),
        "span_id": span.span_id.to_string(),
        "parent_id": span.parent_id.to_string(),
        "name": span.name,
        "begin_time_unix_ns": span.begin_time_unix_ns,
        "duration_ns": span.duration_ns,
        "properties": properties_to_json(&span.properties),
        "events": span.events.iter().map(event_to_json).collect::<Vec<_>>(),
    })
}

fn event_to_json(event: &EventRecord) -> serde_json::Value {
    json!({
        "name": event.name,
        "timestamp_unix_ns": event.timestamp_unix_ns,
        "properties": properties_to_json(&event.properties),
    })
}

fn properties_to_json(
    properties: &[(std::borrow::Cow<'static, str>, std::borrow::Cow<'static, str>)],
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in properties {
        map.insert(key.to_string(), json!(value));
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::io::{BufRead, BufReader};

    use fastrace::collector::{EventRecord, Reporter, SpanRecord};
    use fastrace::prelude::{SpanId, TraceId};

    use super::JsonlReporter;

    fn sample_span() -> SpanRecord {
        SpanRecord {
            trace_id: TraceId(1),
            span_id: SpanId(2),
            parent_id: SpanId(0),
            begin_time_unix_ns: 100,
            duration_ns: 50,
            name: Cow::Borrowed("elph.test.span"),
            properties: vec![(Cow::Borrowed("key"), Cow::Borrowed("value"))],
            events: vec![EventRecord {
                name: Cow::Borrowed("started"),
                timestamp_unix_ns: 120,
                properties: vec![],
            }],
            links: vec![],
        }
    }

    #[test]
    fn writes_span_records_as_json_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut reporter = JsonlReporter::new(dir.path(), "elph").expect("reporter");
        reporter.report(vec![sample_span()]);
        drop(reporter);

        let file = std::fs::File::open(dir.path().join("elph-traces.jsonl")).expect("trace file");
        let line = BufReader::new(file).lines().next().expect("line").expect("read");
        let value: serde_json::Value = serde_json::from_str(&line).expect("json");
        assert_eq!(value["name"], "elph.test.span");
        assert_eq!(value["duration_ns"], 50);
        assert_eq!(value["properties"]["key"], "value");
        assert_eq!(value["events"][0]["name"], "started");
    }

    #[test]
    fn uses_app_scoped_trace_filename() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reporter = JsonlReporter::new(dir.path(), "elph").expect("reporter");
        assert!(reporter.path().ends_with("elph-traces.jsonl"));
    }
}
