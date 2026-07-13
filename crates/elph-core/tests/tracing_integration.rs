#![cfg(feature = "tracing")]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Duration;

use elph_core::logger::LoggingOptions;
use elph_core::trace::{JsonlReporter, flush, root_span, set_reporter};
use fastrace::collector::Config;
use fastrace::local::LocalSpan;

#[test]
fn root_span_flushes_to_jsonl_reporter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let reporter = JsonlReporter::new(dir.path(), "elph").expect("reporter");
    let trace_path = reporter.path().to_path_buf();
    set_reporter(reporter, Config::default().report_interval(Duration::from_millis(10)));

    {
        let _guard = root_span("elph.test.root");
        let _child = LocalSpan::enter_with_local_parent("elph.test.child");
    }
    flush();

    let names = read_span_names(&trace_path);
    assert!(names.iter().any(|name| name == "elph.test.root"));
    assert!(names.iter().any(|name| name == "elph.test.child"));
}

#[test]
fn init_skips_reporter_when_trace_disabled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let options = LoggingOptions {
        app_name: "elph",
        logs_dir: dir.path().to_path_buf(),
        level: "info".to_string(),
        rotation: elph_core::logger::LogRotation::Daily,
        max_files: None,
        file_enabled: false,
        console_enabled: false,
        trace_enabled: false,
    };
    elph_core::trace::init(&options);
    assert!(!dir.path().join("elph-traces.jsonl").exists());
}

fn read_span_names(path: &PathBuf) -> Vec<String> {
    let file = std::fs::File::open(path).expect("trace file");
    BufReader::new(file)
        .lines()
        .map(|line| {
            let line = line.expect("line");
            let value: serde_json::Value = serde_json::from_str(&line).expect("json");
            value["name"].as_str().expect("name").to_string()
        })
        .collect()
}
