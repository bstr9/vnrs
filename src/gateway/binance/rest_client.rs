//! Binance REST API client.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};
use reqwest::{Client, Method, Response};
use serde_json::Value;

use super::constants::Security;

type HmacSha256 = Hmac<Sha256>;

/// REST API client for Binance
pub struct BinanceRestClient {
    /// HTTP client
    client: Arc<RwLock<Client>>,
    /// API key
    key: Arc<RwLock<String>>,
    /// API secret
    secret: Arc<RwLock<Vec<u8>>>,
    /// Base URL
    host: Arc<RwLock<String>>,
    /// Proxy host
    proxy_host: Arc<RwLock<String>>,
    /// Proxy port
    proxy_port: Arc<RwLock<u16>>,
    /// Time offset between local and server
    time_offset: AtomicI64,
    /// Receive window in milliseconds
    recv_window: i64,
}

impl BinanceRestClient {
    /// Create a new REST client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client: Arc::new(RwLock::new(client)),
            key: Arc::new(RwLock::new(String::new())),
            secret: Arc::new(RwLock::new(Vec::new())),
            host: Arc::new(RwLock::new(String::new())),
            proxy_host: Arc::new(RwLock::new(String::new())),
            proxy_port: Arc::new(RwLock::new(0)),
            time_offset: AtomicI64::new(0),
            recv_window: 5000,
        }
    }

    /// Initialize the client with credentials and host
    pub async fn init(
        &self,
        key: &str,
        secret: &str,
        host: &str,
        proxy_host: &str,
        proxy_port: u16,
    ) {
        *self.key.write().await = key.to_string();
        *self.secret.write().await = secret.as_bytes().to_vec();
        *self.host.write().await = host.to_string();
        *self.proxy_host.write().await = proxy_host.to_string();
        *self.proxy_port.write().await = proxy_port;
        
        // Recreate client with proxy if configured
        let new_client = if !proxy_host.is_empty() && proxy_port > 0 {
            let proxy_url = format!("http://{}:{}", proxy_host, proxy_port);
            match reqwest::Proxy::all(&proxy_url) {
                Ok(proxy) => {
                    match Client::builder()
                        .proxy(proxy)
                        .timeout(std::time::Duration::from_secs(30))
                        .build()
                    {
                        Ok(client) => {
                            tracing::info!("✅ REST 客户端代理配置成功: {}:{}", proxy_host, proxy_port);
                            client
                        }
                        Err(e) => {
                            tracing::warn!("⚠️ 创建带代理的 HTTP 客户端失败: {}", e);
                            Client::builder()
                                .timeout(std::time::Duration::from_secs(30))
                                .build()
                                .expect("Failed to create HTTP client")
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("⚠️ 无效的代理配置 {}:{}: {}", proxy_host, proxy_port, e);
                    Client::builder()
                        .timeout(std::time::Duration::from_secs(30))
                        .build()
                        .expect("Failed to create HTTP client")
                }
            }
        } else {
            Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client")
        };
        
        // Replace the client
        *self.client.write().await = new_client;
    }

    /// Get current timestamp in milliseconds
    fn get_timestamp(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as i64;

        let offset = self.time_offset.load(Ordering::Relaxed);
        if offset > 0 {
            now - offset.abs()
        } else if offset < 0 {
            now + offset.abs()
        } else {
            now
        }
    }

    /// Set time offset
    pub fn set_time_offset(&self, offset: i64) {
        self.time_offset.store(offset, Ordering::Relaxed);
    }

    /// Generate signature for request
    async fn sign(&self, query: &str) -> String {
        let secret = self.secret.read().await;
        let mut mac = HmacSha256::new_from_slice(&secret)
            .expect("HMAC can take key of any size");
        mac.update(query.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Build request URL with parameters
    async fn build_url(
        &self,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> String {
        let host = self.host.read().await;
        let mut url = format!("{}{}", host, path);

        let mut query_params = params.clone();

        if security == Security::Signed {
            let timestamp = self.get_timestamp();
            query_params.insert("timestamp".to_string(), timestamp.to_string());
            query_params.insert("recvWindow".to_string(), self.recv_window.to_string());

            // Sort and encode parameters
            let mut sorted_params: Vec<_> = query_params.iter().collect();
            sorted_params.sort_by(|a, b| a.0.cmp(b.0));

            let query_string: String = sorted_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");

            let signature = self.sign(&query_string).await;
            url = format!("{}?{}&signature={}", url, query_string, signature);
        } else if !query_params.is_empty() {
            let query_string: String = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url = format!("{}?{}", url, query_string);
        }

        url
    }

    /// Build request headers
    async fn build_headers(&self, security: Security) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            "application/json".parse().unwrap(),
        );

        if security == Security::Signed || security == Security::ApiKey {
            let key = self.key.read().await;
            headers.insert(
                "X-MBX-APIKEY",
                key.parse().unwrap(),
            );
        }

        headers
    }

    /// Send a request to the API
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> Result<Value, String> {
        let url = self.build_url(path, params, security).await;
        let headers = self.build_headers(security).await;

        debug!("Binance API request: {} {}", method, url);

        let client = self.client.read().await;
        let request = match method {
            Method::GET => client.get(&url),
            Method::POST => client.post(&url),
            Method::PUT => client.put(&url),
            Method::DELETE => client.delete(&url),
            _ => return Err(format!("Unsupported method: {}", method)),
        };

        let response = request
            .headers(headers)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            // Handle rate limit
            if status.as_u16() == 429 {
                warn!("Binance API rate limit hit: {}", text);
                return Err(format!("Rate limit: {}", text));
            }
            error!("Binance API error {}: {}", status, text);
            return Err(format!("API error {}: {}", status, text));
        }

        serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse JSON: {} - {}", e, text))
    }

    /// GET request
    pub async fn get(
        &self,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> Result<Value, String> {
        self.request(Method::GET, path, params, security).await
    }

    /// POST request
    pub async fn post(
        &self,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> Result<Value, String> {
        self.request(Method::POST, path, params, security).await
    }

    /// PUT request
    pub async fn put(
        &self,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> Result<Value, String> {
        self.request(Method::PUT, path, params, security).await
    }

    /// DELETE request
    pub async fn delete(
        &self,
        path: &str,
        params: &HashMap<String, String>,
        security: Security,
    ) -> Result<Value, String> {
        self.request(Method::DELETE, path, params, security).await
    }
}

impl Default for BinanceRestClient {
    fn default() -> Self {
        Self::new()
    }
}
