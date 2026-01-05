//! Binance WebSocket client.

use std::io::{Error, ErrorKind};
use std::pin::Pin;
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
                return Err(Error::new(
                    ErrorKind::Other,
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
    tx: Arc<RwLock<Option<mpsc::UnboundedSender<Message>>>>,
    /// Active flag
    active: Arc<RwLock<bool>>,
    /// Request ID counter
    req_id: Arc<RwLock<i64>>,
    /// Gateway name for logging
    gateway_name: String,
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
                if proxy_host.contains(':') && !proxy_host.ends_with(&proxy_port.to_string()) {
                    format!("{}:{}", proxy_host, proxy_port)
                } else {
                    proxy_host.to_string()
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

        let (write, read) = ws_stream.split();
        let (tx, rx) = mpsc::unbounded_channel::<Message>();

        *self.tx.write().await = Some(tx);
        *self.active.write().await = true;

        // Spawn write task
        let write_active = self.active.clone();
        tokio::spawn(async move {
            let mut write = write;
            let mut rx = rx;
            while let Some(msg) = rx.recv().await {
                if let Err(e) = write.send(msg).await {
                    error!("WebSocket write error: {}", e);
                    break;
                }
            }
            *write_active.write().await = false;
        });

        // Spawn read task
        let handler = self.handler.clone();
        let read_active = self.active.clone();
        let gateway_name = self.gateway_name.clone();
        tokio::spawn(async move {
            let mut read = read;
            while let Some(result) = read.next().await {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if let Some(h) = handler.read().await.as_ref() {
                                h(value);
                            }
                        }
                    }
                    Ok(Message::Ping(_)) => {
                        debug!("{}: Received ping", gateway_name);
                        // Pong is handled automatically by tungstenite
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("{}: Received pong", gateway_name);
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
        });
    }

    /// Disconnect from WebSocket
    pub async fn disconnect(&self) {
        *self.active.write().await = false;
        if let Some(tx) = self.tx.write().await.take() {
            let _ = tx.send(Message::Close(None));
        }
        info!("{}: WebSocket disconnected", self.gateway_name);
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.active.read().await
    }

    /// Send a message
    pub async fn send(&self, message: Value) -> Result<(), String> {
        let text = serde_json::to_string(&message)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;

        if let Some(tx) = self.tx.read().await.as_ref() {
            tx.send(Message::Text(text.into()))
                .map_err(|e| format!("Failed to send message: {}", e))?;
        } else {
            return Err("WebSocket not connected".to_string());
        }

        Ok(())
    }

    /// Subscribe to channels
    pub async fn subscribe(&self, channels: Vec<String>) -> Result<(), String> {
        let mut req_id = self.req_id.write().await;
        *req_id += 1;

        let message = json!({
            "method": "SUBSCRIBE",
            "params": channels,
            "id": *req_id
        });

        self.send(message).await
    }

    /// Unsubscribe from channels
    pub async fn unsubscribe(&self, channels: Vec<String>) -> Result<(), String> {
        let mut req_id = self.req_id.write().await;
        *req_id += 1;

        let message = json!({
            "method": "UNSUBSCRIBE",
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
