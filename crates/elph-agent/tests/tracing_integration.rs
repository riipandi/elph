#![cfg(feature = "tracing")]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Duration;

use elph_agent::logger::LoggingOptions;
use elph_agent::trace::JsonlReporter;
use elph_agent::trace::{Span, SpanContext, flush, is_enabled, root_span, set_reporter};
use fastrace::collector::Config;
use fastrace::local::LocalSpan;

#[test]
fn jsonl_reporter_records_root_batch_and_property_spans() {
    // `set_reporter` installs a process-global fastrace reporter, so reporter-backed
    // scenarios must run serially in one test instead of in parallel threads that
    // would overwrite each other's collector mid-flight.
    let dir = tempfile::tempdir().expect("tempdir");
    let reporter = JsonlReporter::new(dir.path(), "elph").expect("reporter");
    let trace_path = reporter.path().to_path_buf();
    set_reporter(reporter, Config::default().report_interval(Duration::from_millis(10)));
    assert!(is_enabled());

    // Root span flushes with its local child.
    {
        let _guard = root_span("elph.test.root");
        let _child = LocalSpan::enter_with_local_parent("elph.test.child");
    }
    flush();
    let names = read_span_names(&trace_path);
    assert!(names.iter().any(|name| name == "elph.test.root"));
    assert!(names.iter().any(|name| name == "elph.test.child"));

    // Repeated root spans batch into the same file.
    for _ in 0..3 {
        let _guard = root_span("elph.test.batch");
        let _child = LocalSpan::enter_with_local_parent("elph.test.work");
    }
    flush();
    let names = read_span_names(&trace_path);
    assert!(names.iter().filter(|n| *n == "elph.test.batch").count() >= 3);

    // Span properties surface as JSONL records.
    {
        let span = Span::root("elph.test.props", SpanContext::random());
        span.set_local_parent();
    }
    flush();
    let file = std::fs::File::open(&trace_path).expect("trace file");
    let lines: Vec<String> = std::io::BufReader::new(file)
        .lines()
        .map(|l| l.expect("line"))
        .collect();
    assert!(!lines.is_empty(), "should have at least one span record");
}

#[test]
fn init_skips_reporter_when_trace_disabled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let options = LoggingOptions::builder()
        .app_name("elph")
        .logs_dir(dir.path().to_path_buf())
        .file_enabled(false)
        .console_enabled(false)
        .trace_enabled(false)
        .build();
    elph_agent::trace::init(&options);
    assert!(!is_enabled());
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
