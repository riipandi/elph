use std::fs::{self};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::io::{self};
use std::path::PathBuf;

use fastrace::collector::{EventRecord, Reporter, SpanRecord};
use parking_lot::Mutex as ParkingMutex;
use serde_json::json;

/// Writes collected span trees as JSON lines under the application logs directory.
pub struct JsonlReporter {
    path: PathBuf,
    file: ParkingMutex<File>,
}

impl JsonlReporter {
    pub fn new(logs_dir: &std::path::Path, app_name: &str) -> io::Result<Self> {
        fs::create_dir_all(logs_dir)?;
        let path = logs_dir.join(format!("{app_name}-traces.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            file: ParkingMutex::new(file),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Reporter for JsonlReporter {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        let mut file = self.file.lock();
        for span in spans {
            let line = match serde_json::to_string(&span_to_json(&span)) {
                Ok(line) => line,
                Err(_) => continue,
            };
            if writeln!(file, "{line}").is_err() {
                break;
            }
        }
        let _ = file.flush();
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

        let file = std::fs::File::open(reporter.path()).expect("trace file");
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
