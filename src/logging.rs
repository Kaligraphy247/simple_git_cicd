use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::debug;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_core::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const DEFAULT_MAX_LOG_FILES: usize = 5;
const MAX_LOG_MEMORY_BYTES: usize = 2 * 1024 * 1024; // 2MB

pub type LogLevel = Level;

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub job_id: String,
    pub timestamp: DateTime<Utc>,
    pub source: LogSource,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Clone, Debug)]
pub enum LogSource {
    GitFetch,
    GitPull,
    UserScript,
    SystemEvent,
}

pub struct FileLogger {
    log_directory: PathBuf,
    max_files: usize,
    rotation: Rotation,
}

impl FileLogger {
    pub fn new(log_directory: PathBuf) -> Self {
        Self {
            log_directory,
            max_files: DEFAULT_MAX_LOG_FILES,
            rotation: Rotation::DAILY,
        }
    }

    pub fn with_max_files(mut self, max_files: usize) -> Self {
        self.max_files = max_files;
        self
    }

    pub fn with_rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }

    // pub fn create_file_appender(&self) -> RollingFileAppender {
    //     // first, ensure that log dir. exists
    //     std::fs::create_dir_all(&self.log_directory).expect("Failed to create log directory");
    //     // let (non_blocking, _guard) = tracing_appender::non_blocking(writer)
    //     // Then create rolling file appender
    //     RollingFileAppender::new(self.rotation.to_owned(), &self.log_directory, "cicd_logs")
    // }

    pub fn setup_file_logging(
        &self,
    ) -> (
        tracing_appender::non_blocking::NonBlocking,
        // RollingFileAppender,
        tracing_appender::non_blocking::WorkerGuard,
    ) {
        // Ensure log directory exists
        std::fs::create_dir_all(&self.log_directory).expect("Failed to create log directory");

        // Create a rolling file appender
        let file_appender = RollingFileAppender::new(
            self.rotation.to_owned(),
            &self.log_directory,
            "cicd_logs", // Prefix for log files
        );

        // Create a non-blocking writer
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        // guard
        (non_blocking, guard)
    }
}

pub struct GlobalLogManager {
    logs: VecDeque<LogEntry>,
    max_total_memory_size: usize,
    current_job_id: Option<String>,
}

impl GlobalLogManager {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::new(),
            max_total_memory_size: MAX_LOG_MEMORY_BYTES,
            current_job_id: None,
        }
    }

    pub fn start_new_job(&mut self, job_id: String) {
        // Clear logs when a new job starts
        self.logs.clear();
        self.current_job_id = Some(job_id)
    }

    pub fn add_log_entry(&mut self, mut entry: LogEntry) {
        // Ensure the log entry has the current Job ID
        if let Some(job_id) = &self.current_job_id {
            entry.job_id = job_id.clone();
        }

        // Calculate entry size
        let entry_size = std::mem::size_of::<LogEntry>() + entry.message.len();
        debug!("Entry Size: {}", entry_size);

        // Remove oldest entries if we have exceeded memory limit
        while self.calculate_total_size() + entry_size > self.max_total_memory_size {
            self.logs.pop_front();
        }
    }

    pub fn get_current_job_logs(&self) -> Vec<LogEntry> {
        self.logs.iter().cloned().collect()
    }

    pub fn get_logs_by_level(&self, level: LogLevel) -> Vec<LogEntry> {
        self.logs
            .iter()
            .filter(|entry| entry.level == level)
            .cloned()
            .collect()
    }

    fn calculate_total_size(&self) -> usize {
        self.logs
            .iter()
            .map(|entry| std::mem::size_of::<LogEntry>() + entry.message.len())
            .sum()
    }
}

impl Default for GlobalLogManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ThreadSafeLogManager {
    inner: Arc<Mutex<GlobalLogManager>>,
}

impl Default for ThreadSafeLogManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadSafeLogManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(GlobalLogManager::new())),
        }
    }

    pub fn add_log(&self, event: &Event<'_>, metadata: &Metadata<'_>) {
        // Extract log message
        let mut visitor = LogEntryVisitor::default();
        event.record(&mut visitor);

        let log_entry = LogEntry {
            job_id: String::new(), // Set this in GlobalLogManager
            timestamp: Utc::now(),
            source: self.convert_metadata_to_source(metadata),
            message: visitor.message,
            level: *metadata.level(),
        };

        // Thread-safe log addition
        if let Ok(mut guard) = self.inner.lock() {
            guard.add_log_entry(log_entry);
        }
    }

    pub fn get_inner_log_manager(&self) -> Arc<Mutex<GlobalLogManager>> {
        // self.inner.clone()
        Arc::clone(&self.inner)
    }

    fn convert_metadata_to_source(&self, metadata: &Metadata<'_>) -> LogSource {
        let target = metadata.target();
        match target {
            t if t.contains("git::fetch") => LogSource::GitFetch,
            t if t.contains("git::pull") => LogSource::GitPull,
            t if t.contains("user_script") => LogSource::UserScript,
            _ => LogSource::SystemEvent,
        }
    }
}

/// Helper to extract log message
#[derive(Default)]
struct LogEntryVisitor {
    message: String,
}

/// Implementation of the `Visit` trait for `LogEntryVisitor`.
/// This trait is used to visit and record fields of a log entry.
impl tracing::field::Visit for LogEntryVisitor {
    /// Records a field's value in debug format.
    ///
    /// This method is called for each field in the log entry. If the field's name is "message",
    /// the value is formatted as a string and stored in the `message` field of `LogEntryVisitor`.
    ///
    /// # Arguments
    ///
    /// * `field` - A reference to the field being visited.
    /// * `value` - A reference to the value of the field, which implements the `Debug` trait.
    fn record_debug(&mut self, field: &tracing_core::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

// Custom Tracing Layer
#[derive(Clone, Default)]
pub struct GlobalLogManagerLayer {
    log_manager: Arc<Mutex<ThreadSafeLogManager>>,
}

impl GlobalLogManagerLayer {
    pub fn new() -> Self {
        Self {
            log_manager: Arc::new(Mutex::new(ThreadSafeLogManager::new())),
        }
    }

    // Expose a way to access the underlying log manager
    pub fn get_log_manager(&self) -> Arc<Mutex<GlobalLogManager>> {
        self.log_manager.lock().unwrap().get_inner_log_manager()
    }
}

impl<S: Subscriber> Layer<S> for GlobalLogManagerLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if let Ok(guard) = self.log_manager.lock() {
            guard.add_log(event, event.metadata());
        }
    }
}

pub fn setup_logging() -> GlobalLogManagerLayer {
    let global_log_layer = GlobalLogManagerLayer::new();
    tracing_subscriber::registry()
        .with(global_log_layer.clone())
        .with(tracing_subscriber::fmt::layer()) // Console output
        .init();
    global_log_layer
}

// pub fn setup_logging(file_logger: &FileLogger) -> GlobalLogManagerLayer {
//     let file_appender = file_logger.setup_file_logging();
//     let global_log_layer = GlobalLogManagerLayer::new();

//     tracing_subscriber::registry()
//         .with(global_log_layer.clone())
//         .with(tracing_subscriber::fmt::layer()) // Console output
//         .with(
//             tracing_subscriber::fmt::layer()
//                 .with_writer(file_appender)
//                 .with_ansi(false), // Disable ANSI colors
//         )
//         .init();

//     global_log_layer
// }

// pub fn setup_logging(
//     file_logger: &FileLogger,
// ) -> (
//     GlobalLogManagerLayer,
//     // tracing_appender::rolling::RollingFileAppender,
//     tracing_appender::non_blocking::WorkerGuard,
// ) {
//     let global_log_layer = GlobalLogManagerLayer::new();

//     // Setup non-blocking file logging
//     let (file_writer, file_appender) = file_logger.setup_file_logging();

//     tracing_subscriber::registry()
//         .with(global_log_layer.clone())
//         .with(tracing_subscriber::fmt::layer()) // Console output
//         .with(
//             tracing_subscriber::fmt::layer()
//                 .with_writer(file_writer.clone())
//                 .with_ansi(false), // Disable ANSI colors for file logs
//         )
//         .init();

//     (global_log_layer, file_appender)
// }
