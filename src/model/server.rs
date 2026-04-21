//! Model Server — trait for model inference and a ZMQ-based implementation.
//!
//! The `ModelServer` trait abstracts over different serving backends.
//! `ZmqModelServer` sends feature vectors over ZMQ to an external Python
//! inference service and receives prediction results.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

use crate::rpc::client::RpcClient;
use crate::rpc::common::RPC_TIMEOUT;

use super::types::{HealthStatus, ModelEntry, Prediction};

// ---------------------------------------------------------------------------
// ModelServer trait
// ---------------------------------------------------------------------------

/// Trait for model inference servers.
///
/// Implementations may call external services (ZMQ, HTTP, gRPC) or run
/// inference locally (future: ONNX, Candle).
#[async_trait]
pub trait ModelServer: Send + Sync {
    /// Run inference on the given feature map and return a prediction.
    async fn predict(&self, features: HashMap<String, f64>) -> Result<Prediction, String>;

    /// Return a reference to the model entry this server serves.
    fn model_info(&self) -> &ModelEntry;

    /// Check the health of the inference server.
    async fn health(&self) -> Result<HealthStatus, String>;
}

// ---------------------------------------------------------------------------
// ZmqModelServer — ZMQ-backed remote inference
// ---------------------------------------------------------------------------

/// A `ModelServer` that delegates inference to an external Python service
/// over the existing ZMQ RPC infrastructure.
///
/// On `predict()`, the feature map is serialized as JSON and sent via an
/// `RpcClient::call` to the `"predict"` RPC method. The remote service is
/// expected to return a JSON object matching the `Prediction` shape.
///
/// If the ZMQ connection is not available or the call fails, an
/// `InferenceError` is returned.
pub struct ZmqModelServer {
    entry: ModelEntry,
    client: Arc<RpcClient>,
    timeout_ms: u64,
    /// Track consecutive failures for health reporting.
    consecutive_failures: Arc<Mutex<u32>>,
    failure_threshold: u32,
}

impl ZmqModelServer {
    /// Create a new ZMQ model server.
    ///
    /// The `client` should already be connected to the inference service.
    pub fn new(entry: ModelEntry, client: Arc<RpcClient>) -> Self {
        Self {
            entry,
            client,
            timeout_ms: RPC_TIMEOUT.as_millis() as u64,
            consecutive_failures: Arc::new(Mutex::new(0)),
            failure_threshold: 3,
        }
    }

    /// Create with a custom RPC timeout.
    pub fn with_timeout(entry: ModelEntry, client: Arc<RpcClient>, timeout_ms: u64) -> Self {
        Self {
            entry,
            client,
            timeout_ms,
            consecutive_failures: Arc::new(Mutex::new(0)),
            failure_threshold: 3,
        }
    }
}

#[async_trait]
impl ModelServer for ZmqModelServer {
    async fn predict(&self, features: HashMap<String, f64>) -> Result<Prediction, String> {
        let start = Instant::now();

        let args = vec![
            serde_json::json!(self.entry.model_id),
            serde_json::json!(self.entry.version),
            serde_json::json!(features),
        ];

        let result = self
            .client
            .call_with_timeout(
                "predict".to_string(),
                args,
                HashMap::new(),
                self.timeout_ms,
            )
            .await
            .map_err(|e| {
                format!("InferenceError: ZMQ RPC call failed: {}", e)
            })?;

        // Parse response into Prediction
        let mut prediction: Prediction = serde_json::from_value(result)
            .map_err(|e| format!("InferenceError: failed to parse prediction response: {}", e))?;

        // Fill latency from our measurement
        prediction.latency_us = start.elapsed().as_micros() as u64;

        // Reset failure counter on success
        {
            let mut failures = self.consecutive_failures.lock().await;
            *failures = 0;
        }

        debug!(
            model_id = %self.entry.model_id,
            version = %self.entry.version,
            latency_us = prediction.latency_us,
            "Prediction completed via ZMQ"
        );

        Ok(prediction)
    }

    fn model_info(&self) -> &ModelEntry {
        &self.entry
    }

    async fn health(&self) -> Result<HealthStatus, String> {
        // Attempt a lightweight health-check RPC call
        let result = self
            .client
            .call_with_timeout(
                "health".to_string(),
                vec![serde_json::json!(self.entry.model_id)],
                HashMap::new(),
                5000, // 5s health-check timeout
            )
            .await;

        match result {
            Ok(val) => {
                // Try to parse as HealthStatus
                let status: Result<HealthStatus, _> = serde_json::from_value(val);
                match status {
                    Ok(s) => {
                        // Reset failure counter
                        let mut failures = self.consecutive_failures.lock().await;
                        *failures = 0;
                        Ok(s)
                    }
                    Err(_) => {
                        // Server responded but with unexpected format
                        warn!("Health check returned unparseable response, assuming Degraded");
                        Ok(HealthStatus::Degraded)
                    }
                }
            }
            Err(e) => {
                error!("Health check failed: {}", e);
                let mut failures = self.consecutive_failures.lock().await;
                *failures += 1;
                if *failures >= self.failure_threshold {
                    Ok(HealthStatus::Unhealthy)
                } else {
                    Ok(HealthStatus::Degraded)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::ModelStage;

    /// Verify `model_info()` returns the correct entry.
    #[tokio::test]
    async fn test_zmq_model_server_model_info() {
        let entry = ModelEntry::new("test_model", "1.0", "/models/test.onnx");
        let client = Arc::new(RpcClient::new());
        // Don't start the client — we only test model_info() which is sync
        let server = ZmqModelServer::new(entry.clone(), client);
        assert_eq!(server.model_info().model_id, "test_model");
        assert_eq!(server.model_info().version, "1.0");
        assert_eq!(server.model_info().stage, ModelStage::Development);
    }

    /// Verify `predict()` fails gracefully when ZMQ is not connected.
    #[tokio::test]
    async fn test_zmq_predict_fails_without_connection() {
        let entry = ModelEntry::new("test_model", "1.0", "/models/test.onnx");
        let client = Arc::new(RpcClient::new());
        let server = ZmqModelServer::new(entry, client);
        let features = HashMap::from([("close".to_string(), 42000.0)]);
        let result = server.predict(features).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("InferenceError"), "Expected InferenceError, got: {}", err);
    }

    /// Verify `health()` returns Degraded/Unhealthy when ZMQ is not connected.
    #[tokio::test]
    async fn test_zmq_health_without_connection() {
        let entry = ModelEntry::new("test_model", "1.0", "/models/test.onnx");
        let client = Arc::new(RpcClient::new());
        let server = ZmqModelServer::new(entry, client);
        let status = server.health().await.unwrap();
        // Without a connection, should report degraded or unhealthy
        assert!(matches!(status, HealthStatus::Degraded | HealthStatus::Unhealthy));
    }
}
