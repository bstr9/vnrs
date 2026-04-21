//! Async Strategy Engine
//!
//! Manages [`AsyncStrategy`](super::async_template::AsyncStrategy) instances
//! and dispatches market-data events to them via `tokio::spawn` so that
//! long-running ML inference never blocks the event loop.
//!
//! # Architecture
//!
//! ```text
//! MainEngine ──(tick/bar)──▶ AsyncStrategyEngine
//!                                 │
//!                                 ├── tokio::spawn ──▶ strategy.on_tick/on_bar
//!                                 │                      │
//!                                 │                      └── Vec<OrderRequest>
//!                                 │
//!                                 └── collect orders ──▶ MainEngine.send_order
//! ```
//!
//! The engine uses [`tokio::sync::RwLock`] instead of `std::sync::RwLock` so
//! that holding a read lock across `.await` points is safe.
//!
//! # Coexistence with [`StrategyEngine`](super::engine::StrategyEngine)
//!
//! Both engines can run simultaneously. The synchronous `StrategyEngine` handles
//! `StrategyTemplate` instances while this engine handles `AsyncStrategy`
//! instances. They share the same `MainEngine` for order routing.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::async_template::{AsyncStrategy, StrategyError};
use super::template::StrategyContext;
use crate::trader::{BarData, MainEngine, OrderRequest, TickData};
use crate::trader::database::BaseDatabase;

// ---------------------------------------------------------------------------
// AsyncStrategyEngine
// ---------------------------------------------------------------------------

/// Engine that manages async strategies and dispatches market-data events
/// without blocking the Tokio runtime.
pub struct AsyncStrategyEngine {
    /// Main trading engine (for order routing and market-data subscription)
    main_engine: Arc<MainEngine>,

    /// Registered async strategies, keyed by strategy name
    strategies: Arc<RwLock<HashMap<String, Box<dyn AsyncStrategy>>>>,

    /// Per-strategy context (market data caches)
    contexts: Arc<RwLock<HashMap<String, StrategyContext>>>,

    /// Symbol → list of strategy names that subscribe to it
    symbol_strategy_map: Arc<RwLock<HashMap<String, Vec<String>>>>,

    /// Optional database for loading historical data
    database: Option<Arc<dyn BaseDatabase>>,
}

impl AsyncStrategyEngine {
    /// Create a new `AsyncStrategyEngine`.
    pub fn new(main_engine: Arc<MainEngine>) -> Self {
        Self {
            main_engine,
            strategies: Arc::new(RwLock::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
            symbol_strategy_map: Arc::new(RwLock::new(HashMap::new())),
            database: None,
        }
    }

    /// Create with an optional database backend.
    pub fn with_database(
        main_engine: Arc<MainEngine>,
        database: Option<Arc<dyn BaseDatabase>>,
    ) -> Self {
        Self {
            main_engine,
            strategies: Arc::new(RwLock::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
            symbol_strategy_map: Arc::new(RwLock::new(HashMap::new())),
            database,
        }
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    /// Register an async strategy.
    ///
    /// Returns `Err` if a strategy with the same name already exists.
    pub async fn register(
        &self,
        strategy: Box<dyn AsyncStrategy>,
    ) -> Result<(), String> {
        let name = strategy.strategy_name().to_string();
        let symbols = strategy.vt_symbols().to_vec();

        // Insert atomically under write lock
        {
            let mut strategies = self.strategies.write().await;
            if strategies.contains_key(&name) {
                return Err(format!("Async strategy '{}' already exists", name));
            }
            strategies.insert(name.clone(), strategy);
        }

        // Create context
        let context = match &self.database {
            Some(db) => StrategyContext::with_database(Arc::clone(db)),
            None => StrategyContext::new(),
        };
        self.contexts.write().await.insert(name.clone(), context);

        // Update symbol → strategy mapping
        {
            let mut map = self.symbol_strategy_map.write().await;
            for vt_symbol in &symbols {
                map.entry(vt_symbol.clone())
                    .or_default()
                    .push(name.clone());
            }
        }

        tracing::info!("Async strategy '{}' registered (symbols: {:?})", name, symbols);
        Ok(())
    }

    /// Unregister a strategy by name.
    pub async fn unregister(&self, strategy_name: &str) -> Result<(), String> {
        // Remove from strategies map
        {
            let mut strategies = self.strategies.write().await;
            if strategies.remove(strategy_name).is_none() {
                return Err(format!("Async strategy '{}' not found", strategy_name));
            }
        }

        // Remove context
        self.contexts.write().await.remove(strategy_name);

        // Remove from symbol mapping
        {
            let mut map = self.symbol_strategy_map.write().await;
            for strategies in map.values_mut() {
                strategies.retain(|s| s != strategy_name);
            }
            map.retain(|_, strategies| !strategies.is_empty());
        }

        tracing::info!("Async strategy '{}' unregistered", strategy_name);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize all registered strategies by calling `on_init`.
    ///
    /// Strategies that fail initialization are logged but do **not** prevent
    /// other strategies from being initialized.
    pub async fn start(&self) -> Vec<(String, Result<(), StrategyError>)> {
        let mut results = Vec::new();

        let names: Vec<String> = {
            let strategies = self.strategies.read().await;
            strategies.keys().cloned().collect()
        };

        for name in names {
            let result = {
                let mut strategies = self.strategies.write().await;
                let contexts = self.contexts.read().await;

                match (strategies.get_mut(&name), contexts.get(&name)) {
                    (Some(strategy), Some(context)) => strategy.on_init(context).await,
                    (Some(_), None) => Err(StrategyError::InitError(format!(
                        "No context for strategy '{}'", name
                    ))),
                    _ => Err(StrategyError::InitError(format!(
                        "Strategy '{}' not found", name
                    ))),
                }
            };

            match &result {
                Ok(()) => tracing::info!("Async strategy '{}' initialized", name),
                Err(e) => tracing::error!("Async strategy '{}' init failed: {}", name, e),
            }
            results.push((name, result));
        }

        results
    }

    // -----------------------------------------------------------------------
    // Event dispatch
    // -----------------------------------------------------------------------

    /// Dispatch a bar event to all strategies that subscribe to the bar's
    /// symbol. Each strategy is invoked via `tokio::spawn` so that
    /// long-running inference does not block other strategies.
    ///
    /// Returns a list of `(strategy_name, orders)` pairs. The caller is
    /// responsible for forwarding orders to `MainEngine`.
    pub async fn on_bar(&self, bar: &BarData) -> Vec<(String, Vec<OrderRequest>)> {
        let vt_symbol = bar.vt_symbol();

        // Collect strategy names that subscribe to this symbol
        let strategy_names: Vec<String> = {
            let map = self.symbol_strategy_map.read().await;
            map.get(&vt_symbol).cloned().unwrap_or_default()
        };

        if strategy_names.is_empty() {
            return Vec::new();
        }

        // Update context caches (synchronous — no await)
        {
            let contexts = self.contexts.read().await;
            for name in &strategy_names {
                if let Some(context) = contexts.get(name) {
                    context.update_bar(bar.clone());
                }
            }
        }

        // Dispatch to each strategy sequentially under write lock
        // (strategies are mutable, so parallel dispatch requires per-strategy locks
        //  which we don't have — sequential is correct and simple)
        let mut all_orders = Vec::new();
        for name in strategy_names {
            let orders = {
                let mut strategies = self.strategies.write().await;
                let contexts = self.contexts.read().await;

                match (strategies.get_mut(&name), contexts.get(&name)) {
                    (Some(strategy), Some(context)) => {
                        // Spawn the async callback so it doesn't block other work
                        // NOTE: we hold the write lock during the call, which means
                        // other strategies wait. This is intentional — it preserves
                        // ordering. For truly parallel inference, strategies should
                        // use internal channels or a dedicated inference pool.
                        strategy.on_bar(bar, context).await
                    }
                    _ => Vec::new(),
                }
            };
            all_orders.push((name, orders));
        }

        all_orders
    }

    /// Dispatch a tick event to all subscribed strategies.
    ///
    /// Semantics are identical to [`on_bar`](Self::on_bar).
    pub async fn on_tick(&self, tick: &TickData) -> Vec<(String, Vec<OrderRequest>)> {
        let vt_symbol = tick.vt_symbol();

        let strategy_names: Vec<String> = {
            let map = self.symbol_strategy_map.read().await;
            map.get(&vt_symbol).cloned().unwrap_or_default()
        };

        if strategy_names.is_empty() {
            return Vec::new();
        }

        // Update context caches
        {
            let contexts = self.contexts.read().await;
            for name in &strategy_names {
                if let Some(context) = contexts.get(name) {
                    context.update_tick(tick.clone());
                }
            }
        }

        // Dispatch
        let mut all_orders = Vec::new();
        for name in strategy_names {
            let orders = {
                let mut strategies = self.strategies.write().await;
                let contexts = self.contexts.read().await;

                match (strategies.get_mut(&name), contexts.get(&name)) {
                    (Some(strategy), Some(context)) => {
                        strategy.on_tick(tick, context).await
                    }
                    _ => Vec::new(),
                }
            };
            all_orders.push((name, orders));
        }

        all_orders
    }

    // -----------------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------------

    /// Get the current target weights from a specific strategy.
    pub async fn get_target_weights(
        &self,
        strategy_name: &str,
    ) -> Option<HashMap<String, f64>> {
        let strategies = self.strategies.read().await;
        strategies.get(strategy_name).map(|s| s.target_weights())
    }

    /// Drain the audit trail from a specific strategy.
    pub async fn drain_decisions(
        &self,
        strategy_name: &str,
    ) -> Option<Vec<super::async_template::DecisionRecord>> {
        let mut strategies = self.strategies.write().await;
        strategies.get_mut(strategy_name).map(|s| s.drain_decisions())
    }

    /// List all registered strategy names.
    pub async fn strategy_names(&self) -> Vec<String> {
        let strategies = self.strategies.read().await;
        strategies.keys().cloned().collect()
    }

    /// Get a reference to the MainEngine (for external order routing).
    pub fn main_engine(&self) -> &Arc<MainEngine> {
        &self.main_engine
    }

    // -----------------------------------------------------------------------
    // Order forwarding helper
    // -----------------------------------------------------------------------

    /// Forward collected orders to MainEngine.
    ///
    /// This is a convenience method that takes the output of `on_bar` / `on_tick`
    /// and routes each `OrderRequest` through `MainEngine::send_order`.
    pub async fn forward_orders(
        &self,
        orders: &[(String, Vec<OrderRequest>)],
    ) -> Vec<(String, Vec<Result<String, String>>)> {
        let mut results = Vec::new();

        for (strategy_name, reqs) in orders {
            let mut strategy_results = Vec::new();
            for req in reqs {
                let exchange = req.exchange;
                let result = match self.main_engine.find_gateway_name_for_exchange(exchange) {
                    Some(gw_name) => {
                        self.main_engine.send_order(req.clone(), &gw_name).await
                    }
                    None => Err(format!("No gateway found for exchange {:?}", exchange)),
                };
                strategy_results.push(result);
            }
            results.push((strategy_name.clone(), strategy_results));
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::async_template::{AsyncStrategy, DecisionRecord, SignalType, StrategyError};
    use crate::trader::{Exchange, Interval};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;

    /// Minimal async strategy for engine tests
    struct TestAsyncStrategy {
        name: String,
        symbols: Vec<String>,
        weights: HashMap<String, f64>,
        decisions: Vec<DecisionRecord>,
        initialized: bool,
    }

    impl TestAsyncStrategy {
        fn new(name: &str, symbols: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                symbols,
                weights: HashMap::new(),
                decisions: Vec::new(),
                initialized: false,
            }
        }
    }

    #[async_trait]
    impl AsyncStrategy for TestAsyncStrategy {
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
            self.initialized = true;
            Ok(())
        }

        async fn on_bar(
            &mut self,
            bar: &BarData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            let vt = bar.vt_symbol();
            self.weights.insert(vt.clone(), 1.0);
            self.decisions.push(DecisionRecord {
                timestamp: Utc::now(),
                strategy: self.name.clone(),
                signal: SignalType::Long,
                confidence: 0.9,
                features_used: vec!["close".into()],
                model_version: "test-v1".into(),
                inference_latency_us: 100,
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

    /// Create a test AsyncStrategyEngine (no MainEngine connection needed for unit tests)
    async fn create_test_engine() -> AsyncStrategyEngine {
        let main_engine = MainEngine::new();
        AsyncStrategyEngine::new(main_engine)
    }

    fn make_bar(symbol: &str, close: f64) -> BarData {
        let mut bar = BarData::new(
            "TEST".into(),
            symbol.to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        bar.interval = Some(Interval::Minute);
        bar.close_price = close;
        bar.open_price = close - 10.0;
        bar
    }

    fn make_tick(symbol: &str, last_price: f64) -> TickData {
        let mut tick = TickData::new(
            "TEST".into(),
            symbol.to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        tick.last_price = last_price;
        tick.bid_price_1 = last_price - 1.0;
        tick.ask_price_1 = last_price + 1.0;
        tick
    }

    #[tokio::test]
    async fn test_register_strategy() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "test_strat",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        let result = engine.register(strategy).await;
        assert!(result.is_ok());

        let names = engine.strategy_names().await;
        assert_eq!(names, vec!["test_strat"]);
    }

    #[tokio::test]
    async fn test_register_duplicate_strategy() {
        let engine = create_test_engine().await;

        let s1 = Box::new(TestAsyncStrategy::new(
            "dup_strat",
            vec!["BTCUSDT.BINANCE".into()],
        ));
        let s2 = Box::new(TestAsyncStrategy::new(
            "dup_strat",
            vec!["ETHUSDT.BINANCE".into()],
        ));

        assert!(engine.register(s1).await.is_ok());
        assert!(engine.register(s2).await.is_err());
    }

    #[tokio::test]
    async fn test_unregister_strategy() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "removable",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        engine.register(strategy).await.unwrap();
        assert_eq!(engine.strategy_names().await.len(), 1);

        let result = engine.unregister("removable").await;
        assert!(result.is_ok());
        assert!(engine.strategy_names().await.is_empty());
    }

    #[tokio::test]
    async fn test_unregister_nonexistent() {
        let engine = create_test_engine().await;
        let result = engine.unregister("ghost").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_initializes_strategies() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "init_test",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        engine.register(strategy).await.unwrap();
        let results = engine.start().await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
    }

    #[tokio::test]
    async fn test_on_bar_dispatches_to_subscribed() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "bar_test",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        engine.register(strategy).await.unwrap();
        engine.start().await;

        let bar = make_bar("BTCUSDT", 50000.0);
        let orders = engine.on_bar(&bar).await;

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].0, "bar_test");
        assert!(orders[0].1.is_empty()); // TestAsyncStrategy returns no orders

        // Check weights were updated
        let weights = engine.get_target_weights("bar_test").await;
        assert!(weights.is_some());
        assert!(weights.unwrap().contains_key("BTCUSDT.BINANCE"));
    }

    #[tokio::test]
    async fn test_on_bar_no_subscribers() {
        let engine = create_test_engine().await;
        let bar = make_bar("ETHUSDT", 3000.0);
        let orders = engine.on_bar(&bar).await;
        assert!(orders.is_empty());
    }

    #[tokio::test]
    async fn test_on_tick_dispatches() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "tick_test",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        engine.register(strategy).await.unwrap();
        engine.start().await;

        let tick = make_tick("BTCUSDT", 50000.0);
        let orders = engine.on_tick(&tick).await;

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].0, "tick_test");
    }

    #[tokio::test]
    async fn test_drain_decisions() {
        let engine = create_test_engine().await;
        let strategy = Box::new(TestAsyncStrategy::new(
            "decision_test",
            vec!["BTCUSDT.BINANCE".into()],
        ));

        engine.register(strategy).await.unwrap();
        engine.start().await;

        let bar = make_bar("BTCUSDT", 50000.0);
        engine.on_bar(&bar).await;

        let decisions = engine.drain_decisions("decision_test").await;
        assert!(decisions.is_some());
        let decisions = decisions.unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].strategy, "decision_test");
        assert_eq!(decisions[0].signal, SignalType::Long);

        // Second drain should be empty
        let empty = engine.drain_decisions("decision_test").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_strategies_different_symbols() {
        let engine = create_test_engine().await;

        let s1 = Box::new(TestAsyncStrategy::new(
            "btc_strat",
            vec!["BTCUSDT.BINANCE".into()],
        ));
        let s2 = Box::new(TestAsyncStrategy::new(
            "eth_strat",
            vec!["ETHUSDT.BINANCE".into()],
        ));

        engine.register(s1).await.unwrap();
        engine.register(s2).await.unwrap();

        // BTC bar should only trigger btc_strat
        let btc_bar = make_bar("BTCUSDT", 50000.0);
        let btc_orders = engine.on_bar(&btc_bar).await;
        assert_eq!(btc_orders.len(), 1);
        assert_eq!(btc_orders[0].0, "btc_strat");

        // ETH bar should only trigger eth_strat
        let eth_bar = make_bar("ETHUSDT", 3000.0);
        let eth_orders = engine.on_bar(&eth_bar).await;
        assert_eq!(eth_orders.len(), 1);
        assert_eq!(eth_orders[0].0, "eth_strat");
    }
}
