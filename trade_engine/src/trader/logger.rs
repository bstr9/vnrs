//! Logging module for the trading platform.

use chrono::Local;
use std::fs::{self, OpenOptions};

use std::path::PathBuf;
use std::sync::LazyLock;
use tracing::Level;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

use super::setting::SETTINGS;
use super::utility::get_folder_path;

/// Log level constants (compatible with Python logging module)
pub const DEBUG: i32 = 10;
pub const INFO: i32 = 20;
pub const WARNING: i32 = 30;
pub const ERROR: i32 = 40;
pub const CRITICAL: i32 = 50;

/// Convert integer log level to tracing Level
pub fn level_from_int(level: i32) -> Level {
    match level {
        0..=10 => Level::DEBUG,
        11..=20 => Level::INFO,
        21..=30 => Level::WARN,
        _ => Level::ERROR,
    }
}

/// Convert integer log level to string
pub fn level_to_string(level: i32) -> &'static str {
    match level {
        0..=10 => "DEBUG",
        11..=20 => "INFO",
        21..=30 => "WARNING",
        31..=40 => "ERROR",
        _ => "CRITICAL",
    }
}

/// Initialize the logger
pub fn init_logger() {
    let log_level = SETTINGS.get_int("log.level").unwrap_or(INFO as i64) as i32;
    let log_console = SETTINGS.get_bool("log.console").unwrap_or(true);
    let log_file = SETTINGS.get_bool("log.file").unwrap_or(true);

    let level = level_from_int(log_level);
    let filter = EnvFilter::from_default_env()
        .add_directive(level.into());

    let subscriber = tracing_subscriber::registry().with(filter);

    if log_console {
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_ansi(true);

        if log_file {
            let log_path = get_log_file_path();
            
            // Create log file if needed
            if let Some(parent) = log_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .expect("Failed to open log file");

            let file_layer = fmt::layer()
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false);

            subscriber
                .with(fmt_layer)
                .with(file_layer)
                .init();
        } else {
            subscriber.with(fmt_layer).init();
        }
    } else if log_file {
        let log_path = get_log_file_path();
        
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("Failed to open log file");

        let file_layer = fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false);

        subscriber.with(file_layer).init();
    }
}

/// Get the log file path for today
fn get_log_file_path() -> PathBuf {
    let log_folder = get_folder_path("log");
    let today = Local::now().format("%Y%m%d").to_string();
    let filename = format!("vt_{}.log", today);
    log_folder.join(filename)
}

/// Simple logger for writing log messages
pub struct Logger {
    pub name: String,
}

impl Logger {
    /// Create a new logger with a name
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Log a debug message
    pub fn debug(&self, msg: &str) {
        tracing::debug!(logger = %self.name, "{}", msg);
    }

    /// Log an info message
    pub fn info(&self, msg: &str) {
        tracing::info!(logger = %self.name, "{}", msg);
    }

    /// Log a warning message
    pub fn warn(&self, msg: &str) {
        tracing::warn!(logger = %self.name, "{}", msg);
    }

    /// Log an error message
    pub fn error(&self, msg: &str) {
        tracing::error!(logger = %self.name, "{}", msg);
    }

    /// Log a message with specific level
    pub fn log(&self, level: i32, msg: &str) {
        match level {
            0..=10 => self.debug(msg),
            11..=20 => self.info(msg),
            21..=30 => self.warn(msg),
            _ => self.error(msg),
        }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new("Logger")
    }
}

/// Global logger instance
pub static LOGGER: LazyLock<Logger> = LazyLock::new(Logger::default);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_from_int() {
        assert_eq!(level_from_int(DEBUG), Level::DEBUG);
        assert_eq!(level_from_int(INFO), Level::INFO);
        assert_eq!(level_from_int(WARNING), Level::WARN);
        assert_eq!(level_from_int(ERROR), Level::ERROR);
    }

    #[test]
    fn test_level_to_string() {
        assert_eq!(level_to_string(DEBUG), "DEBUG");
        assert_eq!(level_to_string(INFO), "INFO");
        assert_eq!(level_to_string(WARNING), "WARNING");
        assert_eq!(level_to_string(ERROR), "ERROR");
        assert_eq!(level_to_string(CRITICAL), "CRITICAL");
    }

    #[test]
    fn test_logger_new() {
        let logger = Logger::new("TestLogger");
        assert_eq!(logger.name, "TestLogger");
    }
}
