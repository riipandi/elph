//! Typed JSONL log record + logforth layout.

use std::io::Cursor;

use logforth::Error;
use logforth::diagnostic::Diagnostic;
use logforth::kv::KeyView;
use logforth::kv::ValueView;
use logforth::kv::Visitor;
use logforth::layout::Layout;
use logforth::record::Record;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_jsonlines::JsonLinesWriter;

/// One application log line (`APP_DATA/logs/{app}.jsonl`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogRecord {
    pub ts: String,
    pub level: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub kv: Map<String, serde_json::Value>,
}

impl LogRecord {
    pub fn from_logforth(record: &Record<'_>, diags: &[Box<dyn Diagnostic>]) -> Result<Self, Error> {
        let ts = chrono::DateTime::<chrono::Utc>::from(record.time())
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        let mut kv = Map::new();
        let mut visitor = KvCollector { kv: &mut kv };
        record.key_values().visit(&mut visitor)?;
        for diagnostic in diags {
            diagnostic.visit(&mut visitor)?;
        }

        let thread = std::thread::current().name().map(str::to_string);

        Ok(Self {
            ts,
            level: record.level().to_string(),
            target: record.target().to_string(),
            module: record.module_path().map(str::to_string),
            file: record.file().map(str::to_string),
            line: record.line(),
            message: record.payload().to_string(),
            thread,
            kv,
        })
    }

    /// Serialize as one JSON line (no trailing newline — the file appender adds it).
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = JsonLinesWriter::new(&mut cursor);
            writer
                .write(self)
                .map_err(|err| Error::new("serialize log record").with_source(err))?;
            writer
                .flush()
                .map_err(|err| Error::new("flush log record").with_source(err))?;
        }
        let mut bytes = cursor.into_inner();
        while bytes.last() == Some(&b'\n') || bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        Ok(bytes)
    }
}

/// logforth layout that emits [`LogRecord`] JSON.
#[derive(Debug, Default, Clone)]
pub struct JsonlLayout;

impl Layout for JsonlLayout {
    fn format(&self, record: &Record, diags: &[Box<dyn Diagnostic>]) -> Result<Vec<u8>, Error> {
        LogRecord::from_logforth(record, diags)?.to_json_bytes()
    }
}

struct KvCollector<'a> {
    kv: &'a mut Map<String, serde_json::Value>,
}

impl Visitor for KvCollector<'_> {
    fn visit(&mut self, key: KeyView, value: ValueView) -> Result<(), Error> {
        let key = key.to_string();
        match serde_json::to_value(&value) {
            Ok(value) => self.kv.insert(key, value),
            Err(_) => self.kv.insert(key, value.to_string().into()),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_jsonlines::JsonLinesReader;
    use std::io::Cursor;

    #[test]
    fn jsonl_roundtrip_via_serde_jsonlines() {
        let record = LogRecord {
            ts: "2026-08-18T12:00:00.123Z".into(),
            level: "INFO".into(),
            target: "elph_agent::session".into(),
            module: Some("elph_agent::session".into()),
            file: Some("mod.rs".into()),
            line: Some(225),
            message: "release session lease".into(),
            thread: Some("main".into()),
            kv: Map::new(),
        };
        let mut buf = Vec::new();
        {
            let mut writer = JsonLinesWriter::new(&mut buf);
            writer.write(&record).expect("write");
            writer.flush().expect("flush");
        }
        let mut reader = JsonLinesReader::new(Cursor::new(buf));
        let back: LogRecord = reader.read().expect("read").expect("eof");
        assert_eq!(back, record);
    }
}
