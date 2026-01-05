//! RPC Client implementation using ZMQ
//!
//! Provides a Request (REQ) socket for making RPC calls
//! and a Subscribe (SUB) socket for receiving published messages.
//! Matches vnpy Python RPC client functionality.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::rpc::common::{
    HEARTBEAT_TOLERANCE, HEARTBEAT_TOPIC, POLL_TIMEOUT_MS, RPC_TIMEOUT, TCP_KEEPALIVE_IDLE,
    ConnectionError, RemoteException, RpcMessage, RpcRequest, RpcResponse, TimeoutError,
};

/// RPC Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// REQ socket address for request-reply pattern (e.g., "tcp://localhost:2014")
    pub req_address: String,
    /// SUB socket address for publish-subscribe pattern (e.g., "tcp://localhost:4102")
    pub sub_address: String,
    /// Timeout for RPC calls in milliseconds (default: 30000ms)
    pub timeout_ms: u64,
    /// Heartbeat tolerance in seconds (default: 30 seconds)
    pub heartbeat_tolerance: Duration,
    /// LRU cache size for method proxies (default: 100)
    pub cache_size: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            req_address: "tcp://localhost:2014".to_string(),
            sub_address: "tcp://localhost:4102".to_string(),
            timeout_ms: RPC_TIMEOUT.as_millis() as u64,
            heartbeat_tolerance: HEARTBEAT_TOLERANCE,
            cache_size: 100,
        }
    }
}

/// Callback type for handling received messages
pub type MessageCallback = Arc<dyn Fn(String, serde_json::Value) + Send + Sync>;

/// RPC Client
///
/// Connects to an RPC server and provides:
/// - Remote method invocation via REQ socket
/// - Message subscription via SUB socket
/// - Automatic heartbeat monitoring
/// - Connection state management
pub struct RpcClient {
    /// Configuration for the client
    config: ClientConfig,
    /// Client active status
    active: Arc<Mutex<bool>>,
    /// ZMQ context
    context: Arc<zmq::Context>,
    /// REQ socket for request-reply
    socket_req: Arc<Mutex<Option<zmq::Socket>>>,
    /// SUB socket for publish-subscribe
    socket_sub: Arc<Mutex<Option<zmq::Socket>>>,
    /// Last received heartbeat timestamp
    last_received_ping: Arc<Mutex<Instant>>,
    /// Lock for REQ socket operations
    req_lock: Arc<Mutex<()>>,
    /// Callback for received messages
    callback: Arc<Mutex<Option<MessageCallback>>>,
    /// Subscribed topics
    subscribed_topics: Arc<Mutex<Vec<String>>>,
}

impl RpcClient {
    /// Create a new RPC client with default configuration
    pub fn new() -> Self {
        Self::with_config(ClientConfig::default())
    }

    /// Create a new RPC client with custom configuration
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            config,
            active: Arc::new(Mutex::new(false)),
            context: Arc::new(zmq::Context::new()),
            socket_req: Arc::new(Mutex::new(None)),
            socket_sub: Arc::new(Mutex::new(None)),
            last_received_ping: Arc::new(Mutex::new(Instant::now())),
            req_lock: Arc::new(Mutex::new(())),
            callback: Arc::new(Mutex::new(None)),
            subscribed_topics: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if the client is currently active
    pub fn is_active(&self) -> bool {
        let active = self.active.try_lock();
        match active {
            Ok(guard) => *guard,
            Err(_) => false,
        }
    }

    /// Start the RPC client
    ///
    /// Connects to the configured addresses and starts listening for published messages.
    /// This is a non-blocking operation that spawns background tasks.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if already active
        {
            let mut active = self.active.lock().await;
            if *active {
                info!("RPC Client already active");
                return Ok(());
            }
            *active = true;
        }

        // Create and configure REQ socket
        let socket_req = self.context.socket(zmq::REQ)?;
        socket_req.set_tcp_keepalive(1)?;
        socket_req.set_tcp_keepalive_idle(TCP_KEEPALIVE_IDLE)?;
        socket_req.set_linger(0)?;
        socket_req.connect(&self.config.req_address)?;
        info!("RPC REQ socket connected to: {}", self.config.req_address);

        // Create and configure SUB socket
        let socket_sub = self.context.socket(zmq::SUB)?;
        socket_sub.set_tcp_keepalive(1)?;
        socket_sub.set_tcp_keepalive_idle(TCP_KEEPALIVE_IDLE)?;
        socket_sub.set_linger(0)?;
        socket_sub.connect(&self.config.sub_address)?;
        
        // Subscribe to heartbeat by default
        socket_sub.set_subscribe(HEARTBEAT_TOPIC.as_bytes())?;
        
        // Subscribe to any previously registered topics
        {
            let topics = self.subscribed_topics.lock().await;
            for topic in topics.iter() {
                socket_sub.set_subscribe(topic.as_bytes())?;
            }
        }
        
        info!("RPC SUB socket connected to: {}", self.config.sub_address);

        // Store sockets
        {
            let mut req_guard = self.socket_req.lock().await;
            *req_guard = Some(socket_req);
        }
        {
            let mut sub_guard = self.socket_sub.lock().await;
            *sub_guard = Some(socket_sub);
        }

        // Initialize heartbeat timestamp
        {
            let mut last_ping = self.last_received_ping.lock().await;
            *last_ping = Instant::now();
        }

        // Spawn message listener task
        self.spawn_message_listener().await;

        info!("RPC Client started successfully");
        Ok(())
    }

    /// Stop the RPC client
    ///
    /// Gracefully shuts down the client and disconnects all sockets.
    pub async fn stop(&self) {
        info!("Stopping RPC Client");

        // Set active to false
        {
            let mut active = self.active.lock().await;
            *active = false;
        }

        // Disconnect sockets
        {
            let mut req_guard = self.socket_req.lock().await;
            if let Some(socket) = req_guard.take() {
                if let Err(e) = socket.disconnect(&self.config.req_address) {
                    warn!("Error disconnecting REQ socket: {}", e);
                }
            }
        }
        {
            let mut sub_guard = self.socket_sub.lock().await;
            if let Some(socket) = sub_guard.take() {
                if let Err(e) = socket.disconnect(&self.config.sub_address) {
                    warn!("Error disconnecting SUB socket: {}", e);
                }
            }
        }

        info!("RPC Client stopped");
    }

    /// Make a remote procedure call
    ///
    /// Sends a request to the server and waits for the response.
    /// Returns the result or an error if the call fails.
    pub async fn call(
        &self,
        method: String,
        args: Vec<serde_json::Value>,
        kwargs: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.call_with_timeout(method, args, kwargs, self.config.timeout_ms)
            .await
    }

    /// Make a remote procedure call with custom timeout
    pub async fn call_with_timeout(
        &self,
        method: String,
        args: Vec<serde_json::Value>,
        kwargs: HashMap<String, serde_json::Value>,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        // Create request
        let request = RpcRequest::new(method.clone(), args, kwargs);

        // Get socket with lock to ensure thread safety
        let socket = {
            let _lock = self.req_lock.lock().await;
            let req_guard = self.socket_req.lock().await;
            req_guard.as_ref().map(|s| s as *const zmq::Socket)
        };

        let socket_ptr = socket.ok_or("REQ socket not initialized")?;
        let socket = unsafe { &*socket_ptr };

        // Serialize and send request
        let serialized = serde_json::to_vec(&request)?;
        socket.send(&serialized, 0)?;
        debug!("Sent RPC request: {}", method);

        // Wait for response with timeout
        let poll_timeout = timeout_ms as i64;
        match socket.poll(zmq::PollEvents::POLLIN, poll_timeout) {
            Ok(n) if n > 0 => {
                // Receive response
                let data = socket.recv_bytes(0)?;
                
                // Deserialize response
                let response: RpcResponse = serde_json::from_slice(&data)?;
                
                if response.success {
                    debug!("RPC call successful: {}", method);
                    Ok(response.data)
                } else {
                    let error_msg = response.data.as_str().unwrap_or("Unknown error").to_string();
                    error!("RPC call failed: {} - {}", method, error_msg);
                    Err(Box::new(RemoteException::new(error_msg)))
                }
            }
            Ok(_) => {
                // Timeout
                error!("RPC call timeout: {}", method);
                Err(Box::new(TimeoutError::new(timeout_ms, request)))
            }
            Err(e) => {
                error!("RPC call error: {} - {}", method, e);
                Err(Box::new(ConnectionError::SocketError(e.to_string())))
            }
        }
    }

    /// Subscribe to a topic
    ///
    /// Messages published to this topic will be received and passed to the callback.
    pub async fn subscribe_topic(&self, topic: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let socket = {
            let sub_guard = self.socket_sub.lock().await;
            sub_guard.as_ref().map(|s| s as *const zmq::Socket)
        };

        if let Some(socket_ptr) = socket {
            let socket = unsafe { &*socket_ptr };
            socket.set_subscribe(topic.as_bytes())?;
            
            // Track subscribed topic
            let mut topics = self.subscribed_topics.lock().await;
            if !topics.contains(&topic) {
                topics.push(topic.clone());
            }
            
            info!("Subscribed to topic: {}", topic);
        } else {
            warn!("Attempted to subscribe but SUB socket not initialized");
            // Still track the topic for when socket connects
            let mut topics = self.subscribed_topics.lock().await;
            if !topics.contains(&topic) {
                topics.push(topic.clone());
            }
        }

        Ok(())
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe_topic(&self, topic: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let socket = {
            let sub_guard = self.socket_sub.lock().await;
            sub_guard.as_ref().map(|s| s as *const zmq::Socket)
        };

        if let Some(socket_ptr) = socket {
            let socket = unsafe { &*socket_ptr };
            socket.set_unsubscribe(topic.as_bytes())?;
            
            // Remove from tracked topics
            let mut topics = self.subscribed_topics.lock().await;
            topics.retain(|t| t != &topic);
            
            info!("Unsubscribed from topic: {}", topic);
        }

        Ok(())
    }

    /// Set the callback for handling received messages
    ///
    /// The callback will be invoked for each non-heartbeat message received.
    pub async fn set_callback<F>(&self, callback: F)
    where
        F: Fn(String, serde_json::Value) + Send + Sync + 'static,
    {
        let mut cb_guard = self.callback.lock().await;
        *cb_guard = Some(Arc::new(callback));
    }

    /// Get the last received heartbeat timestamp
    pub async fn last_received_ping(&self) -> Instant {
        *self.last_received_ping.lock().await
    }

    /// Check if the connection is alive based on heartbeat
    pub async fn is_connected(&self) -> bool {
        let last_ping = *self.last_received_ping.lock().await;
        Instant::now().duration_since(last_ping) < self.config.heartbeat_tolerance
    }

    /// Spawn the message listener task
    async fn spawn_message_listener(&self) {
        let active = self.active.clone();
        let socket_sub = self.socket_sub.clone();
        let callback = self.callback.clone();
        let last_received_ping = self.last_received_ping.clone();
        let heartbeat_tolerance = self.config.heartbeat_tolerance;

        tokio::spawn(async move {
            let mut disconnected_warned = false;

            loop {
                // Check if still active
                {
                    let active_guard = active.lock().await;
                    if !*active_guard {
                        break;
                    }
                }

                // Get socket reference
                let socket = {
                    let sub_guard = socket_sub.lock().await;
                    sub_guard.as_ref().map(|s| s as *const zmq::Socket)
                };

                if let Some(socket_ptr) = socket {
                    let socket = unsafe { &*socket_ptr };
                    
                    // Poll for incoming messages with timeout
                    let poll_timeout = (heartbeat_tolerance.as_millis() as i64).min(POLL_TIMEOUT_MS as i64);
                    
                    match socket.poll(zmq::PollEvents::POLLIN, poll_timeout) {
                        Ok(n) if n > 0 => {
                            // Receive message
                            match socket.recv_bytes(0) {
                                Ok(data) => {
                                    // Deserialize message
                                    match serde_json::from_slice::<RpcMessage>(&data) {
                                        Ok(message) => {
                                            // Handle heartbeat
                                            if message.topic == HEARTBEAT_TOPIC {
                                                *last_received_ping.lock().await = Instant::now();
                                                disconnected_warned = false;
                                                debug!("Received heartbeat");
                                            } else {
                                                // Handle regular message
                                                let cb_guard = callback.lock().await;
                                                if let Some(ref cb) = *cb_guard {
                                                    cb(message.topic, message.data);
                                                } else {
                                                    warn!("Received message but no callback set: {}", message.topic);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to deserialize message: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to receive message: {}", e);
                                }
                            }
                        }
                        Ok(_) => {
                            // No data available, check heartbeat
                            let last_ping = *last_received_ping.lock().await;
                            if Instant::now().duration_since(last_ping) > heartbeat_tolerance {
                                if !disconnected_warned {
                                    warn!(
                                        "RPC Server has no response over {} seconds, connection may be lost",
                                        heartbeat_tolerance.as_secs()
                                    );
                                    disconnected_warned = true;
                                }
                            }
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

    }

impl Default for RpcClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Remote method proxy for convenient RPC calls
///
/// Provides a callable interface that transparently makes RPC calls.
#[derive(Clone)]
pub struct RemoteMethod {
    client: Arc<RpcClient>,
    method: String,
}

impl RemoteMethod {
    /// Create a new remote method proxy
    pub fn new(client: Arc<RpcClient>, method: String) -> Self {
        Self { client, method }
    }

    /// Call the remote method with arguments
    pub async fn call(
        &self,
        args: Vec<serde_json::Value>,
        kwargs: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.client.call(self.method.clone(), args, kwargs).await
    }
}

/// Method cache for efficient remote method proxy creation
pub struct MethodCache {
    client: Arc<RpcClient>,
    cache: Arc<Mutex<LruCache<String, RemoteMethod>>>,
}

impl MethodCache {
    /// Create a new method cache
    pub fn new(client: Arc<RpcClient>, capacity: usize) -> Self {
        Self {
            client,
            cache: Arc::new(Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(capacity).unwrap(),
            ))),
        }
    }

    /// Get or create a remote method proxy
    pub async fn get(&self, method: String) -> RemoteMethod {
        let mut cache = self.cache.lock().await;
        
        if let Some(_method_proxy) = cache.get(&method) {
            // Clone the method proxy (note: this creates a new proxy with the same client/method)
            RemoteMethod::new(self.client.clone(), method.clone())
        } else {
            let method_proxy = RemoteMethod::new(self.client.clone(), method.clone());
            cache.put(method.clone(), method_proxy.clone());
            method_proxy
        }
    }
}