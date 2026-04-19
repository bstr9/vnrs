//! Binance WebSocket client.

use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use std::task::{Context, Poll};

use base64::{engine::general_purpose, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio_socks::tcp::Socks5Stream;
use tokio_tungstenite::{
    client_async_tls_with_config, connect_async, tungstenite::Message, WebSocketStream,
};
use tracing::{debug, error, info, warn};
use url::Url;

/// WebSocket message handler type
pub type WsMessageHandler = Arc<dyn Fn(Value) + Send + Sync>;

/// Proxy stream enum supporting multiple proxy protocols
enum ProxyStream {
    Http(TcpStream),
    Socks(Socks5Stream<TcpStream>),
}

impl AsyncRead for ProxyStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ProxyStream::Http(s) => Pin::new(s).poll_read(cx, buf),
            ProxyStream::Socks(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ProxyStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.get_mut() {
            ProxyStream::Http(s) => Pin::new(s).poll_write(cx, buf),
            ProxyStream::Socks(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            ProxyStream::Http(s) => Pin::new(s).poll_flush(cx),
            ProxyStream::Socks(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            ProxyStream::Http(s) => Pin::new(s).poll_shutdown(cx),
            ProxyStream::Socks(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Inner proxy implementation supporting HTTP/HTTPS and SOCKS5
enum InnerProxy {
    Http {
        auth: Option<Vec<u8>>,
        url: String,
    },
    Socks {
        auth: Option<(String, String)>,
        url: String,
    },
}

impl InnerProxy {
    /// Parse proxy from string format (e.g., "http://user:pass@host:port" or "socks5://host:port")
    fn from_proxy_str(proxy_str: &str) -> Result<InnerProxy, Error> {
        use url::Position;

        let url = Url::parse(proxy_str).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Failed to parse proxy url: {}", e),
            )
        })?;

        let addr = &url[Position::BeforeHost..Position::AfterPort];

        match url.scheme() {
            "http" | "https" => {
                let mut basic_bytes: Option<Vec<u8>> = None;
                if let Some(pwd) = url.password() {
                    let credentials = format!("{}:{}", url.username(), pwd);
                    let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
                    let encoded_str = format!("Basic {}", encoded);
                    basic_bytes = Some(encoded_str.into_bytes());
                }

                Ok(InnerProxy::Http {
                    auth: basic_bytes,
                    url: addr.to_string(),
                })
            }
            "socks5" => {
                let mut auth_pair = None;
                if let Some(pwd) = url.password() {
                    auth_pair = Some((url.username().to_string(), pwd.to_string()));
                }

                Ok(InnerProxy::Socks {
                    auth: auth_pair,
                    url: addr.to_string(),
                })
            }
            _ => Err(Error::new(
                ErrorKind::Unsupported,
                format!("Unsupported proxy scheme: {}", url.scheme()),
            )),
        }
    }

    /// Connect to target through proxy
    async fn connect_async(&self, target: &str) -> Result<ProxyStream, Error> {
        let target_url = Url::parse(target).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Failed to parse target url: {}", e),
            )
        })?;

        let host = target_url
            .host_str()
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Target host not available"))?
            .to_string();

        let port = target_url.port().unwrap_or(443);

        match self {
            InnerProxy::Http { auth, url } => {
                let tcp_stream = TcpStream::connect(url).await.map_err(|e| {
                    Error::new(
                        ErrorKind::ConnectionRefused,
                        format!("Failed to connect to HTTP proxy: {}", e),
                    )
                })?;
                Ok(ProxyStream::Http(
                    Self::tunnel(tcp_stream, host, port, auth).await?,
                ))
            }
            InnerProxy::Socks { auth, url } => {
                let stream = match auth {
                    Some(au) => {
                        Socks5Stream::connect_with_password(
                            url.as_str(),
                            (host.as_str(), port),
                            &au.0,
                            &au.1,
                        )
                        .await
                    }
                    None => Socks5Stream::connect(url.as_str(), (host.as_str(), port)).await,
                };

                stream
                    .map(ProxyStream::Socks)
                    .map_err(|e| Error::new(ErrorKind::ConnectionRefused, format!("Failed to connect to SOCKS5 proxy: {}", e)))
            }
        }
    }

    /// Create HTTP CONNECT tunnel
    async fn tunnel(
        mut conn: TcpStream,
        host: String,
        port: u16,
        auth: &Option<Vec<u8>>,
    ) -> Result<TcpStream, Error> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut buf = format!(
            "CONNECT {0}:{1} HTTP/1.1\r\nHost: {0}:{1}\r\n",
            host, port
        )
        .into_bytes();

        if let Some(au) = auth {
            buf.extend_from_slice(b"Proxy-Authorization: ");
            buf.extend_from_slice(au.as_slice());
            buf.extend_from_slice(b"\r\n");
        }

        buf.extend_from_slice(b"\r\n");
        conn.write_all(&buf).await?;

        let mut buf = [0; 1024];
        let mut pos = 0;

        loop {
            let n = conn.read(&mut buf[pos..]).await?;
            if n == 0 {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "Connection closed while reading tunnel response",
                ));
            }
            pos += n;

            let recvd = &buf[..pos];
            if recvd.starts_with(b"HTTP/1.1 200") || recvd.starts_with(b"HTTP/1.0 200") {
                if recvd.ends_with(b"\r\n\r\n") {
                    return Ok(conn);
                }
                if pos == buf.len() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "Proxy headers too long",
                    ));
                }
            } else if recvd.starts_with(b"HTTP/1.1 407") {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    "Proxy authentication required",
                ));
            } else if recvd.len() >= 12 {
                return Err(Error::other(
                    format!(
                        "Unsuccessful tunnel: {}",
                        String::from_utf8_lossy(&recvd[..12])
                    ),
                ));
            }
        }
    }
}

/// WebSocket client for Binance
pub struct BinanceWebSocketClient {
    /// WebSocket URL
    url: Arc<RwLock<String>>,
    /// Message handler
    handler: Arc<RwLock<Option<WsMessageHandler>>>,
    /// Message sender for sending to WebSocket
    tx: Arc<RwLock<Option<mpsc::Sender<Message>>>>,
    /// Active flag
    active: Arc<RwLock<bool>>,
    /// Request ID counter
    req_id: Arc<RwLock<i64>>,
    /// Gateway name for logging
    gateway_name: String,
    /// Last pong received timestamp (for connection health monitoring)
    last_pong: Arc<RwLock<Option<std::time::Instant>>>,
    /// Connection URL for potential reconnect
    connection_url: Arc<RwLock<Option<String>>>,
    /// Proxy settings for potential reconnect
    proxy_settings: Arc<RwLock<(String, u16)>>,
    /// Tracked subscriptions for re-subscription after reconnect
    subscriptions: Arc<RwLock<Vec<String>>>,
    /// Callback invoked when connection is lost unexpectedly (not via disconnect())
    on_disconnect: Arc<RwLock<Option<Arc<dyn Fn() + Send + Sync>>>>,
    /// Flag to distinguish intentional disconnect from unexpected connection loss
    graceful_shutdown: Arc<AtomicBool>,
    /// Reconnection attempt counter for exponential backoff
    reconnect_attempts: Arc<AtomicU32>,
}

impl BinanceWebSocketClient {
    /// Create a new WebSocket client
    pub fn new(gateway_name: &str) -> Self {
        Self {
            url: Arc::new(RwLock::new(String::new())),
            handler: Arc::new(RwLock::new(None)),
            tx: Arc::new(RwLock::new(None)),
            active: Arc::new(RwLock::new(false)),
            req_id: Arc::new(RwLock::new(0)),
            gateway_name: gateway_name.to_string(),
            last_pong: Arc::new(RwLock::new(None)),
            connection_url: Arc::new(RwLock::new(None)),
            proxy_settings: Arc::new(RwLock::new((String::new(), 0u16))),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            on_disconnect: Arc::new(RwLock::new(None)),
            graceful_shutdown: Arc::new(AtomicBool::new(false)),
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Set the message handler
    pub async fn set_handler(&self, handler: WsMessageHandler) {
        *self.handler.write().await = Some(handler);
    }

    /// Connect to WebSocket
    pub async fn connect(
        &self,
        url: &str,
        proxy_host: &str,
        proxy_port: u16,
    ) -> Result<(), String> {
        *self.url.write().await = url.to_string();

        // Save connection parameters for potential reconnection
        *self.connection_url.write().await = Some(url.to_string());
        *self.proxy_settings.write().await = (proxy_host.to_string(), proxy_port);

        info!("{}: Connecting to WebSocket: {}", self.gateway_name, url);

        // Determine if we need to use proxy
        let use_proxy = !proxy_host.is_empty() && proxy_port > 0;

        if use_proxy {
            // Construct proxy URL
            let proxy_url = if proxy_host.starts_with("http://")
                || proxy_host.starts_with("https://")
                || proxy_host.starts_with("socks5://")
            {
                // If proxy_host already contains scheme, use it directly
                if proxy_host.contains(':') {
                    // Host already has a port or full URL with scheme
                    proxy_host.to_string()
                } else {
                    format!("{}:{}", proxy_host, proxy_port)
                }
            } else {
                // Default to socks5 if no scheme specified
                format!("socks5://{}:{}", proxy_host, proxy_port)
            };

            info!(
                "{}: Using proxy: {}",
                self.gateway_name, proxy_url
            );

            // Create proxy connection
            let proxy = InnerProxy::from_proxy_str(&proxy_url)
                .map_err(|e| format!("Failed to parse proxy: {}", e))?;

            let proxy_stream = proxy
                .connect_async(url)
                .await
                .map_err(|e| format!("Failed to connect through proxy: {}", e))?;

            // Connect WebSocket through proxy
            let (ws_stream, _) = client_async_tls_with_config(
                url,
                proxy_stream,
                None,
                None, // Use default TLS connector
            )
            .await
            .map_err(|e| format!("WebSocket connection through proxy failed: {}", e))?;

            info!("{}: WebSocket connected", self.gateway_name);

            self.handle_websocket_stream(ws_stream).await;
        } else {
            // Direct connection without proxy
            let (ws_stream, _) = connect_async(url)
                .await
                .map_err(|e| format!("WebSocket connection failed: {}", e))?;

            info!("{}: WebSocket connected", self.gateway_name);

            self.handle_websocket_stream(ws_stream).await;
        }

        Ok(())
    }

    /// Handle the WebSocket stream (common logic for both proxy and direct connections)
    async fn handle_websocket_stream<S>(&self, ws_stream: WebSocketStream<S>)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Reset graceful shutdown flag for new connection
        self.graceful_shutdown.store(false, std::sync::atomic::Ordering::SeqCst);

        let (write, read) = ws_stream.split();
        let (tx, rx) = mpsc::channel::<Message>(1024);

        *self.tx.write().await = Some(tx);
        *self.active.write().await = true;
        *self.last_pong.write().await = Some(std::time::Instant::now());

        // Reset reconnect attempts counter — connection succeeded
        self.reconnect_attempts.store(0, std::sync::atomic::Ordering::SeqCst);

        // Shared flag to ensure on_disconnect is called at most once per connection lifecycle
        let disconnect_notified = Arc::new(AtomicBool::new(false));

        // Spawn write task (with periodic ping for keepalive)
        let write_active = self.active.clone();
        let write_tx = self.tx.clone();
        let last_pong_clone = self.last_pong.clone();
        let gateway_name = self.gateway_name.clone();
        let graceful_shutdown_write = self.graceful_shutdown.clone();
        let disconnect_notified_write = disconnect_notified.clone();
        let on_disconnect_write = self.on_disconnect.clone();
        tokio::spawn(async move {
            let mut write = write;
            let mut rx = rx;
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
            ping_interval.tick().await; // First tick is immediate

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(msg) => {
                                if let Err(e) = write.send(msg).await {
                                    error!("WebSocket write error: {}", e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = ping_interval.tick() => {
                        // Check if connection is still alive (pong received within last 60s)
                        let pong_ok = {
                            let last = last_pong_clone.read().await;
                            last.map(|t| t.elapsed() < std::time::Duration::from_secs(60))
                                .unwrap_or(false)
                        };
                        if !pong_ok {
                            warn!("{}: No pong received in 60s, connection is dead", gateway_name);
                            break; // Exit write loop — triggers reconnect
                        }
                        // Send ping to keep connection alive
                        if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                            error!("{}: Failed to send ping: {}", gateway_name, e);
                            break;
                        }
                    }
                }
            }
            *write_active.write().await = false;
            // Clear tx so subsequent send() calls fail immediately
            *write_tx.write().await = None;

            // Notify disconnect if not graceful
            let was_notified = disconnect_notified_write.swap(true, std::sync::atomic::Ordering::SeqCst);
            if !was_notified && !graceful_shutdown_write.load(std::sync::atomic::Ordering::SeqCst) {
                warn!("{}: Write loop ended unexpectedly, invoking on_disconnect", gateway_name);
                if let Some(cb) = on_disconnect_write.read().await.as_ref() {
                    cb();
                }
            }
        });

        // Spawn read task
        let handler = self.handler.clone();
        let read_active = self.active.clone();
        let gateway_name = self.gateway_name.clone();
        let last_pong_read = self.last_pong.clone();
        let graceful_shutdown_read = self.graceful_shutdown.clone();
        let disconnect_notified_read = disconnect_notified.clone();
        let on_disconnect_read = self.on_disconnect.clone();
        tokio::spawn(async move {
            let mut read = read;
            while let Some(result) = read.next().await {
                match result {
                    Ok(Message::Text(text)) => {
                        let value: Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("Failed to parse WebSocket message: {}. Text preview: {:.100}", e, &text[..text.len().min(100)]);
                                continue;
                            }
                        };
                        if let Some(h) = handler.read().await.as_ref() {
                            h(value);
                        }
                    }
                    Ok(Message::Ping(_)) => {
                        debug!("{}: Received ping", gateway_name);
                        // Pong is handled automatically by tungstenite
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("{}: Received pong", gateway_name);
                        *last_pong_read.write().await = Some(std::time::Instant::now());
                    }
                    Ok(Message::Close(_)) => {
                        warn!("{}: WebSocket closed by server", gateway_name);
                        break;
                    }
                    Ok(Message::Binary(_)) => {
                        debug!("{}: Received binary message", gateway_name);
                    }
                    Err(e) => {
                        error!("{}: WebSocket read error: {}", gateway_name, e);
                        break;
                    }
                    _ => {}
                }
            }
            *read_active.write().await = false;
            warn!("{}: WebSocket read loop ended", gateway_name);

            // Notify disconnect if not graceful
            let was_notified = disconnect_notified_read.swap(true, std::sync::atomic::Ordering::SeqCst);
            if !was_notified && !graceful_shutdown_read.load(std::sync::atomic::Ordering::SeqCst) {
                warn!("{}: Read loop ended unexpectedly, invoking on_disconnect", gateway_name);
                if let Some(cb) = on_disconnect_read.read().await.as_ref() {
                    cb();
                }
            }
        });

        // Spawn health monitor task
        let health_active = self.active.clone();
        let health_last_pong = self.last_pong.clone();
        let health_gateway_name = self.gateway_name.clone();
        let health_graceful = self.graceful_shutdown.clone();
        let health_disconnect_notified = disconnect_notified;
        let health_on_disconnect = self.on_disconnect.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                if !*health_active.read().await {
                    break;
                }
                // Check if connection is dead (no pong in 60s)
                let dead = {
                    let last = health_last_pong.read().await;
                    last.map(|t| t.elapsed() > std::time::Duration::from_secs(60))
                        .unwrap_or(false)
                };
                if dead {
                    let already_handled = health_disconnect_notified.load(std::sync::atomic::Ordering::SeqCst);
                    if !already_handled && !health_graceful.load(std::sync::atomic::Ordering::SeqCst) {
                        warn!("{}: Health monitor detected dead connection, triggering disconnect notification", health_gateway_name);
                        *health_active.write().await = false;
                        // The disconnect_notified flag + callback invocation will be handled
                        // by whichever task gets here first
                        let was_notified = health_disconnect_notified.swap(true, std::sync::atomic::Ordering::SeqCst);
                        if !was_notified {
                            if let Some(cb) = health_on_disconnect.read().await.as_ref() {
                                cb();
                            }
                        }
                    }
                    break; // Health monitor can stop — reconnect is in progress
                }
            }
        });
    }

    /// Disconnect from WebSocket
    pub async fn disconnect(&self) {
        self.graceful_shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
        *self.active.write().await = false;
        if let Some(tx) = self.tx.write().await.take() {
            let _ = tx.send(Message::Close(None)).await;
        }
        info!("{}: WebSocket disconnected", self.gateway_name);
    }

    /// Check if connected
    #[allow(dead_code)]
    pub async fn is_connected(&self) -> bool {
        *self.active.read().await
    }

    /// Get time since last pong (for connection health monitoring)
    pub async fn time_since_last_pong(&self) -> Option<std::time::Duration> {
        self.last_pong.read().await.map(|t| t.elapsed())
    }

    /// Calculate exponential backoff delay with jitter for reconnect attempts.
    /// Base: 1s, doubles each attempt, capped at 60s, ±25% jitter.
    pub fn calculate_backoff_delay(attempt: u32) -> std::time::Duration {
        let base_secs: u64 = 1u64.checked_shl(attempt).unwrap_or(60).min(60);
        // Simple jitter using SystemTime as entropy source (no external rand crate)
        let jitter_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        // Map to [-0.25, +0.25] range
        let jitter_pct = (jitter_nanos as f64 / u32::MAX as f64) * 0.5 - 0.25;
        let final_secs = ((base_secs as f64) * (1.0 + jitter_pct)).max(1.0) as u64;
        std::time::Duration::from_secs(final_secs)
    }

    /// Attempt to reconnect using saved connection parameters
    /// Returns Ok if reconnection succeeded, Err with reason if it failed
    pub async fn reconnect(&self) -> Result<(), String> {
        // Disconnect first
        self.disconnect().await;

        // Exponential backoff with jitter: increment counter and calculate delay
        let attempt = self.reconnect_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let delay = Self::calculate_backoff_delay(attempt);
        info!(
            "{}: Reconnect attempt {}, waiting {:?} before connecting",
            self.gateway_name, attempt + 1, delay
        );
        tokio::time::sleep(delay).await;

        // Retrieve saved connection parameters
        let (url, proxy_host, proxy_port) = {
            let url_opt = self.connection_url.read().await.clone();
            let proxy = self.proxy_settings.read().await.clone();
            match url_opt {
                Some(u) => (u, proxy.0, proxy.1),
                None => return Err("No saved connection URL for reconnect".to_string()),
            }
        };

        info!("{}: Attempting reconnect to {}", self.gateway_name, url);
        self.connect(&url, &proxy_host, proxy_port).await
    }

    /// Send a message
    pub async fn send(&self, message: Value) -> Result<(), String> {
        let text = serde_json::to_string(&message)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;

        if let Some(tx) = self.tx.read().await.as_ref() {
            tx.send(Message::Text(text.into())).await
                .map_err(|e| format!("Failed to send message: {}", e))?;
        } else {
            return Err("WebSocket not connected".to_string());
        }

        Ok(())
    }

    /// Subscribe to channels.
    ///
    /// If the WebSocket is temporarily disconnected (e.g. during reconnect),
    /// this method will retry for up to 5 seconds before giving up.
    /// The subscription is always tracked so `resubscribe()` can restore it
    /// after a successful reconnect even if this call times out.
    pub async fn subscribe(&self, channels: Vec<String>) -> Result<(), String> {
        // Track subscriptions for re-subscription after reconnect
        {
            let mut subs = self.subscriptions.write().await;
            for ch in &channels {
                if !subs.contains(ch) {
                    subs.push(ch.clone());
                }
            }
        }

        let mut req_id = self.req_id.write().await;
        *req_id += 1;

        let message = json!({
            "method": "SUBSCRIBE",
            "params": channels,
            "id": *req_id
        });

        // Retry send when WebSocket is temporarily disconnected (reconnect in progress).
        // The subscription is already tracked above, so resubscribe() will also restore it,
        // but we try to send immediately to avoid missing data during the gap.
        let max_retries = 50u32; // 50 × 100ms = 5 seconds
        for attempt in 0..=max_retries {
            match self.send(message.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) if e == "WebSocket not connected" && attempt < max_retries => {
                    if attempt == 0 {
                        info!(
                            "{}: WebSocket not connected when subscribing, waiting for reconnect...",
                            self.gateway_name
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err("WebSocket not connected after 5s retry".to_string())
    }

    /// Unsubscribe from channels
    #[allow(dead_code)]
    pub async fn unsubscribe(&self, channels: Vec<String>) -> Result<(), String> {
        // Remove from tracked subscriptions
        {
            let mut subs = self.subscriptions.write().await;
            subs.retain(|ch| !channels.contains(ch));
        }

        let mut req_id = self.req_id.write().await;
        *req_id += 1;

        let message = json!({
            "method": "UNSUBSCRIBE",
            "params": channels,
            "id": *req_id
        });

        self.send(message).await
    }

    /// Set the on_disconnect callback, invoked when connection is lost unexpectedly
    pub async fn set_on_disconnect(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.on_disconnect.write().await = Some(callback);
    }

    /// Re-send all tracked subscriptions after reconnection
    pub async fn resubscribe(&self) -> Result<(), String> {
        let channels = self.subscriptions.read().await.clone();
        if channels.is_empty() {
            return Ok(());
        }

        let mut req_id = self.req_id.write().await;
        *req_id += 1;

        let message = json!({
            "method": "SUBSCRIBE",
            "params": channels,
            "id": *req_id
        });

        self.send(message).await
    }
}

impl Default for BinanceWebSocketClient {
    fn default() -> Self {
        Self::new("BINANCE")
    }
}

/// Connection manager that provides automatic reconnection with exponential backoff.
///
/// Wraps a `BinanceWebSocketClient` and sets up the `on_disconnect` callback
/// to trigger automatic reconnect and re-subscription when the connection drops.
///
/// # Usage
/// ```ignore
/// let ws = Arc::new(BinanceWebSocketClient::new("BINANCE_SPOT"));
/// let manager = ConnectionManager::new(ws.clone(), "BINANCE_SPOT");
/// manager.enable_auto_reconnect().await;
/// // Now if the WebSocket disconnects unexpectedly, it will auto-reconnect
/// ```
pub struct ConnectionManager {
    /// The WebSocket client being managed
    ws: Arc<BinanceWebSocketClient>,
    /// Gateway name for logging
    gateway_name: String,
    /// Whether auto-reconnect is enabled
    auto_reconnect_enabled: Arc<AtomicBool>,
    /// Maximum number of reconnect attempts before giving up (0 = unlimited)
    max_reconnect_attempts: u32,
    /// Custom reconnect callback (called after successful reconnect, before re-subscription)
    on_reconnected: Arc<RwLock<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl ConnectionManager {
    /// Create a new connection manager for the given WebSocket client
    pub fn new(ws: Arc<BinanceWebSocketClient>, gateway_name: &str) -> Self {
        Self {
            ws,
            gateway_name: gateway_name.to_string(),
            auto_reconnect_enabled: Arc::new(AtomicBool::new(false)),
            max_reconnect_attempts: 0, // unlimited by default
            on_reconnected: Arc::new(RwLock::new(None)),
        }
    }

    /// Set maximum reconnect attempts (0 = unlimited)
    pub fn with_max_reconnect_attempts(mut self, max: u32) -> Self {
        self.max_reconnect_attempts = max;
        self
    }

    /// Set a custom callback invoked after successful reconnection (before re-subscription).
    /// This is useful for gateways that need to re-establish state (e.g., user data stream listen key).
    pub async fn set_on_reconnected(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.on_reconnected.write().await = Some(callback);
    }

    /// Enable automatic reconnection.
    ///
    /// Sets the `on_disconnect` callback on the WebSocket client. When the connection
    /// drops unexpectedly (not via `disconnect()`), the manager will:
    /// 1. Wait with exponential backoff
    /// 2. Attempt to reconnect
    /// 3. Re-subscribe to all tracked channels
    /// 4. Invoke the `on_reconnected` callback if set
    pub async fn enable_auto_reconnect(&self) {
        self.auto_reconnect_enabled.store(true, std::sync::atomic::Ordering::SeqCst);

        let ws = self.ws.clone();
        let gateway_name = self.gateway_name.clone();
        let max_attempts = self.max_reconnect_attempts;
        let on_reconnected = self.on_reconnected.clone();
        let auto_enabled = self.auto_reconnect_enabled.clone();

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let ws = ws.clone();
            let gateway_name = gateway_name.clone();
            let on_reconnected = on_reconnected.clone();
            let auto_enabled = auto_enabled.clone();

            tokio::spawn(async move {
                if !auto_enabled.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("{}: Auto-reconnect disabled, skipping", gateway_name);
                    return;
                }

                warn!("{}: 连接断开，开始自动重连...", gateway_name);

                let mut attempt = 0u32;
                loop {
                    if !auto_enabled.load(std::sync::atomic::Ordering::SeqCst) {
                        info!("{}: Auto-reconnect disabled during retry, aborting", gateway_name);
                        return;
                    }

                    // Check max attempts
                    if max_attempts > 0 && attempt >= max_attempts {
                        error!(
                            "{}: 达到最大重连次数 {}，放弃重连",
                            gateway_name, max_attempts
                        );
                        return;
                    }

                    attempt += 1;
                    let delay = BinanceWebSocketClient::calculate_backoff_delay(attempt - 1);
                    info!(
                        "{}: 重连尝试 {}/{}, 等待 {:?}",
                        gateway_name,
                        attempt,
                        if max_attempts > 0 { max_attempts.to_string() } else { "∞".to_string() },
                        delay
                    );
                    tokio::time::sleep(delay).await;

                    match ws.reconnect().await {
                        Ok(()) => {
                            info!("{}: 重连成功", gateway_name);

                            // Invoke on_reconnected callback (e.g., re-establish user data stream)
                            if let Some(cb) = on_reconnected.read().await.as_ref() {
                                cb();
                            }

                            // Re-subscribe to tracked channels
                            match ws.resubscribe().await {
                                Ok(()) => {
                                    info!("{}: 重新订阅成功", gateway_name);
                                }
                                Err(e) => {
                                    warn!("{}: 重新订阅失败: {}", gateway_name, e);
                                }
                            }

                            return; // Successfully reconnected
                        }
                        Err(e) => {
                            warn!("{}: 重连失败 (尝试 {}): {}", gateway_name, attempt, e);
                            // Continue loop to try again
                        }
                    }
                }
            });
        });

        self.ws.set_on_disconnect(callback).await;
        info!("{}: 自动重连已启用", self.gateway_name);
    }

    /// Disable automatic reconnection
    pub fn disable_auto_reconnect(&self) {
        self.auto_reconnect_enabled.store(false, std::sync::atomic::Ordering::SeqCst);
        info!("{}: 自动重连已禁用", self.gateway_name);
    }

    /// Check if auto-reconnect is enabled
    pub fn is_auto_reconnect_enabled(&self) -> bool {
        self.auto_reconnect_enabled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff_delay_first_attempt() {
        let delay = BinanceWebSocketClient::calculate_backoff_delay(0);
        // First attempt: base = 1s, with jitter ±25%, so [0.75s, 1.25s]
        assert!(delay.as_secs() >= 1);
        assert!(delay.as_secs() <= 2);
    }

    #[test]
    fn test_calculate_backoff_delay_capped() {
        // Very high attempt number should cap at 60s
        let delay = BinanceWebSocketClient::calculate_backoff_delay(100);
        assert!(delay.as_secs() <= 75); // 60s + 25% jitter
    }

    #[test]
    fn test_calculate_backoff_delay_increasing() {
        let d1 = BinanceWebSocketClient::calculate_backoff_delay(1);
        let d2 = BinanceWebSocketClient::calculate_backoff_delay(3);
        // d2 should generally be longer than d1 (ignoring jitter edge cases)
        // Just check d2 is at least 2 seconds (2^3=8s base, minus max jitter)
        assert!(d2.as_secs() >= 2);
    }

    #[tokio::test]
    async fn test_connection_manager_new() {
        let ws = Arc::new(BinanceWebSocketClient::new("TEST_GW"));
        let manager = ConnectionManager::new(ws, "TEST_GW");
        assert!(!manager.is_auto_reconnect_enabled());
    }

    #[tokio::test]
    async fn test_connection_manager_enable_disable() {
        let ws = Arc::new(BinanceWebSocketClient::new("TEST_GW"));
        let manager = ConnectionManager::new(ws, "TEST_GW");

        assert!(!manager.is_auto_reconnect_enabled());
        manager.enable_auto_reconnect().await;
        assert!(manager.is_auto_reconnect_enabled());

        manager.disable_auto_reconnect();
        assert!(!manager.is_auto_reconnect_enabled());
    }

    #[tokio::test]
    async fn test_connection_manager_with_max_attempts() {
        let ws = Arc::new(BinanceWebSocketClient::new("TEST_GW"));
        let manager = ConnectionManager::new(ws, "TEST_GW").with_max_reconnect_attempts(5);
        assert!(!manager.is_auto_reconnect_enabled());
        // Just verify construction works - max_attempts is used internally
        manager.enable_auto_reconnect().await;
        assert!(manager.is_auto_reconnect_enabled());
    }

    #[tokio::test]
    async fn test_connection_manager_on_reconnected_callback() {
        let ws = Arc::new(BinanceWebSocketClient::new("TEST_GW"));
        let manager = ConnectionManager::new(ws, "TEST_GW");

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        manager.set_on_reconnected(Arc::new(move || {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        })).await;

        // Verify callback was set (the callback itself is invoked by the reconnect loop,
        // which we can't easily test without a real server)
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
