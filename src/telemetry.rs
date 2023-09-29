use std::io::{self, Stderr, StderrLock, Stdout, StdoutLock};

use tracing::{Level, Metadata};

use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing_bunyan_formatter::JsonStorageLayer;
use tracing_subscriber::{
    filter::LevelFilter,
    fmt::{writer::MakeWriterExt, MakeWriter},
    EnvFilter,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tracing_bunyan_formatter::BunyanFormattingLayer;

pub struct LogRecorder {
    stdout: Stdout,
    stderr: Stderr,
}

impl LogRecorder {
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }
}

pub enum StdioLock<'a> {
    Stdout(StdoutLock<'a>),
    Stderr(StderrLock<'a>),
}

impl<'a> io::Write for StdioLock<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            StdioLock::Stdout(lock) => lock.write(buf),
            StdioLock::Stderr(lock) => lock.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            StdioLock::Stdout(lock) => lock.write_all(buf),
            StdioLock::Stderr(lock) => lock.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            StdioLock::Stdout(lock) => lock.flush(),
            StdioLock::Stderr(lock) => lock.flush(),
        }
    }
}

impl<'a> MakeWriter<'a> for LogRecorder {
    type Writer = StdioLock<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        StdioLock::Stdout(self.stdout.lock())
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        if meta.level() <= &Level::WARN {
            return StdioLock::Stderr(self.stderr.lock());
        }
        StdioLock::Stdout(self.stdout.lock())
    }
}
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "debug".into())
        .max_level_hint();

    let level = match filter {
        Some(LevelFilter::OFF) | None => None,
        Some(LevelFilter::ERROR) => Some(Level::ERROR),
        Some(LevelFilter::WARN) => Some(Level::WARN),
        Some(LevelFilter::INFO) => Some(Level::INFO),
        Some(LevelFilter::DEBUG) => Some(Level::DEBUG),
        Some(LevelFilter::TRACE) => Some(Level::TRACE),
    };

    if let Some(level) = level {
        tracing_subscriber::registry()
            .with(LevelFilter::TRACE)
            .with(JsonStorageLayer)
            .with(BunyanFormattingLayer::new(
                "webserver".into(),
                LogRecorder::new().with_max_level(level),
            ))
            // .with(
            //     // use tracing_subscriber stdout without bunyan
            //     tracing_subscriber::fmt::layer()
            //         .json()
            //         .with_writer(LogRecorder::new())
            //         .with_ansi(false),
            // )
            .init();
    }
}

pub fn http_trace_layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}
