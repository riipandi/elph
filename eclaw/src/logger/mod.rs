mod rotation;

use elph_agent::LoggingOptions;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub use rotation::build_writer;

/// Bounded queue for the async file writer. Caps memory under sustained log bursts
/// (default upstream limit is 128_000 lines).
const FILE_WRITER_BUFFER_LINES: usize = 16_384;

/// Initializes the global tracing subscriber.
///
/// Returns a [`WorkerGuard`] that must be kept alive for the process lifetime so
/// the non-blocking file writer can flush buffered records.
pub fn init(options: LoggingOptions) -> Option<WorkerGuard> {
    if cfg!(test) {
        return None;
    }

    install_subscriber(&options)
}

fn non_blocking_writer(options: &LoggingOptions) -> (tracing_appender::non_blocking::NonBlocking, WorkerGuard) {
    let writer = build_writer(&options.logs_dir, options.app_name, options.rotation, options.max_files)
        .expect("failed to initialize rolling log writer");
    NonBlockingBuilder::default()
        .buffered_lines_limit(FILE_WRITER_BUFFER_LINES)
        .finish(writer)
}

fn install_subscriber(options: &LoggingOptions) -> Option<WorkerGuard> {
    let env_filter = EnvFilter::new(options.level.clone());

    match (options.file_enabled, options.console_enabled) {
        (true, true) => {
            let (non_blocking, guard) = non_blocking_writer(options);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_writer(non_blocking)
                        .with_ansi(false)
                        .with_target(true),
                )
                .with(
                    fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_target(true)
                        .with_ansi(true),
                )
                .init();

            Some(guard)
        }
        (true, false) => {
            let (non_blocking, guard) = non_blocking_writer(options);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_writer(non_blocking)
                        .with_ansi(false)
                        .with_target(true),
                )
                .init();

            Some(guard)
        }
        (false, true) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_target(true)
                        .with_ansi(true),
                )
                .init();

            None
        }
        (false, false) => {
            tracing_subscriber::registry().with(env_filter).init();
            None
        }
    }
}
