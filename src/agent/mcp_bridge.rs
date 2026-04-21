//! MCP Bridge — client for calling external LLM via MCP Sampling protocol.
//!
//! Since we cannot add an MCP client dependency (rmcp is server-only), we use
//! a trait-based approach. The `LlmClient` trait defines the interface for
//! requesting LLM reasoning, and `McpBridge` wraps a boxed trait object.
//!
//! A `NoOpLlmClient` is provided for testing that returns placeholder responses.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SamplingParams — parameters for LLM inference requests
// ---------------------------------------------------------------------------

/// Parameters for an LLM sampling request.
///
/// Mirrors the MCP Sampling protocol's `CreateMessageRequestParams`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingParams {
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Temperature for sampling (0.0–1.0).
    pub temperature: f32,
    /// Optional system prompt.
    pub system_prompt: Option<String>,
    /// Optional model preference hint.
    pub model_preference: Option<String>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
            model_preference: None,
        }
    }
}

impl SamplingParams {
    /// Create new sampling parameters with the given max tokens.
    pub fn new(max_tokens: u32) -> Self {
        Self {
            max_tokens,
            ..Self::default()
        }
    }

    /// Set the temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the model preference.
    pub fn with_model_preference(mut self, model: impl Into<String>) -> Self {
        self.model_preference = Some(model.into());
        self
    }
}

// ---------------------------------------------------------------------------
// LlmClient trait — abstraction for LLM inference
// ---------------------------------------------------------------------------

/// Trait for requesting LLM reasoning.
///
/// This trait abstracts the LLM client so that agents can use different
/// backends (MCP Sampling, direct API, local model, etc.) without coupling
/// to a specific implementation.
///
/// # Example
///
/// ```rust,ignore
/// use trade_engine::agent::mcp_bridge::{LlmClient, SamplingParams};
///
/// struct MyLlmClient;
///
/// #[async_trait::async_trait]
/// impl LlmClient for MyLlmClient {
///     async fn request_reasoning(&self, prompt: &str, params: SamplingParams) -> Result<String, String> {
///         // Call your LLM API here
///         Ok("response".to_string())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Request LLM reasoning for the given prompt.
    ///
    /// # Arguments
    /// * `prompt` - The user prompt to send to the LLM.
    /// * `params` - Sampling parameters (max tokens, temperature, etc.).
    ///
    /// # Returns
    /// The LLM's response text, or an error message.
    async fn request_reasoning(&self, prompt: &str, params: SamplingParams) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// NoOpLlmClient — test stub that returns placeholder responses
// ---------------------------------------------------------------------------

/// A no-op LLM client for testing.
///
/// Returns a placeholder response containing the prompt and parameters,
/// allowing tests to verify agent logic without an actual LLM.
pub struct NoOpLlmClient {
    /// The fixed response to return (if set).
    response: Option<String>,
}

impl NoOpLlmClient {
    /// Create a new `NoOpLlmClient` with a generic placeholder response.
    pub fn new() -> Self {
        Self { response: None }
    }

    /// Create a `NoOpLlmClient` that always returns the given response.
    pub fn with_response(response: impl Into<String>) -> Self {
        Self {
            response: Some(response.into()),
        }
    }
}

impl Default for NoOpLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LlmClient for NoOpLlmClient {
    async fn request_reasoning(&self, prompt: &str, _params: SamplingParams) -> Result<String, String> {
        if let Some(ref response) = self.response {
            Ok(response.clone())
        } else {
            Ok(format!("[NoOpLlmClient] Prompt: {}", prompt))
        }
    }
}

// ---------------------------------------------------------------------------
// McpBridgeConfig — configuration for McpBridge
// ---------------------------------------------------------------------------

/// Configuration for the MCP Bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpBridgeConfig {
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum number of retries on failure.
    pub max_retries: u32,
    /// Default model to use (if no override).
    pub default_model: Option<String>,
}

impl Default for McpBridgeConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_retries: 3,
            default_model: None,
        }
    }
}

impl McpBridgeConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the max retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }
}

// ---------------------------------------------------------------------------
// McpBridge — the bridge to external LLM services
// ---------------------------------------------------------------------------

/// Bridge for calling external LLM services via a trait-based client.
///
/// Wraps a boxed `LlmClient` trait object and provides a high-level
/// `request_reasoning` method with retry logic.
///
/// In production, the `LlmClient` implementation would communicate with
/// an MCP server using the Sampling protocol. For testing, use
/// `NoOpLlmClient`.
pub struct McpBridge {
    /// URL of the MCP server (for future use when a real client is available).
    server_url: String,
    /// Bridge configuration.
    config: McpBridgeConfig,
    /// The underlying LLM client.
    client: Box<dyn LlmClient>,
}

impl McpBridge {
    /// Create a new `McpBridge` with the given server URL and client.
    pub fn new(server_url: impl Into<String>, client: Box<dyn LlmClient>) -> Self {
        Self {
            server_url: server_url.into(),
            config: McpBridgeConfig::default(),
            client,
        }
    }

    /// Create a `McpBridge` with custom configuration.
    pub fn with_config(
        server_url: impl Into<String>,
        config: McpBridgeConfig,
        client: Box<dyn LlmClient>,
    ) -> Self {
        Self {
            server_url: server_url.into(),
            config,
            client,
        }
    }

    /// Create a `McpBridge` with a `NoOpLlmClient` for testing.
    pub fn new_noop(server_url: impl Into<String>) -> Self {
        Self::new(server_url, Box::new(NoOpLlmClient::new()))
    }

    /// Request LLM reasoning with retry logic.
    ///
    /// Constructs `SamplingParams` from the bridge config and delegates
    /// to the underlying `LlmClient`. Retries up to `max_retries` times
    /// on failure.
    ///
    /// # Arguments
    /// * `prompt` - The prompt to send to the LLM.
    /// * `params` - Optional sampling parameters override. If `None`,
    ///   default params from the config are used.
    ///
    /// # Returns
    /// The LLM response text, or an error after all retries are exhausted.
    pub async fn request_reasoning(
        &self,
        prompt: &str,
        params: Option<SamplingParams>,
    ) -> Result<String, String> {
        let sampling_params = params.unwrap_or_else(|| {
            let mut p = SamplingParams::new(1024);
            if let Some(ref model) = self.config.default_model {
                p = p.with_model_preference(model.clone());
            }
            p
        });

        let mut last_error = String::new();
        for attempt in 0..=self.config.max_retries {
            match self.client.request_reasoning(prompt, sampling_params.clone()).await {
                Ok(response) => {
                    if attempt > 0 {
                        tracing::info!(
                            attempt = attempt,
                            "McpBridge request succeeded after retry"
                        );
                    }
                    return Ok(response);
                }
                Err(e) => {
                    last_error = e;
                    if attempt < self.config.max_retries {
                        tracing::warn!(
                            attempt = attempt,
                            max_retries = self.config.max_retries,
                            error = %last_error,
                            "McpBridge request failed, retrying"
                        );
                        // Simple exponential backoff: 100ms * 2^attempt
                        let delay_ms = 100 * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(format!(
            "McpBridge request failed after {} retries: {}",
            self.config.max_retries, last_error
        ))
    }

    /// Get the server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Get the bridge config.
    pub fn config(&self) -> &McpBridgeConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_params_default() {
        let params = SamplingParams::default();
        assert_eq!(params.max_tokens, 1024);
        assert!((params.temperature - 0.7).abs() < f32::EPSILON);
        assert!(params.system_prompt.is_none());
        assert!(params.model_preference.is_none());
    }

    #[test]
    fn test_sampling_params_builder() {
        let params = SamplingParams::new(2048)
            .with_temperature(0.5)
            .with_system_prompt("You are a risk analyst.")
            .with_model_preference("gpt-4");

        assert_eq!(params.max_tokens, 2048);
        assert!((params.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(params.system_prompt.as_deref(), Some("You are a risk analyst."));
        assert_eq!(params.model_preference.as_deref(), Some("gpt-4"));
    }

    #[tokio::test]
    async fn test_noop_llm_client_default() {
        let client = NoOpLlmClient::new();
        let params = SamplingParams::default();
        let result = client.request_reasoning("Hello", params).await;
        assert!(result.is_ok());
        let response = result.expect("ok");
        assert!(response.contains("Hello"));
        assert!(response.contains("[NoOpLlmClient]"));
    }

    #[tokio::test]
    async fn test_noop_llm_client_with_response() {
        let client = NoOpLlmClient::with_response("Bullish");
        let params = SamplingParams::default();
        let result = client.request_reasoning("Analyze BTC", params).await;
        assert_eq!(result.expect("ok"), "Bullish");
    }

    #[test]
    fn test_mcp_bridge_config_default() {
        let config = McpBridgeConfig::default();
        assert_eq!(config.timeout_ms, 30_000);
        assert_eq!(config.max_retries, 3);
        assert!(config.default_model.is_none());
    }

    #[test]
    fn test_mcp_bridge_config_builder() {
        let config = McpBridgeConfig::new()
            .with_timeout(60_000)
            .with_max_retries(5)
            .with_default_model("claude-3");

        assert_eq!(config.timeout_ms, 60_000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.default_model.as_deref(), Some("claude-3"));
    }

    #[tokio::test]
    async fn test_mcp_bridge_request_reasoning_success() {
        let bridge = McpBridge::new_noop("http://localhost:3000");
        let result = bridge.request_reasoning("Analyze risk", None).await;
        assert!(result.is_ok());
        let response = result.expect("ok");
        assert!(response.contains("Analyze risk"));
    }

    #[tokio::test]
    async fn test_mcp_bridge_request_reasoning_with_custom_params() {
        let bridge = McpBridge::new_noop("http://localhost:3000");
        let params = SamplingParams::new(512).with_temperature(0.3);
        let result = bridge.request_reasoning("Test prompt", Some(params)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_bridge_with_custom_client() {
        let client = NoOpLlmClient::with_response("Risk level: moderate");
        let bridge = McpBridge::new("http://localhost:3000", Box::new(client));
        let result = bridge.request_reasoning("Assess risk", None).await;
        assert_eq!(result.expect("ok"), "Risk level: moderate");
    }

    #[test]
    fn test_mcp_bridge_server_url() {
        let bridge = McpBridge::new_noop("http://localhost:8080");
        assert_eq!(bridge.server_url(), "http://localhost:8080");
    }

    #[test]
    fn test_mcp_bridge_config_accessor() {
        let config = McpBridgeConfig::new().with_timeout(5000);
        let bridge = McpBridge::with_config(
            "http://localhost:3000",
            config,
            Box::new(NoOpLlmClient::new()),
        );
        assert_eq!(bridge.config().timeout_ms, 5000);
    }

    /// Failing LLM client for testing retry logic.
    struct FailingLlmClient {
        fail_count: std::sync::Mutex<usize>,
        succeed_after: usize,
    }

    impl FailingLlmClient {
        fn new(succeed_after: usize) -> Self {
            Self {
                fail_count: std::sync::Mutex::new(0),
                succeed_after,
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for FailingLlmClient {
        async fn request_reasoning(&self, _prompt: &str, _params: SamplingParams) -> Result<String, String> {
            let mut count = self.fail_count.lock().unwrap_or_else(|e| e.into_inner());
            *count += 1;
            if *count > self.succeed_after {
                Ok("success after retries".to_string())
            } else {
                Err("transient failure".to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_mcp_bridge_retry_succeeds() {
        let client = FailingLlmClient::new(1); // Fails once, succeeds on 2nd try
        let config = McpBridgeConfig::new().with_max_retries(3).with_timeout(100);
        let bridge = McpBridge::with_config(
            "http://localhost:3000",
            config,
            Box::new(client),
        );
        let result = bridge.request_reasoning("Test", None).await;
        assert!(result.is_ok());
        assert_eq!(result.expect("ok"), "success after retries");
    }

    #[tokio::test]
    async fn test_mcp_bridge_retry_exhausted() {
        let client = FailingLlmClient::new(999); // Always fails
        let config = McpBridgeConfig::new().with_max_retries(2).with_timeout(100);
        let bridge = McpBridge::with_config(
            "http://localhost:3000",
            config,
            Box::new(client),
        );
        let result = bridge.request_reasoning("Test", None).await;
        assert!(result.is_err());
        let err = result.expect_err("should be error");
        assert!(err.contains("failed after 2 retries"));
    }
}
