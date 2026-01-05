//! Logger module for alpha research framework
//! Provides logging functionality for alpha research tasks using tracing

use tracing::{info, error, warn, debug, Level};

/// Alpha logger instance using tracing
#[derive(Clone, Copy)]
#[derive(Debug)]
pub struct AlphaLogger;

impl AlphaLogger {
    /// Log info message
    pub fn info(&self, message: &str) {
        info!("{}", message);
    }

    /// Log error message
    pub fn error(&self, message: &str) {
        error!("{}", message);
    }

    /// Log warning message
    pub fn warning(&self, message: &str) {
        warn!("{}", message);
    }

    /// Log debug message
    pub fn debug(&self, message: &str) {
        debug!("{}", message);
    }
}

/// Get the global logger instance
pub fn logger() -> AlphaLogger {
    AlphaLogger
}

/// Initialize the global logger
pub fn init_logger() {
    // Initialize tracing subscriber
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .finish();
    
    // This is a simplified initialization
    // In production, you might want to use tracing_subscriber::registry()
    let _ = subscriber;
}