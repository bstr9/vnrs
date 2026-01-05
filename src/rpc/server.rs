//! RPC Server implementation using ZMQ
//!
//! Provides a Request-Reply (REP) socket for handling RPC calls
//! and a Publish-Subscribe (PUB) socket for broadcasting messages.
//! Matches vnpy Python RPC server functionality.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::rpc::common::{
    HEARTBEAT_INTERVAL, POLL_TIMEOUT_MS, RpcMessage,
    RpcRequest, RpcResponse, TCP_KEEPALIVE_IDLE,
};

/// Type alias for registered RPC functions
pub type RpcFunction = Arc<
    dyn Fn(Vec<serde_json::Value>, HashMap<String, serde_json::Value>) -> Result<serde_json::Value, String>
        + Send
        + Sync,
>;

/// RPC Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// REP socket address for request-reply pattern (e.g., "tcp://*:2014")
    pub rep_address: String,
    /// PUB socket address for publish-subscribe pattern (e.g., "tcp://*:4102")
    pub pub_address: String,
    /// Heartbeat interval (default: 10 seconds)
    pub heartbeat_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            rep_address: "tcp://*:2014".to_string(),
            pub_address: "tcp://*:4102".to_string(),
            heartbeat_interval: HEARTBEAT_INTERVAL,
        }
    }
}

/// RPC Server
///
/// Handles incoming RPC requests via REP socket and publishes messages via PUB socket.
/// Maintains a registry of callable functions and sends periodic heartbeats.
pub struct RpcServer {
    /// Configuration for the server
    config: ServerConfig,
    /// Registered RPC functions: method name -> function
    functions: Arc<RwLock<HashMap<String, RpcFunction>>>,
    /// Server active status
    active: Arc<Mutex<bool>>,
    /// ZMQ context
    context: Arc<zmq::Context>,
    /// REP socket for request-reply
    socket_rep: Arc<Mutex<Option<zmq::Socket>>>,
    /// PUB socket for publish-subscribe
    socket_pub: Arc<Mutex<Option<zmq::Socket>>>,
    /// Last heartbeat timestamp
    last_heartbeat: Arc<Mutex<Instant>>,
}

impl RpcServer {
    /// Create a new RPC server with default configuration
    pub fn new() -> Self {
        Self::with_config(ServerConfig::default())
    }

    /// Create a new RPC server with custom configuration
    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            config,
            functions: Arc::new(RwLock::new(HashMap::new())),
            active: Arc::new(Mutex::new(false)),
            context: Arc::new(zmq::Context::new()),
            socket_rep: Arc::new(Mutex::new(None)),
            socket_pub: Arc::new(Mutex::new(None)),
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Check if the server is currently active
    pub fn is_active(&self) -> bool {
        let active = self.active.try_lock();
        match active {
            Ok(guard) => *guard,
            Err(_) => false,
        }
    }

    /// Start the RPC server
    ///
    /// Binds to the configured addresses and starts listening for requests.
    /// This is a non-blocking operation that spawns background tasks.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if already active
        {
            let mut active = self.active.lock().await;
            if *active {
                info!("RPC Server already active");
                return Ok(());
            }
            *active = true;
        }

        // Create and configure REP socket
        let socket_rep = self.context.socket(zmq::REP)?;
        socket_rep.set_tcp_keepalive(1)?;
        socket_rep.set_tcp_keepalive_idle(TCP_KEEPALIVE_IDLE)?;
        socket_rep.set_linger(0)?;
        socket_rep.bind(&self.config.rep_address)?;
        info!("RPC REP socket bound to: {}", self.config.rep_address);

        // Create and configure PUB socket
        let socket_pub = self.context.socket(zmq::PUB)?;
        socket_pub.set_tcp_keepalive(1)?;
        socket_pub.set_tcp_keepalive_idle(TCP_KEEPALIVE_IDLE)?;
        socket_pub.set_linger(0)?;
        socket_pub.bind(&self.config.pub_address)?;
        info!("RPC PUB socket bound to: {}", self.config.pub_address);

        // Store sockets
        {
            let mut rep_guard = self.socket_rep.lock().await;
            *rep_guard = Some(socket_rep);
        }
        {
            let mut pub_guard = self.socket_pub.lock().await;
            *pub_guard = Some(socket_pub);
        }

        // Initialize heartbeat timestamp
        {
            let mut last_heartbeat = self.last_heartbeat.lock().await;
            *last_heartbeat = Instant::now() + self.config.heartbeat_interval;
        }

        // Spawn request handler task
        self.spawn_request_handler().await;

        // Spawn heartbeat task
        self.spawn_heartbeat_task().await;

        info!("RPC Server started successfully");
        Ok(())
    }

    /// Stop the RPC server
    ///
    /// Gracefully shuts down the server and disconnects all sockets.
    pub async fn stop(&self) {
        info!("Stopping RPC Server");

        // Set active to false
        {
            let mut active = self.active.lock().await;
            *active = false;
        }

        // Disconnect sockets
        {
            let mut rep_guard = self.socket_rep.lock().await;
            if let Some(socket) = rep_guard.take() {
                if let Err(e) = socket.disconnect(&self.config.rep_address) {
                    warn!("Error disconnecting REP socket: {}", e);
                }
            }
        }
        {
            let mut pub_guard = self.socket_pub.lock().await;
            if let Some(socket) = pub_guard.take() {
                if let Err(e) = socket.disconnect(&self.config.pub_address) {
                    warn!("Error disconnecting PUB socket: {}", e);
                }
            }
        }

        info!("RPC Server stopped");
    }

    /// Register a callable function
    ///
    /// The function will be available for RPC calls with the given name.
    pub async fn register<F>(&self, name: String, func: F)
    where
        F: Fn(Vec<serde_json::Value>, HashMap<String, serde_json::Value>) -> Result<serde_json::Value, String>
            + Send
            + Sync
            + 'static,
    {
        let mut functions = self.functions.write().await;
        functions.insert(name.clone(), Arc::new(func));
        debug!("Registered RPC function: {}", name);
    }

    /// Publish a message to all subscribers
    ///
    /// Messages are sent with a topic for filtering on the client side.
    pub async fn publish(&self, topic: String, data: serde_json::Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let socket_pub = {
            let pub_guard = self.socket_pub.lock().await;
            pub_guard.as_ref().map(|s| s as *const zmq::Socket)
        };

        if let Some(socket_ptr) = socket_pub {
            let socket = unsafe { &*socket_ptr };
            let message = RpcMessage::new(topic, data);
            let serialized = serde_json::to_vec(&message)?;
            socket.send(&serialized, 0)?;
            debug!("Published message to topic: {}", message.topic);
        } else {
            warn!("Attempted to publish but PUB socket not initialized");
        }

        Ok(())
    }

    /// Spawn the request handler task
    async fn spawn_request_handler(&self) {
        let functions = self.functions.clone();
        let active = self.active.clone();
        let socket_rep = self.socket_rep.clone();

        tokio::spawn(async move {
            loop {
                // Check if still active
                {
                    let active_guard = active.lock().await;
                    if !*active_guard {
                        break;
                    }
                }

                // Get socket reference
                let socket_ptr = {
                    let rep_guard = socket_rep.lock().await;
                    rep_guard.as_ref().map(|s| s as *const zmq::Socket)
                };

                if let Some(socket_ptr) = socket_ptr {
                    // Use unsafe to get reference - this is safe because we hold the lock
                    let socket = unsafe { &*socket_ptr };
                    
                    // Poll for incoming requests with timeout
                    match socket.poll(zmq::PollEvents::POLLIN, POLL_TIMEOUT_MS as i64) {
                        Ok(n) if n > 0 => {
                            // Receive request
                            let data = match socket.recv_bytes(0) {
                                Ok(d) => d,
                                Err(e) => {
                                    error!("Failed to receive request: {}", e);
                                    continue;
                                }
                            };

                            // Deserialize request
                            let req = match serde_json::from_slice::<RpcRequest>(&data) {
                                Ok(r) => r,
                                Err(e) => {
                                    error!("Failed to deserialize request: {}", e);
                                    let error_response = RpcResponse::failure(format!(
                                        "Invalid request format: {}",
                                        e
                                    ));
                                    if let Ok(resp_data) = serde_json::to_vec(&error_response) {
                                        let _ = socket.send(&resp_data, 0);
                                    }
                                    continue;
                                }
                            };

                            debug!("Received RPC request: {}", req.method);

                            // Execute function (this is the async part)
                            let response = {
                                let func_map = functions.read().await;
                                if let Some(func) = func_map.get(&req.method) {
                                    match func(req.args.clone(), req.kwargs.clone()) {
                                        Ok(result) => RpcResponse::success(result),
                                        Err(e) => RpcResponse::failure(e),
                                    }
                                } else {
                                    RpcResponse::failure(format!(
                                        "Function '{}' not found",
                                        req.method
                                    ))
                                }
                            };

                            // Send response (need to get socket again after await)
                            let socket_ptr = {
                                let rep_guard = socket_rep.lock().await;
                                rep_guard.as_ref().map(|s| s as *const zmq::Socket)
                            };

                            if let Some(socket_ptr) = socket_ptr {
                                let socket = unsafe { &*socket_ptr };
                                match serde_json::to_vec(&response) {
                                    Ok(resp_data) => {
                                        if let Err(e) = socket.send(&resp_data, 0) {
                                            error!("Failed to send response: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to serialize response: {}", e);
                                    }
                                }
                            }
                        }
                        Ok(_) => {
                            // No data available, continue polling
                        }
                        Err(e) => {
                            error!("Socket poll error: {}", e);
                        }
                    }
                } else {
                    // Socket not initialized, wait a bit
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
    }

    /// Spawn the heartbeat task
    async fn spawn_heartbeat_task(&self) {
        let active = self.active.clone();
        let socket_pub = self.socket_pub.clone();
        let last_heartbeat = self.last_heartbeat.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let heartbeat_topic = super::common::HEARTBEAT_TOPIC.to_string();

        tokio::spawn(async move {
            loop {
                // Check if still active
                {
                    let active_guard = active.lock().await;
                    if !*active_guard {
                        break;
                    }
                }

                // Check if it's time to send heartbeat
                let should_send = {
                    let last = last_heartbeat.lock().await;
                    Instant::now().duration_since(*last) >= heartbeat_interval
                };

                if should_send {
                    // Update timestamp
                    {
                        let mut last = last_heartbeat.lock().await;
                        *last = Instant::now() + heartbeat_interval;
                    }

                    // Send heartbeat
                    let socket = {
                        let pub_guard = socket_pub.lock().await;
                        pub_guard.as_ref().map(|s| s as *const zmq::Socket)
                    };

                    if let Some(socket_ptr) = socket {
                        let socket = unsafe { &*socket_ptr };
                        let timestamp = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
                        let message = RpcMessage {
                            topic: heartbeat_topic.clone(),
                            data: serde_json::json!(timestamp),
                        };
                        match serde_json::to_vec(&message) {
                            Ok(data) => {
                                if let Err(e) = socket.send(&data, 0) {
                                    error!("Failed to send heartbeat: {}", e);
                                } else {
                                    debug!("Sent heartbeat");
                                }
                            }
                            Err(e) => {
                                error!("Failed to serialize heartbeat: {}", e);
                            }
                        }
                    }
                }

                // Sleep for a short interval
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    /// Check if heartbeat should be sent
    pub async fn check_heartbeat(&self) -> bool {
        let last = self.last_heartbeat.lock().await;
        Instant::now().duration_since(*last) >= self.config.heartbeat_interval
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}