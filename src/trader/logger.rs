//! Logging module for the trading platform.

use chrono::Local;
use std::fs::{self, OpenOptions};

use std::path::PathBuf;
use std::sync::LazyLock;
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

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

/// Check if JSON log format is enabled via the `VNRS_LOG_FORMAT` environment variable.
fn is_json_format() -> bool {
    std::env::var("VNRS_LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json"))
}

/// Initialize the logger with optional JSON format.
///
/// When `force_json` is `true`, JSON format is used regardless of the environment variable.
/// When `force_json` is `false`, the `VNRS_LOG_FORMAT=json` environment variable is consulted.
fn init_logger_inner(force_json: bool) {
    let json = force_json || is_json_format();
    let log_level = SETTINGS.get_int("log.level").unwrap_or(INFO as i64) as i32;
    let log_console = SETTINGS.get_bool("log.console").unwrap_or(true);
    let log_file = SETTINGS.get_bool("log.file").unwrap_or(true);

    let level = level_from_int(log_level);
    let filter = EnvFilter::from_default_env().add_directive(level.into());

    let subscriber = tracing_subscriber::registry().with(filter);

    if log_console {
        if json {
            let fmt_layer = fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_ansi(true);

            if log_file {
                let log_path = get_log_file_path();

                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                let file = OpenOptions::new().create(true).append(true).open(&log_path);

                match file {
                    Ok(f) => {
                        let file_layer = fmt::layer()
                            .json()
                            .with_writer(std::sync::Mutex::new(f))
                            .with_ansi(false);
                        subscriber.with(fmt_layer).with(file_layer).init();
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to open log file {:?}: {}. Falling back to console only.",
                            log_path, e
                        );
                        subscriber.with(fmt_layer).init();
                    }
                }
            } else {
                subscriber.with(fmt_layer).init();
            }
        } else {
            let fmt_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_ansi(true);

            if log_file {
                let log_path = get_log_file_path();

                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                let file = OpenOptions::new().create(true).append(true).open(&log_path);

                match file {
                    Ok(f) => {
                        let file_layer = fmt::layer()
                            .with_writer(std::sync::Mutex::new(f))
                            .with_ansi(false);
                        subscriber.with(fmt_layer).with(file_layer).init();
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to open log file {:?}: {}. Falling back to console only.",
                            log_path, e
                        );
                        subscriber.with(fmt_layer).init();
                    }
                }
            } else {
                subscriber.with(fmt_layer).init();
            }
        }
    } else if log_file {
        let log_path = get_log_file_path();

        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = OpenOptions::new().create(true).append(true).open(&log_path);

        match file {
            Ok(f) => {
                if json {
                    let file_layer = fmt::layer()
                        .json()
                        .with_writer(std::sync::Mutex::new(f))
                        .with_ansi(false);
                    subscriber.with(file_layer).init();
                } else {
                    let file_layer = fmt::layer()
                        .with_writer(std::sync::Mutex::new(f))
                        .with_ansi(false);
                    subscriber.with(file_layer).init();
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to open log file {:?}: {}. Falling back to console only.",
                    log_path, e
                );
                let console_layer = fmt::layer();
                subscriber.with(console_layer).init();
            }
        }
    }
}

/// Initialize the logger.
///
/// Checks the `VNRS_LOG_FORMAT` environment variable: if set to `"json"`,
/// all log layers output in JSON format. Otherwise, the default human-readable
/// format is used.
pub fn init_logger() {
    init_logger_inner(false);
}

/// Initialize the logger with JSON format explicitly enabled.
///
/// This forces JSON output on all log layers regardless of the `VNRS_LOG_FORMAT`
/// environment variable.
pub fn init_logger_with_json() {
    init_logger_inner(true);
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
    use std::env;

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

    #[test]
    fn test_is_json_format() {
        // Save original value if any
        let original = env::var("VNRS_LOG_FORMAT").ok();

        // Test: env var not set -> false
        env::remove_var("VNRS_LOG_FORMAT");
        assert!(!is_json_format());

        // Test: env var set to "json" (lowercase) -> true
        env::set_var("VNRS_LOG_FORMAT", "json");
        assert!(is_json_format());

        // Test: env var set to "JSON" (uppercase) -> true
        env::set_var("VNRS_LOG_FORMAT", "JSON");
        assert!(is_json_format());

        // Test: env var set to "Json" (mixed case) -> true
        env::set_var("VNRS_LOG_FORMAT", "Json");
        assert!(is_json_format());

        // Test: env var set to other value -> false
        env::set_var("VNRS_LOG_FORMAT", "text");
        assert!(!is_json_format());

        // Restore original value
        if let Some(val) = original {
            env::set_var("VNRS_LOG_FORMAT", val);
        } else {
            env::remove_var("VNRS_LOG_FORMAT");
        }
    }
}
