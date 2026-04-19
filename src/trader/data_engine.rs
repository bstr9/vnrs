//! DataEngine — centralized subscription management and tick→bar aggregation.
//!
//! Provides:
//! - Centralized subscription registry (de-duplicates gateway subscriptions)
//! - Tick→1m bar aggregation pipeline
//! - 1m→higher timeframe bar synthesis (5m/15m/1h/4h/1d)
//! - Tick and bar caching for query interface
//!
//! # Example
//!
//! ```ignore
//! use trade_engine::trader::data_engine::DataEngine;
//! use trade_engine::trader::{MainEngine, Interval};
//! use std::sync::Arc;
//!
//! let main_engine = Arc::new(MainEngine::new());
//! let data_engine = main_engine.add_data_engine();
//!
//! // Subscribe to BTCUSDT 5-minute bars
//! data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute5, "my_strategy", "BINANCE_SPOT").unwrap();
//!
//! // Later, get the latest bar
//! if let Some(bar) = data_engine.get_bar("BTCUSDT.BINANCE", Interval::Minute5) {
//!     println!("Latest 5m close: {}", bar.close_price);
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Timelike, Utc};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::bar_synthesizer::BarSynthesizer;
use super::constant::Interval;
use super::engine::{BaseEngine, MainEngine};
use super::event::{EVENT_BAR, EVENT_TICK};
use super::gateway::GatewayEvent;
use super::object::{BarData, TickData};

// ---------------------------------------------------------------------------
// TickBarAggregator trait
// ---------------------------------------------------------------------------

/// Trait for tick→bar aggregation, allowing `Box<dyn>` usage.
///
/// This abstraction is needed because the existing `BarGenerator` uses generic
/// callbacks and cannot be stored in a collection.
pub trait TickBarAggregator: Send + Sync {
    /// Feed a tick; returns completed 1m `BarData` if bar was finished.
    fn update_tick(&mut self, tick: TickData) -> Option<BarData>;

    /// Force-generate the current bar (for session close, etc.).
    fn generate(&mut self) -> Option<BarData>;
}

// ---------------------------------------------------------------------------
// DefaultBarAggregator — concrete tick→1m implementation
// ---------------------------------------------------------------------------

/// Default implementation of tick→1m bar aggregation.
///
/// Reimplements the logic from `BarGenerator` without generic callbacks,
/// returning completed bars instead of calling a callback.
pub struct DefaultBarAggregator {
    /// Currently accumulating 1-minute bar
    bar: Option<BarData>,
    /// Last tick seen (for volume delta calculation)
    last_tick: Option<TickData>,
}

impl DefaultBarAggregator {
    /// Create a new `DefaultBarAggregator`.
    pub fn new() -> Self {
        Self {
            bar: None,
            last_tick: None,
        }
    }
}

impl Default for DefaultBarAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl TickBarAggregator for DefaultBarAggregator {
    fn update_tick(&mut self, tick: TickData) -> Option<BarData> {
        // Filter tick data with 0 last price
        if tick.last_price == 0.0 {
            return None;
        }

        let mut completed_bar: Option<BarData> = None;
        let mut new_minute = false;

        // Check if we need to emit the current bar
        if self.bar.is_none() {
            new_minute = true;
        } else if let Some(ref bar) = self.bar {
            let bar_minute = bar.datetime.format("%M").to_string();
            let tick_minute = tick.datetime.format("%M").to_string();
            let bar_hour = bar.datetime.format("%H").to_string();
            let tick_hour = tick.datetime.format("%H").to_string();

            if bar_minute != tick_minute || bar_hour != tick_hour {
                // Minute changed — emit the completed bar
                if let Some(mut finished_bar) = self.bar.take() {
                    finished_bar.datetime = finished_bar
                        .datetime
                        .with_second(0)
                        .unwrap_or(finished_bar.datetime)
                        .with_nanosecond(0)
                        .unwrap_or(finished_bar.datetime);
                    finished_bar.interval = Some(Interval::Minute);
                    completed_bar = Some(finished_bar);
                }
                new_minute = true;
            }
        }

        // Create new bar or update existing
        if new_minute {
            self.bar = Some(BarData {
                gateway_name: tick.gateway_name.clone(),
                symbol: tick.symbol.clone(),
                exchange: tick.exchange,
                datetime: tick.datetime,
                interval: Some(Interval::Minute),
                volume: 0.0,
                turnover: 0.0,
                open_interest: tick.open_interest,
                open_price: tick.last_price,
                high_price: tick.last_price,
                low_price: tick.last_price,
                close_price: tick.last_price,
                extra: None,
            });
        } else if let Some(ref mut bar) = self.bar {
            bar.high_price = bar.high_price.max(tick.last_price);
            bar.low_price = bar.low_price.min(tick.last_price);
            bar.close_price = tick.last_price;
            bar.open_interest = tick.open_interest;
            bar.datetime = tick.datetime;
        }

        // Update volume from delta
        if let (Some(ref last_tick), Some(ref mut bar)) = (&self.last_tick, &mut self.bar) {
            let volume_change = tick.volume - last_tick.volume;
            bar.volume += volume_change.max(0.0);

            let turnover_change = tick.turnover - last_tick.turnover;
            bar.turnover += turnover_change.max(0.0);
        }

        self.last_tick = Some(tick);
        completed_bar
    }

    fn generate(&mut self) -> Option<BarData> {
        if let Some(mut bar) = self.bar.take() {
            bar.datetime = bar
                .datetime
                .with_second(0)
                .unwrap_or(bar.datetime)
                .with_nanosecond(0)
                .unwrap_or(bar.datetime);
            bar.interval = Some(Interval::Minute);
            Some(bar)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// DataEngine — centralized subscription and bar aggregation
// ---------------------------------------------------------------------------

/// Centralized data engine for subscription management and tick→bar aggregation.
///
/// Key responsibilities:
/// - De-duplicate gateway subscriptions when multiple strategies subscribe to same symbol
/// - Aggregate ticks into 1-minute bars centrally
/// - Synthesize higher-timeframe bars (5m/15m/1h/4h/1d) from 1m bars
/// - Cache latest tick/bar for query interface
/// - Emit bar events into MainEngine's event stream for StrategyEngine consumption
pub struct DataEngine {
    /// Reference to MainEngine (for gateway subscription)
    main_engine: Arc<MainEngine>,
    /// Subscription registry: key = "vt_symbol.Interval" → list of subscriber names
    subscriptions: RwLock<HashMap<String, Vec<String>>>,
    /// Per-symbol 1m bar aggregator: key = vt_symbol
    bar_generators: RwLock<HashMap<String, Box<dyn TickBarAggregator>>>,
    /// Per-(symbol, interval) bar synthesizer: key = "vt_symbol.Interval" (higher TF only)
    bar_synthesizers: RwLock<HashMap<String, BarSynthesizer>>,
    /// Tick cache: latest tick per symbol
    tick_cache: RwLock<HashMap<String, TickData>>,
    /// Bar cache: latest bar per (symbol, interval), key = "vt_symbol.Interval"
    bar_cache: RwLock<HashMap<String, BarData>>,
    /// Event sender to inject bar events into MainEngine's event stream
    event_tx: mpsc::UnboundedSender<(String, GatewayEvent)>,
    /// Track which symbols have gateway subscriptions (for de-duplication)
    gateway_subscriptions: RwLock<HashMap<String, String>>, // vt_symbol -> gateway_name
}

impl DataEngine {
    /// Create a new DataEngine.
    pub fn new(
        main_engine: Arc<MainEngine>,
        event_tx: mpsc::UnboundedSender<(String, GatewayEvent)>,
    ) -> Self {
        Self {
            main_engine,
            subscriptions: RwLock::new(HashMap::new()),
            bar_generators: RwLock::new(HashMap::new()),
            bar_synthesizers: RwLock::new(HashMap::new()),
            tick_cache: RwLock::new(HashMap::new()),
            bar_cache: RwLock::new(HashMap::new()),
            event_tx,
            gateway_subscriptions: RwLock::new(HashMap::new()),
        }
    }

    /// Subscribe to market data for a symbol+interval.
    ///
    /// If this is the first subscriber for this (symbol, interval):
    /// 1. Register in subscriptions map
    /// 2. Create BarGenerator for this symbol if needed
    /// 3. Create BarSynthesizer for higher timeframes if needed
    /// 4. Subscribe to gateway if this is the first interval for this symbol
    ///
    /// If already subscribed, just add the subscriber name.
    pub fn subscribe(
        &self,
        vt_symbol: &str,
        interval: Interval,
        subscriber: &str,
        gateway_name: &str,
    ) -> Result<(), String> {
        let key = subscription_key(vt_symbol, interval);

        // Check if this is a new subscription for this (symbol, interval)
        let is_new_subscription = {
            let subscriptions = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
            !subscriptions.contains_key(&key)
        };

        // Add subscriber to registry
        {
            let mut subscriptions = self.subscriptions.write().unwrap_or_else(|e| e.into_inner());
            subscriptions
                .entry(key.clone())
                .or_insert_with(Vec::new)
                .push(subscriber.to_string());
        }

        // Create BarGenerator for this symbol if it doesn't exist
        {
            let mut generators = self.bar_generators.write().unwrap_or_else(|e| e.into_inner());
            if !generators.contains_key(vt_symbol) {
                generators.insert(vt_symbol.to_string(), Box::new(DefaultBarAggregator::new()));
                info!("创建K线聚合器: {}", vt_symbol);
            }
        }

        // Create BarSynthesizer for higher timeframes if needed
        if interval != Interval::Minute && interval != Interval::Tick {
            let mut synthesizers = self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner());
            if !synthesizers.contains_key(&key) {
                let synth = BarSynthesizer::new(Interval::Minute, interval);
                synthesizers.insert(key.clone(), synth);
                info!("创建K线合成器: {} -> {:?}", vt_symbol, interval);
            }
        }

        // Subscribe to gateway if this is the first subscription for this symbol
        {
            let mut gateway_subs = self.gateway_subscriptions.write().unwrap_or_else(|e| e.into_inner());
            if !gateway_subs.contains_key(vt_symbol) {
                // Note: MainEngine.subscribe() is async and cannot be called from sync context.
                // The caller (StrategyEngine) should handle gateway subscription separately.
                // We just track the subscription for de-duplication purposes.
                gateway_subs.insert(vt_symbol.to_string(), gateway_name.to_string());
                info!("数据引擎记录网关订阅: {} (gateway: {})", vt_symbol, gateway_name);
            }
        }

        debug!(
            "数据引擎订阅: {} {:?} <- {}",
            vt_symbol, interval, subscriber
        );
        Ok(())
    }

    /// Unsubscribe from market data.
    ///
    /// If this is the last subscriber for this (symbol, interval):
    /// - Remove from subscriptions map
    /// - Remove associated bar_synthesizer
    /// - If no subscriptions remain for this symbol at ANY interval:
    ///   - Remove bar_generator
    ///   - Unsubscribe from gateway
    pub fn unsubscribe(&self, vt_symbol: &str, interval: Interval, subscriber: &str) -> Result<(), String> {
        let key = subscription_key(vt_symbol, interval);

        // Remove subscriber from registry
        let was_last_subscriber = {
            let mut subscriptions = self.subscriptions.write().unwrap_or_else(|e| e.into_inner());
            if let Some(subscribers) = subscriptions.get_mut(&key) {
                subscribers.retain(|s| s != subscriber);
                if subscribers.is_empty() {
                    subscriptions.remove(&key);
                    true
                } else {
                    false
                }
            } else {
                return Err(format!("订阅不存在: {} {:?}", vt_symbol, interval));
            }
        };

        if was_last_subscriber {
            // Remove bar synthesizer for this interval
            {
                let mut synthesizers = self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner());
                synthesizers.remove(&key);
            }

            // Check if any subscriptions remain for this symbol
            let has_other_subscriptions = {
                let subscriptions = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
                subscriptions.keys().any(|k| k.starts_with(&format!("{}.", vt_symbol)))
            };

            if !has_other_subscriptions {
                // Remove bar generator
                {
                    let mut generators = self.bar_generators.write().unwrap_or_else(|e| e.into_inner());
                    generators.remove(vt_symbol);
                }

                // Clear caches
                {
                    let mut tick_cache = self.tick_cache.write().unwrap_or_else(|e| e.into_inner());
                    tick_cache.remove(vt_symbol);
                }
                {
                    let mut bar_cache = self.bar_cache.write().unwrap_or_else(|e| e.into_inner());
                    bar_cache.retain(|k, _| !k.starts_with(&format!("{}.", vt_symbol)));
                }

                // Unsubscribe from gateway
                {
                    let mut gateway_subs = self.gateway_subscriptions.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(gateway_name) = gateway_subs.remove(vt_symbol) {
                        let contract = self.main_engine.get_contract(vt_symbol);
                        if let Some(contract) = contract {
                            let req = super::object::SubscribeRequest {
                                symbol: contract.symbol.clone(),
                                exchange: contract.exchange,
                            };
                            // Note: MainEngine doesn't have unsubscribe method, so we just log
                            info!("网关取消订阅: {} (gateway: {})", vt_symbol, gateway_name);
                        }
                    }
                }

                info!("数据引擎取消订阅: {} (无剩余订阅者)", vt_symbol);
            }
        }

        Ok(())
    }

    /// Process a tick event from MainEngine.
    ///
    /// 1. Cache the tick
    /// 2. Feed tick to symbol's BarGenerator
    /// 3. If BarGenerator produces a 1m bar:
    ///    a. Cache the 1m bar
    ///    b. Emit GatewayEvent::Bar into event stream
    ///    c. Feed 1m bar to all BarSynthesizers for this symbol
    ///    d. If BarSynthesizer produces a higher-TF bar, cache and emit it
    pub fn process_tick(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();

        // Cache tick
        {
            let mut cache = self.tick_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.insert(vt_symbol.clone(), tick.clone());
        }

        // Feed to BarGenerator
        let one_min_bar = {
            let mut generators = self.bar_generators.write().unwrap_or_else(|e| e.into_inner());
            if let Some(generator) = generators.get_mut(&vt_symbol) {
                generator.update_tick(tick.clone())
            } else {
                None
            }
        };

        // If we got a 1m bar, process it
        if let Some(bar) = one_min_bar {
            self.process_completed_bar(&bar);
        }
    }

    /// Process a completed 1m bar.
    ///
    /// Caches the bar, emits it to event stream, and feeds to synthesizers.
    fn process_completed_bar(&self, bar: &BarData) {
        let vt_symbol = bar.vt_symbol();

        // Cache 1m bar
        let one_min_key = subscription_key(&vt_symbol, Interval::Minute);
        {
            let mut cache = self.bar_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.insert(one_min_key.clone(), bar.clone());
        }

        // Emit 1m bar event
        let event_type = format!("{}{}", EVENT_BAR, vt_symbol);
        if let Err(e) = self.event_tx.send((event_type, GatewayEvent::Bar(bar.clone()))) {
            warn!("发送1分钟K线事件失败: {}", e);
        }

        // Feed to all synthesizers for this symbol
        let synthesized_bars: Vec<(String, BarData)> = {
            let mut synthesizers = self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner());
            let mut results = Vec::new();

            for (key, synthesizer) in synthesizers.iter_mut() {
                // Check if this synthesizer is for this symbol
                if key.starts_with(&format!("{}.", vt_symbol)) {
                    if let Some(synthesized) = synthesizer.update_bar(bar) {
                        results.push((key.clone(), synthesized));
                    }
                }
            }
            results
        };

        // Cache and emit synthesized bars
        for (key, synthesized_bar) in synthesized_bars {
            // Cache
            {
                let mut cache = self.bar_cache.write().unwrap_or_else(|e| e.into_inner());
                cache.insert(key.clone(), synthesized_bar.clone());
            }

            // Emit
            let event_type = format!("{}{}", EVENT_BAR, vt_symbol);
            if let Err(e) = self.event_tx.send((event_type, GatewayEvent::Bar(synthesized_bar))) {
                warn!("发送合成K线事件失败: {}", e);
            }
        }
    }

    /// Process a bar event (for bars that arrive from gateway directly).
    pub fn process_bar(&self, bar: &BarData) {
        let vt_symbol = bar.vt_symbol();

        // Determine interval from bar
        let interval = bar.interval.unwrap_or(Interval::Minute);
        let key = subscription_key(&vt_symbol, interval);

        // Cache the bar
        {
            let mut cache = self.bar_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.insert(key, bar.clone());
        }
    }

    /// Get the latest tick for a symbol.
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        let cache = self.tick_cache.read().unwrap_or_else(|e| e.into_inner());
        cache.get(vt_symbol).cloned()
    }

    /// Get the latest bar for a symbol+interval.
    pub fn get_bar(&self, vt_symbol: &str, interval: Interval) -> Option<BarData> {
        let key = subscription_key(vt_symbol, interval);
        let cache = self.bar_cache.read().unwrap_or_else(|e| e.into_inner());
        cache.get(&key).cloned()
    }

    /// Get all active subscriptions.
    pub fn get_subscriptions(&self) -> HashMap<String, Vec<String>> {
        self.subscriptions.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Check if a symbol+interval is subscribed.
    pub fn is_subscribed(&self, vt_symbol: &str, interval: Interval) -> bool {
        let key = subscription_key(vt_symbol, interval);
        let subscriptions = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        subscriptions.contains_key(&key)
    }

    /// Get the number of subscribers for a symbol+interval.
    pub fn subscriber_count(&self, vt_symbol: &str, interval: Interval) -> usize {
        let key = subscription_key(vt_symbol, interval);
        let subscriptions = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        subscriptions.get(&key).map(|v| v.len()).unwrap_or(0)
    }

    /// Force-generate all pending bars (for session close, etc.).
    pub fn generate_all(&self) -> Vec<BarData> {
        let mut results = Vec::new();
        let mut generators = self.bar_generators.write().unwrap_or_else(|e| e.into_inner());

        for (vt_symbol, generator) in generators.iter_mut() {
            if let Some(bar) = generator.generate() {
                results.push(bar);
            }
        }

        // Process each generated bar
        for bar in &results {
            self.process_completed_bar(bar);
        }

        results
    }
}

impl BaseEngine for DataEngine {
    fn engine_name(&self) -> &str {
        "data"
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        match event_type {
            t if t.starts_with(EVENT_TICK) => {
                if let GatewayEvent::Tick(tick) = event {
                    self.process_tick(tick);
                }
            }
            t if t.starts_with(EVENT_BAR) => {
                if let GatewayEvent::Bar(bar) = event {
                    self.process_bar(bar);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Generate subscription key from vt_symbol and interval.
fn subscription_key(vt_symbol: &str, interval: Interval) -> String {
    format!("{}.{:?}", vt_symbol, interval)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::Exchange;

    fn make_tick(price: f64, volume: f64, minute_offset: i64) -> TickData {
        let dt = DateTime::UNIX_EPOCH + chrono::Duration::minutes(minute_offset);
        let mut tick = TickData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            dt,
        );
        tick.last_price = price;
        tick.volume = volume;
        tick.turnover = price * volume;
        tick.bid_price_1 = price - 1.0;
        tick.bid_volume_1 = 1.0;
        tick.ask_price_1 = price + 1.0;
        tick.ask_volume_1 = 1.0;
        tick
    }

    #[test]
    fn test_default_bar_aggregator_single_bar() {
        let mut aggregator = DefaultBarAggregator::new();

        // Feed ticks within the same minute
        let tick1 = make_tick(100.0, 10.0, 0);
        let tick2 = make_tick(101.0, 20.0, 0); // Same minute, different price

        let result1 = aggregator.update_tick(tick1);
        assert!(result1.is_none(), "First tick should not produce a bar");

        let result2 = aggregator.update_tick(tick2);
        assert!(result2.is_none(), "Second tick in same minute should not produce a bar");

        // Feed tick in next minute
        let tick3 = make_tick(102.0, 30.0, 1); // Next minute
        let result3 = aggregator.update_tick(tick3);
        assert!(result3.is_some(), "Tick in new minute should produce a bar");

        let bar = result3.unwrap();
        assert_eq!(bar.open_price, 100.0);
        assert_eq!(bar.high_price, 101.0);
        assert_eq!(bar.low_price, 100.0);
        assert_eq!(bar.close_price, 101.0);
        assert!((bar.volume - 10.0).abs() < f64::EPSILON); // Volume delta
    }

    #[test]
    fn test_default_bar_aggregator_generate() {
        let mut aggregator = DefaultBarAggregator::new();

        let tick = make_tick(100.0, 10.0, 0);
        let _ = aggregator.update_tick(tick);

        // Force generate without minute change
        let bar = aggregator.generate();
        assert!(bar.is_some());

        let bar = bar.unwrap();
        assert_eq!(bar.close_price, 100.0);
    }

    #[test]
    fn test_default_bar_aggregator_zero_price_filter() {
        let mut aggregator = DefaultBarAggregator::new();

        // Tick with zero price should be filtered
        let tick = make_tick(0.0, 10.0, 0);
        let result = aggregator.update_tick(tick);
        assert!(result.is_none());
    }

    #[test]
    fn test_subscription_key() {
        assert_eq!(
            subscription_key("BTCUSDT.BINANCE", Interval::Minute),
            "BTCUSDT.BINANCE.Minute"
        );
        assert_eq!(
            subscription_key("ETHUSDT.BINANCE", Interval::Minute5),
            "ETHUSDT.BINANCE.Minute5"
        );
    }

    #[test]
    fn test_data_engine_subscribe_unsubscribe() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        // Subscribe
        let result = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy1", "BINANCE");
        assert!(result.is_ok());
        assert!(data_engine.is_subscribed("BTCUSDT.BINANCE", Interval::Minute));
        assert_eq!(data_engine.subscriber_count("BTCUSDT.BINANCE", Interval::Minute), 1);

        // Add another subscriber
        let result = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy2", "BINANCE");
        assert!(result.is_ok());
        assert_eq!(data_engine.subscriber_count("BTCUSDT.BINANCE", Interval::Minute), 2);

        // Unsubscribe one
        let result = data_engine.unsubscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy1");
        assert!(result.is_ok());
        assert_eq!(data_engine.subscriber_count("BTCUSDT.BINANCE", Interval::Minute), 1);

        // Unsubscribe last
        let result = data_engine.unsubscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy2");
        assert!(result.is_ok());
        assert!(!data_engine.is_subscribed("BTCUSDT.BINANCE", Interval::Minute));
    }

    #[test]
    fn test_data_engine_higher_timeframe_subscription() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        // Subscribe to 5m bars
        let result = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute5, "strategy1", "BINANCE");
        assert!(result.is_ok());

        // Should create both 1m generator and 5m synthesizer
        {
            let generators = data_engine.bar_generators.read().unwrap_or_else(|e| e.into_inner());
            assert!(generators.contains_key("BTCUSDT.BINANCE"));
        }
        {
            let synthesizers = data_engine.bar_synthesizers.read().unwrap_or_else(|e| e.into_inner());
            assert!(synthesizers.contains_key("BTCUSDT.BINANCE.Minute5"));
        }
    }

    #[test]
    fn test_data_engine_tick_caching() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        // Subscribe first
        let _ = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy1", "BINANCE");

        // Process tick
        let tick = make_tick(100.0, 10.0, 0);
        data_engine.process_tick(&tick);

        // Verify cache
        let cached = data_engine.get_tick("BTCUSDT.BINANCE");
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert!((cached.last_price - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_data_engine_bar_caching() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        // Subscribe to 1m bars
        let _ = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "strategy1", "BINANCE");

        // Feed ticks across minute boundary
        let tick1 = make_tick(100.0, 10.0, 0);
        let tick2 = make_tick(101.0, 20.0, 0);
        let tick3 = make_tick(102.0, 30.0, 1); // Next minute

        data_engine.process_tick(&tick1);
        data_engine.process_tick(&tick2);
        data_engine.process_tick(&tick3); // Should emit 1m bar

        // Verify 1m bar cache
        let cached = data_engine.get_bar("BTCUSDT.BINANCE", Interval::Minute);
        assert!(cached.is_some());
        let bar = cached.unwrap();
        assert!((bar.close_price - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_data_engine_get_subscriptions() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        let _ = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "s1", "BINANCE");
        let _ = data_engine.subscribe("ETHUSDT.BINANCE", Interval::Minute5, "s2", "BINANCE");

        let subs = data_engine.get_subscriptions();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains_key("BTCUSDT.BINANCE.Minute"));
        assert!(subs.contains_key("ETHUSDT.BINANCE.Minute5"));
    }

    #[test]
    fn test_data_engine_generate_all() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        let _ = data_engine.subscribe("BTCUSDT.BINANCE", Interval::Minute, "s1", "BINANCE");

        // Feed a tick
        let tick = make_tick(100.0, 10.0, 0);
        data_engine.process_tick(&tick);

        // Force generate
        let bars = data_engine.generate_all();
        assert_eq!(bars.len(), 1);
        assert!((bars[0].close_price - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_data_engine_unsubscribe_nonexistent() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let main_engine = Arc::new(MainEngine::new());
        let data_engine = DataEngine::new(main_engine, tx);

        let result = data_engine.unsubscribe("NONEXISTENT.BINANCE", Interval::Minute, "s1");
        assert!(result.is_err());
    }
}
