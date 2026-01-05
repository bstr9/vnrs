//! Common constants and types for RPC communication
//!
//! This module provides shared types and constants for the RPC client-server system,
//! matching the vnpy Python RPC implementation functionality.

use serde::{Deserialize, Serialize};
use std::time::Duration;

// Heartbeat topic name - must match Python vnpy
pub const HEARTBEAT_TOPIC: &str = "heartbeat";

// Heartbeat interval in seconds (server sends heartbeat every 10 seconds)
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

// Heartbeat tolerance in seconds (client will disconnect if no heartbeat for 30 seconds)
pub const HEARTBEAT_TOLERANCE: Duration = Duration::from_secs(30);

/// RPC timeout for request-response calls (default 30 seconds)
pub const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Socket poll timeout in milliseconds (1 second)
pub const POLL_TIMEOUT_MS: i32 = 1000;

/// TCP keepalive idle time in seconds
pub const TCP_KEEPALIVE_IDLE: i32 = 60;

/// RPC Request sent from client to server
/// Format: [method_name, args, kwargs]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Name of the RPC method to call
    pub method: String,
    /// Positional arguments
    pub args: Vec<serde_json::Value>,
    /// Keyword arguments
    pub kwargs: std::collections::HashMap<String, serde_json::Value>,
}

impl RpcRequest {
    /// Create a new RPC request
    pub fn new(method: String, args: Vec<serde_json::Value>, kwargs: std::collections::HashMap<String, serde_json::Value>) -> Self {
        Self { method, args, kwargs }
    }
}

/// RPC Response sent from server to client
/// Format: [success, data/error_message]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    /// Whether the RPC call succeeded
    pub success: bool,
    /// Result data if success, or error message if failure
    pub data: serde_json::Value,
}

impl RpcResponse {
    /// Create a successful response
    pub fn success(data: serde_json::Value) -> Self {
        Self { success: true, data }
    }

    /// Create a failure response
    pub fn failure(error_message: String) -> Self {
        Self { success: false, data: serde_json::json!(error_message) }
    }
}

/// Published message from server to subscribers
/// Format: [topic, data]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcMessage {
    /// Topic name for filtering subscriptions
    pub topic: String,
    /// Message data
    pub data: serde_json::Value,
}

impl RpcMessage {
    /// Create a new RPC message
    pub fn new(topic: String, data: serde_json::Value) -> Self {
        Self { topic, data }
    }

    /// Create a heartbeat message
    pub fn heartbeat(timestamp: f64) -> Self {
        Self {
            topic: HEARTBEAT_TOPIC.to_string(),
            data: serde_json::json!(timestamp),
        }
    }
}

/// Remote exception representing errors from RPC calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteException {
    /// Error message from the remote side
    pub message: String,
}

impl RemoteException {
    /// Create a new remote exception
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for RemoteException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RemoteException: {}", self.message)
    }
}

impl std::error::Error for RemoteException {}

/// Timeout error for RPC calls
#[derive(Debug, Clone)]
pub struct TimeoutError {
    /// Timeout duration in milliseconds
    pub timeout_ms: u64,
    /// Request that timed out
    pub request: RpcRequest,
}

impl TimeoutError {
    /// Create a new timeout error
    pub fn new(timeout_ms: u64, request: RpcRequest) -> Self {
        Self { timeout_ms, request }
    }
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Timeout of {}ms reached for request: {}",
            self.timeout_ms, self.request.method
        )
    }
}

impl std::error::Error for TimeoutError {}

/// Connection error for RPC
#[derive(Debug, Clone)]
pub enum ConnectionError {
    /// Server disconnected (heartbeat timeout)
    Disconnected,
    /// Failed to connect to server
    ConnectionFailed(String),
    /// Socket error
    SocketError(String),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Disconnected => {
                write!(f, "RPC Server disconnected (heartbeat timeout)")
            }
            ConnectionError::ConnectionFailed(msg) => {
                write!(f, "Failed to connect to RPC server: {}", msg)
            }
            ConnectionError::SocketError(msg) => {
                write!(f, "RPC socket error: {}", msg)
            }
        }
    }
}

impl std::error::Error for ConnectionError {}