//! Async Strategy Template
//!
//! Async strategy trait that supports ML inference (ONNX, gRPC, LLM API)
//! without blocking the event loop. The weight-centric interface (FinRL-X pattern)
//! provides a unified interface for RL/LLM/traditional strategies.
//!
//! Unlike the synchronous [`StrategyTemplate`](super::template::StrategyTemplate),
//! this trait uses `async_trait` so that `on_init`, `on_bar`, and `on_tick` can
//! perform async work (ML inference, network calls) without blocking the Tokio
//! runtime.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::template::StrategyContext;
use crate::trader::{BarData, OrderRequest, TickData};

// ---------------------------------------------------------------------------
// Signal type
// ---------------------------------------------------------------------------

/// Signal direction produced by an async strategy.
///
/// This is the canonical signal vocabulary for the weight-centric interface.
/// Traditional strategies map Long/Short to positive/negative weights;
/// CloseLong/CloseShort map to zero-weight for the corresponding symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    /// Open / add to long position
    Long,
    /// Open / add to short position
    Short,
    /// No signal — hold current position
    Neutral,
    /// Close an existing long position
    CloseLong,
    /// Close an existing short position
    CloseShort,
}

impl std::fmt::Display for SignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalType::Long => write!(f, "Long"),
            SignalType::Short => write!(f, "Short"),
            SignalType::Neutral => write!(f, "Neutral"),
            SignalType::CloseLong => write!(f, "CloseLong"),
            SignalType::CloseShort => write!(f, "CloseShort"),
        }
    }
}

// ---------------------------------------------------------------------------
// Strategy error
// ---------------------------------------------------------------------------

/// Errors that can occur during async strategy lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StrategyError {
    /// Strategy initialization failed
    InitError(String),
    /// ML / model inference failed
    InferenceError(String),
    /// Order generation or validation failed
    OrderError(String),
    /// An async operation timed out
    TimeoutError(String),
}

impl std::fmt::Display for StrategyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyError::InitError(msg) => write!(f, "InitError: {}", msg),
            StrategyError::InferenceError(msg) => write!(f, "InferenceError: {}", msg),
            StrategyError::OrderError(msg) => write!(f, "OrderError: {}", msg),
            StrategyError::TimeoutError(msg) => write!(f, "TimeoutError: {}", msg),
        }
    }
}

impl std::error::Error for StrategyError {}

// ---------------------------------------------------------------------------
// Decision record
// ---------------------------------------------------------------------------

/// Audit trail entry for a single strategy decision.
///
/// Every time an async strategy produces a signal (via `on_bar` / `on_tick`),
/// it should record a [`DecisionRecord`] so that the full decision history can
/// be drained later with [`AsyncStrategy::drain_decisions`].
///
/// This is the core of the **reproducibility** guarantee: given the same market
/// data, the same model version, and the same features, the audit trail must
/// show the same signal and confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// When the decision was made (UTC)
    pub timestamp: DateTime<Utc>,
    /// Name of the strategy that produced this decision
    pub strategy: String,
    /// Signal direction
    pub signal: SignalType,
    /// Confidence in [0.0, 1.0]
    pub confidence: f64,
    /// Feature names that were fed into the model / decision function
    pub features_used: Vec<String>,
    /// Model version string (e.g. `"onnx-v2.3"` or `"gpt-4-0613"`)
    pub model_version: String,
    /// Wall-clock inference latency in **microseconds**
    pub inference_latency_us: u64,
    /// VT-order-IDs of the orders generated from this decision (may be empty)
    pub orders_generated: Vec<String>,
}

// ---------------------------------------------------------------------------
// AsyncStrategy trait
// ---------------------------------------------------------------------------

/// Async strategy trait for ML-driven trading strategies.
///
/// # Weight-centric interface (FinRL-X pattern)
///
/// The [`target_weights`](AsyncStrategy::target_weights) method returns a
/// `HashMap<vt_symbol, weight>` where:
/// - weight > 0 → long exposure proportional to weight
/// - weight < 0 → short exposure proportional to |weight|
/// - weight = 0 → no exposure / close position
///
/// This provides a unified interface for RL agents (which output action
/// vectors), LLM-based strategies (which can parse weight preferences from
/// text), and traditional quantitative strategies (which compute weights from
/// indicators).
///
/// # Coexistence with [`StrategyTemplate`]
///
/// The synchronous `StrategyTemplate` and this async trait are **independent**.
/// A running system may have both kinds of strategies registered — they are
/// managed by separate engines (`StrategyEngine` vs `AsyncStrategyEngine`).
#[async_trait]
pub trait AsyncStrategy: Send + Sync {
    /// Human-readable strategy name (must be unique within an `AsyncStrategyEngine`)
    fn strategy_name(&self) -> &str;

    /// List of vt_symbols this strategy subscribes to
    fn vt_symbols(&self) -> &[String];

    /// Initialize the strategy (load model, warm up indicators, etc.)
    ///
    /// Called once before the strategy starts receiving market data.
    async fn on_init(
        &mut self,
        context: &StrategyContext,
    ) -> Result<(), StrategyError>;

    /// Handle a new bar — run inference and optionally generate orders.
    async fn on_bar(
        &mut self,
        bar: &BarData,
        context: &StrategyContext,
    ) -> Vec<OrderRequest>;

    /// Handle a new tick — run inference and optionally generate orders.
    async fn on_tick(
        &mut self,
        tick: &TickData,
        context: &StrategyContext,
    ) -> Vec<OrderRequest>;

    /// Return the current portfolio weight vector.
    ///
    /// Keys are vt_symbols, values are signed weights in (-∞, +∞).
    /// A weight of 0.0 means "no position / close". The sum of absolute
    /// weights need not equal 1.0 — position sizing is handled downstream.
    fn target_weights(&self) -> HashMap<String, f64>;

    /// Drain and return the audit trail of decisions accumulated since the
    /// last call. The internal buffer is cleared.
    fn drain_decisions(&mut self) -> Vec<DecisionRecord>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Exchange, Interval};
    use chrono::Utc;
    use std::collections::HashMap;

    /// Minimal async strategy for testing
    struct DummyAsyncStrategy {
        name: String,
        symbols: Vec<String>,
        weights: HashMap<String, f64>,
        decisions: Vec<DecisionRecord>,
    }

    impl DummyAsyncStrategy {
        fn new(name: &str, symbols: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                symbols,
                weights: HashMap::new(),
                decisions: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl AsyncStrategy for DummyAsyncStrategy {
        fn strategy_name(&self) -> &str {
            &self.name
        }

        fn vt_symbols(&self) -> &[String] {
            &self.symbols
        }

        async fn on_init(
            &mut self,
            _context: &StrategyContext,
        ) -> Result<(), StrategyError> {
            Ok(())
        }

        async fn on_bar(
            &mut self,
            bar: &BarData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            let vt = bar.vt_symbol();
            self.weights.insert(vt.clone(), 0.5);
            self.decisions.push(DecisionRecord {
                timestamp: Utc::now(),
                strategy: self.name.clone(),
                signal: SignalType::Long,
                confidence: 0.8,
                features_used: vec!["close_price".into()],
                model_version: "dummy-v1".into(),
                inference_latency_us: 42,
                orders_generated: Vec::new(),
            });
            Vec::new()
        }

        async fn on_tick(
            &mut self,
            _tick: &TickData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            Vec::new()
        }

        fn target_weights(&self) -> HashMap<String, f64> {
            self.weights.clone()
        }

        fn drain_decisions(&mut self) -> Vec<DecisionRecord> {
            std::mem::take(&mut self.decisions)
        }
    }

    fn make_bar(close: f64) -> BarData {
        let mut bar = BarData::new(
            "TEST".into(),
            "BTCUSDT".into(),
            Exchange::Binance,
            Utc::now(),
        );
        bar.interval = Some(Interval::Minute);
        bar.close_price = close;
        bar
    }

    #[tokio::test]
    async fn test_async_strategy_on_init() {
        let mut s = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let result = s.on_init(&ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_async_strategy_on_bar_records_decision() {
        let mut s = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let bar = make_bar(50000.0);

        let orders = s.on_bar(&bar, &ctx).await;
        assert!(orders.is_empty()); // dummy produces no orders

        let decisions = s.drain_decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].signal, SignalType::Long);
        assert!((decisions[0].confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(decisions[0].model_version, "dummy-v1");
        assert_eq!(decisions[0].inference_latency_us, 42);
    }

    #[tokio::test]
    async fn test_async_strategy_target_weights() {
        let mut s = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let bar = make_bar(50000.0);
        s.on_bar(&bar, &ctx).await;

        let weights = s.target_weights();
        assert_eq!(weights.len(), 1);
        assert!((weights.get("BTCUSDT.BINANCE").copied().unwrap_or(0.0) - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_drain_decisions_clears_buffer() {
        let mut s = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let bar = make_bar(50000.0);
        s.on_bar(&bar, &ctx).await;

        let first = s.drain_decisions();
        assert_eq!(first.len(), 1);

        let second = s.drain_decisions();
        assert!(second.is_empty());
    }

    #[test]
    fn test_signal_type_display() {
        assert_eq!(format!("{}", SignalType::Long), "Long");
        assert_eq!(format!("{}", SignalType::Short), "Short");
        assert_eq!(format!("{}", SignalType::Neutral), "Neutral");
        assert_eq!(format!("{}", SignalType::CloseLong), "CloseLong");
        assert_eq!(format!("{}", SignalType::CloseShort), "CloseShort");
    }

    #[test]
    fn test_strategy_error_display() {
        assert_eq!(
            format!("{}", StrategyError::InitError("bad".into())),
            "InitError: bad"
        );
        assert_eq!(
            format!("{}", StrategyError::InferenceError("timeout".into())),
            "InferenceError: timeout"
        );
        assert_eq!(
            format!("{}", StrategyError::OrderError("invalid".into())),
            "OrderError: invalid"
        );
        assert_eq!(
            format!("{}", StrategyError::TimeoutError("100ms".into())),
            "TimeoutError: 100ms"
        );
    }

    #[test]
    fn test_decision_record_serialization() {
        let rec = DecisionRecord {
            timestamp: Utc::now(),
            strategy: "test".into(),
            signal: SignalType::Short,
            confidence: 0.65,
            features_used: vec!["rsi".into(), "macd".into()],
            model_version: "onnx-v2".into(),
            inference_latency_us: 1234,
            orders_generated: vec!["ORDER_1".into()],
        };
        let json = serde_json::to_string(&rec).unwrap();
        let deserialized: DecisionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.strategy, "test");
        assert_eq!(deserialized.signal, SignalType::Short);
        assert!((deserialized.confidence - 0.65).abs() < f64::EPSILON);
        assert_eq!(deserialized.features_used.len(), 2);
        assert_eq!(deserialized.inference_latency_us, 1234);
    }

    // -----------------------------------------------------------------------
    // MomentumAsyncStrategy — a simple example strategy
    // -----------------------------------------------------------------------

    /// Example: a simple momentum strategy that uses async on_bar to produce
    /// a Long signal when close > open (bullish bar) and Short otherwise.
    ///
    /// This demonstrates the weight-centric interface: after each bar the
    /// strategy updates `target_weights` with a weight of `+confidence` for
    /// Long or `-confidence` for Short.
    struct MomentumAsyncStrategy {
        name: String,
        symbols: Vec<String>,
        weights: HashMap<String, f64>,
        decisions: Vec<DecisionRecord>,
        model_version: String,
    }

    impl MomentumAsyncStrategy {
        fn new(name: &str, symbols: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                symbols,
                weights: HashMap::new(),
                decisions: Vec::new(),
                model_version: "momentum-v1".into(),
            }
        }
    }

    #[async_trait]
    impl AsyncStrategy for MomentumAsyncStrategy {
        fn strategy_name(&self) -> &str {
            &self.name
        }

        fn vt_symbols(&self) -> &[String] {
            &self.symbols
        }

        async fn on_init(
            &mut self,
            _context: &StrategyContext,
        ) -> Result<(), StrategyError> {
            // In a real strategy, load ONNX model / connect to gRPC endpoint here.
            Ok(())
        }

        async fn on_bar(
            &mut self,
            bar: &BarData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            let vt = bar.vt_symbol();
            let start = std::time::Instant::now();

            // Simple momentum signal: bullish bar → Long, bearish → Short
            let (signal, confidence) = if bar.close_price > bar.open_price {
                (SignalType::Long, 0.6)
            } else if bar.close_price < bar.open_price {
                (SignalType::Short, 0.6)
            } else {
                (SignalType::Neutral, 0.0)
            };

            // Update target weights (weight-centric interface)
            let weight = match signal {
                SignalType::Long => confidence,
                SignalType::Short => -confidence,
                SignalType::CloseLong | SignalType::CloseShort => 0.0,
                SignalType::Neutral => 0.0,
            };
            self.weights.insert(vt.clone(), weight);

            let latency_us = start.elapsed().as_micros() as u64;
            self.decisions.push(DecisionRecord {
                timestamp: Utc::now(),
                strategy: self.name.clone(),
                signal,
                confidence,
                features_used: vec!["open_price".into(), "close_price".into()],
                model_version: self.model_version.clone(),
                inference_latency_us: latency_us,
                orders_generated: Vec::new(),
            });

            Vec::new()
        }

        async fn on_tick(
            &mut self,
            _tick: &TickData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            Vec::new()
        }

        fn target_weights(&self) -> HashMap<String, f64> {
            self.weights.clone()
        }

        fn drain_decisions(&mut self) -> Vec<DecisionRecord> {
            std::mem::take(&mut self.decisions)
        }
    }

    #[tokio::test]
    async fn test_momentum_strategy_bullish() {
        let mut s = MomentumAsyncStrategy::new("momentum", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let mut bar = make_bar(50100.0);
        bar.open_price = 50000.0;

        s.on_bar(&bar, &ctx).await;
        let weights = s.target_weights();
        assert!(weights["BTCUSDT.BINANCE"] > 0.0, "bullish bar should have positive weight");

        let decisions = s.drain_decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].signal, SignalType::Long);
    }

    #[tokio::test]
    async fn test_momentum_strategy_bearish() {
        let mut s = MomentumAsyncStrategy::new("momentum", vec!["BTCUSDT.BINANCE".into()]);
        let ctx = StrategyContext::new();
        let mut bar = make_bar(49900.0);
        bar.open_price = 50000.0;

        s.on_bar(&bar, &ctx).await;
        let weights = s.target_weights();
        assert!(weights["BTCUSDT.BINANCE"] < 0.0, "bearish bar should have negative weight");

        let decisions = s.drain_decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].signal, SignalType::Short);
    }
}
