//! Algorithmic execution engine for large order splitting (TWAP, VWAP).
//!
//! Provides execution algorithms that split large orders into smaller child orders
//! to minimize market impact:
//!
//! - **TWAP (Time-Weighted Average Price)**: Slices order evenly over time
//! - **VWAP (Volume-Weighted Average Price)**: Sizes slices by volume profile

use std::collections::HashMap;
use std::sync::{Arc, RwLock, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::constant::{Direction, Exchange, OrderType};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{OrderData, OrderRequest, TradeData};

/// Unique identifier for an algo order
pub type AlgoId = u64;

/// Algo order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlgoStatus {
    /// Algo order is waiting to start
    Pending,
    /// Algo order is actively executing child orders
    Running,
    /// Algo order completed successfully (all child orders filled)
    Completed,
    /// Algo order was cancelled before completion
    Cancelled,
    /// Algo order failed (e.g., child order rejected)
    Failed,
}

impl std::fmt::Display for AlgoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgoStatus::Pending => write!(f, "Pending"),
            AlgoStatus::Running => write!(f, "Running"),
            AlgoStatus::Completed => write!(f, "Completed"),
            AlgoStatus::Cancelled => write!(f, "Cancelled"),
            AlgoStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Algorithm type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlgoType {
    /// Time-Weighted Average Price
    Twap,
    /// Volume-Weighted Average Price (simplified with uniform volume profile)
    Vwap,
}

impl std::fmt::Display for AlgoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgoType::Twap => write!(f, "TWAP"),
            AlgoType::Vwap => write!(f, "VWAP"),
        }
    }
}

/// Algo order state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgoOrderState {
    /// Algo ID
    pub algo_id: AlgoId,
    /// Algorithm type
    pub algo_type: AlgoType,
    /// Symbol
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Direction (Long/Short)
    pub direction: Direction,
    /// Total volume to execute
    pub total_volume: f64,
    /// Total volume filled so far
    pub filled_volume: f64,
    /// Average fill price
    pub avg_price: f64,
    /// Number of child orders
    pub slice_count: usize,
    /// Number of child orders filled
    pub filled_count: usize,
    /// Current status
    pub status: AlgoStatus,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Start time (when first child order was sent)
    pub started_at: Option<DateTime<Utc>>,
    /// Completion time
    pub completed_at: Option<DateTime<Utc>>,
    /// Gateway name for execution
    pub gateway_name: String,
    /// User reference
    pub reference: String,
}

impl AlgoOrderState {
    /// Calculate progress percentage
    pub fn progress_pct(&self) -> f64 {
        if self.total_volume <= 0.0 {
            return 0.0;
        }
        (self.filled_volume / self.total_volume) * 100.0
    }

    /// Check if algo is still active
    pub fn is_active(&self) -> bool {
        matches!(self.status, AlgoStatus::Pending | AlgoStatus::Running)
    }
}

/// TWAP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwapConfig {
    /// Number of slices
    pub slice_count: usize,
    /// Interval between slices in seconds
    pub interval_secs: u64,
    /// Price limit (optional) - if set, won't execute above (buy) or below (sell)
    pub price_limit: Option<f64>,
    /// Order type for child orders
    pub order_type: OrderType,
    /// Price for limit orders (if None, uses market)
    pub limit_price: Option<f64>,
    /// Randomize slice timing +/- this percentage
    pub randomize_pct: f64,
}

impl Default for TwapConfig {
    fn default() -> Self {
        Self {
            slice_count: 10,
            interval_secs: 60,
            price_limit: None,
            order_type: OrderType::Market,
            limit_price: None,
            randomize_pct: 0.0,
        }
    }
}

/// VWAP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VwapConfig {
    /// Number of slices
    pub slice_count: usize,
    /// Interval between slices in seconds
    pub interval_secs: u64,
    /// Volume profile (relative weights for each slice, should sum to 1.0)
    /// If None, uses uniform distribution
    pub volume_profile: Option<Vec<f64>>,
    /// Order type for child orders
    pub order_type: OrderType,
    /// Price for limit orders
    pub limit_price: Option<f64>,
}

impl Default for VwapConfig {
    fn default() -> Self {
        Self {
            slice_count: 10,
            interval_secs: 60,
            volume_profile: None,
            order_type: OrderType::Market,
            limit_price: None,
        }
    }
}

/// Callback when algo order state changes
pub type AlgoCallback = Box<dyn Fn(&AlgoOrderState) + Send + Sync>;

/// Order executor trait - abstracts MainEngine's send_order
#[async_trait]
pub trait OrderExecutor: Send + Sync {
    /// Send a child order, returns vt_orderid or error
    async fn send_order(&self, req: OrderRequest, gateway_name: &str) -> Result<String, String>;
}

/// Algo Engine - manages algorithmic order execution
pub struct AlgoEngine {
    name: String,
    /// Active algo orders by algo_id
    algo_orders: Arc<RwLock<HashMap<AlgoId, AlgoOrderState>>>,
    /// Map from vt_orderid to algo_id for routing trade/order events
    orderid_to_algo: Arc<RwLock<HashMap<String, AlgoId>>>,
    /// Next algo ID
    next_algo_id: AtomicU64,
    /// Running flag
    running: AtomicBool,
    /// Callbacks for algo state changes
    callbacks: Arc<RwLock<Vec<AlgoCallback>>>,
    /// Order executor (set after MainEngine is available)
    executor: Arc<RwLock<Option<Arc<dyn OrderExecutor>>>>,
}

impl AlgoEngine {
    /// Create a new AlgoEngine
    pub fn new() -> Self {
        Self {
            name: "AlgoEngine".to_string(),
            algo_orders: Arc::new(RwLock::new(HashMap::new())),
            orderid_to_algo: Arc::new(RwLock::new(HashMap::new())),
            next_algo_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            callbacks: Arc::new(RwLock::new(Vec::new())),
            executor: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the order executor (called by MainEngine after initialization)
    pub fn set_executor(&self, executor: Arc<dyn OrderExecutor>) {
        let mut exec = self.executor.write().unwrap_or_else(|e| e.into_inner());
        *exec = Some(executor);
    }

    /// Register a callback for algo state changes
    pub fn register_callback(&self, callback: AlgoCallback) {
        let mut callbacks = self.callbacks.write().unwrap_or_else(|e| e.into_inner());
        callbacks.push(callback);
    }

    /// Get algo state by ID
    pub fn get_algo(&self, algo_id: AlgoId) -> Option<AlgoOrderState> {
        let orders = self.algo_orders.read().unwrap_or_else(|e| e.into_inner());
        orders.get(&algo_id).cloned()
    }

    /// Get all active algo orders
    pub fn get_active_algos(&self) -> Vec<AlgoOrderState> {
        let orders = self.algo_orders.read().unwrap_or_else(|e| e.into_inner());
        orders.values().filter(|a| a.is_active()).cloned().collect()
    }

    /// Get all algo orders
    pub fn get_all_algos(&self) -> Vec<AlgoOrderState> {
        let orders = self.algo_orders.read().unwrap_or_else(|e| e.into_inner());
        orders.values().cloned().collect()
    }

    /// Cancel an algo order
    pub fn cancel_algo(&self, algo_id: AlgoId) -> Result<(), String> {
        let mut orders = self.algo_orders.write().unwrap_or_else(|e| e.into_inner());
        if let Some(algo) = orders.get_mut(&algo_id) {
            if algo.is_active() {
                algo.status = AlgoStatus::Cancelled;
                algo.completed_at = Some(Utc::now());
                info!("[AlgoEngine] Algo {} cancelled", algo_id);
                self.notify_callbacks(algo);
            }
            Ok(())
        } else {
            Err(format!("Algo {} not found", algo_id))
        }
    }

    /// Start a TWAP algo order
    #[allow(clippy::too_many_arguments)]
    pub fn start_twap(
        &self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        total_volume: f64,
        config: TwapConfig,
        gateway_name: &str,
        reference: &str,
    ) -> Result<AlgoId, String> {
        if total_volume <= 0.0 {
            return Err("Total volume must be positive".to_string());
        }
        if config.slice_count == 0 {
            return Err("Slice count must be positive".to_string());
        }

        let algo_id = self.next_algo_id.fetch_add(1, Ordering::Relaxed);
        let state = AlgoOrderState {
            algo_id,
            algo_type: AlgoType::Twap,
            symbol: symbol.to_string(),
            exchange,
            direction,
            total_volume,
            filled_volume: 0.0,
            avg_price: 0.0,
            slice_count: config.slice_count,
            filled_count: 0,
            status: AlgoStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            gateway_name: gateway_name.to_string(),
            reference: reference.to_string(),
        };

        // Store state
        {
            let mut orders = self.algo_orders.write().unwrap_or_else(|e| e.into_inner());
            orders.insert(algo_id, state.clone());
        }

        info!(
            "[AlgoEngine] Starting TWAP algo {} for {} {} ({} slices, {}s interval)",
            algo_id, symbol, exchange.value(), config.slice_count, config.interval_secs
        );

        // Spawn TWAP execution task
        self.spawn_twap_task(state, config);

        Ok(algo_id)
    }

    /// Start a VWAP algo order
    #[allow(clippy::too_many_arguments)]
    pub fn start_vwap(
        &self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        total_volume: f64,
        config: VwapConfig,
        gateway_name: &str,
        reference: &str,
    ) -> Result<AlgoId, String> {
        if total_volume <= 0.0 {
            return Err("Total volume must be positive".to_string());
        }
        if config.slice_count == 0 {
            return Err("Slice count must be positive".to_string());
        }

        let algo_id = self.next_algo_id.fetch_add(1, Ordering::Relaxed);
        let state = AlgoOrderState {
            algo_id,
            algo_type: AlgoType::Vwap,
            symbol: symbol.to_string(),
            exchange,
            direction,
            total_volume,
            filled_volume: 0.0,
            avg_price: 0.0,
            slice_count: config.slice_count,
            filled_count: 0,
            status: AlgoStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            gateway_name: gateway_name.to_string(),
            reference: reference.to_string(),
        };

        // Store state
        {
            let mut orders = self.algo_orders.write().unwrap_or_else(|e| e.into_inner());
            orders.insert(algo_id, state.clone());
        }

        info!(
            "[AlgoEngine] Starting VWAP algo {} for {} {} ({} slices, {}s interval)",
            algo_id, symbol, exchange.value(), config.slice_count, config.interval_secs
        );

        // Spawn VWAP execution task
        self.spawn_vwap_task(state, config);

        Ok(algo_id)
    }

    fn spawn_twap_task(&self, state: AlgoOrderState, config: TwapConfig) {
        let algo_id = state.algo_id;
        let slice_volume = state.total_volume / config.slice_count as f64;
        let interval = Duration::from_secs(config.interval_secs);

        // Clone Arc references for the async task
        let self_orders = self.algo_orders.clone();
        let self_orderid_to_algo = self.orderid_to_algo.clone();
        let self_executor = self.executor.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await; // Skip first immediate tick

            for slice_idx in 0..config.slice_count {
                // Check if algo was cancelled
                {
                    let orders = self_orders.read().unwrap_or_else(|e| e.into_inner());
                    if let Some(algo) = orders.get(&algo_id) {
                        if algo.status == AlgoStatus::Cancelled {
                            info!("[AlgoEngine] TWAP algo {} cancelled, stopping execution", algo_id);
                            return;
                        }
                    } else {
                        return; // Algo was removed
                    }
                }

                // Update status to Running on first slice
                if slice_idx == 0 {
                    let mut orders = self_orders.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(algo) = orders.get_mut(&algo_id) {
                        algo.status = AlgoStatus::Running;
                        algo.started_at = Some(Utc::now());
                    }
                }

                // Send child order
                let req = OrderRequest {
                    symbol: state.symbol.clone(),
                    exchange: state.exchange,
                    direction: state.direction,
                    order_type: config.order_type,
                    offset: crate::trader::constant::Offset::None,
                    price: config.limit_price.unwrap_or(0.0),
                    volume: slice_volume,
                    reference: format!("TWAP_{}_{}", algo_id, slice_idx),
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: state.gateway_name.clone(),
                };

                // Clone executor Arc out of RwLock before await to satisfy Send bound
                let executor = {
                    let guard = self_executor.read().unwrap_or_else(|e| e.into_inner());
                    guard.clone()
                };

                if let Some(exec) = executor {
                    match exec.send_order(req.clone(), &state.gateway_name).await {
                        Ok(vt_orderid) => {
                            // Map vt_orderid to algo_id for trade routing
                            let mut mapping = self_orderid_to_algo.write().unwrap_or_else(|e| e.into_inner());
                            mapping.insert(vt_orderid, algo_id);
                            info!("[AlgoEngine] TWAP slice {}/{} sent: {}", slice_idx + 1, config.slice_count, req.volume);
                        }
                        Err(e) => {
                            warn!("[AlgoEngine] TWAP slice {}/{} failed: {}", slice_idx + 1, config.slice_count, e);
                            // Mark algo as failed
                            let mut orders = self_orders.write().unwrap_or_else(|e| e.into_inner());
                            if let Some(algo) = orders.get_mut(&algo_id) {
                                algo.status = AlgoStatus::Failed;
                                algo.completed_at = Some(Utc::now());
                            }
                            return;
                        }
                    }
                } else {
                    warn!("[AlgoEngine] No executor set, cannot send TWAP child order");
                    return;
                }

                // Wait for next interval (except after last slice)
                if slice_idx < config.slice_count - 1 {
                    interval_timer.tick().await;
                }
            }

            // Mark as completed (all slices sent)
            // Note: Actual fill tracking happens via process_event
            info!("[AlgoEngine] TWAP algo {} all {} slices sent", algo_id, config.slice_count);
        });
    }

    fn spawn_vwap_task(&self, state: AlgoOrderState, config: VwapConfig) {
        let algo_id = state.algo_id;
        let interval = Duration::from_secs(config.interval_secs);

        // Calculate volume profile
        let volumes: Vec<f64> = if let Some(ref profile) = config.volume_profile {
            let total_weight: f64 = profile.iter().sum();
            profile.iter().map(|w| state.total_volume * w / total_weight).collect()
        } else {
            // Uniform distribution
            let slice_volume = state.total_volume / config.slice_count as f64;
            vec![slice_volume; config.slice_count]
        };

        let self_orders = self.algo_orders.clone();
        let self_orderid_to_algo = self.orderid_to_algo.clone();
        let self_executor = self.executor.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            interval_timer.tick().await; // Skip first immediate tick

            for (slice_idx, &slice_volume) in volumes.iter().enumerate() {
                // Check if algo was cancelled
                {
                    let orders = self_orders.read().unwrap_or_else(|e| e.into_inner());
                    if let Some(algo) = orders.get(&algo_id) {
                        if algo.status == AlgoStatus::Cancelled {
                            info!("[AlgoEngine] VWAP algo {} cancelled, stopping execution", algo_id);
                            return;
                        }
                    } else {
                        return;
                    }
                }

                // Update status to Running on first slice
                if slice_idx == 0 {
                    let mut orders = self_orders.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(algo) = orders.get_mut(&algo_id) {
                        algo.status = AlgoStatus::Running;
                        algo.started_at = Some(Utc::now());
                    }
                }

                // Send child order
                let req = OrderRequest {
                    symbol: state.symbol.clone(),
                    exchange: state.exchange,
                    direction: state.direction,
                    order_type: config.order_type,
                    offset: crate::trader::constant::Offset::None,
                    price: config.limit_price.unwrap_or(0.0),
                    volume: slice_volume,
                    reference: format!("VWAP_{}_{}", algo_id, slice_idx),
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: state.gateway_name.clone(),
                };

                // Clone executor Arc out of RwLock before await to satisfy Send bound
                let executor = {
                    let guard = self_executor.read().unwrap_or_else(|e| e.into_inner());
                    guard.clone()
                };

                if let Some(exec) = executor {
                    match exec.send_order(req.clone(), &state.gateway_name).await {
                        Ok(vt_orderid) => {
                            let mut mapping = self_orderid_to_algo.write().unwrap_or_else(|e| e.into_inner());
                            mapping.insert(vt_orderid, algo_id);
                            info!("[AlgoEngine] VWAP slice {}/{} sent: vol={}", slice_idx + 1, config.slice_count, slice_volume);
                        }
                        Err(e) => {
                            warn!("[AlgoEngine] VWAP slice {}/{} failed: {}", slice_idx + 1, config.slice_count, e);
                            let mut orders = self_orders.write().unwrap_or_else(|e| e.into_inner());
                            if let Some(algo) = orders.get_mut(&algo_id) {
                                algo.status = AlgoStatus::Failed;
                                algo.completed_at = Some(Utc::now());
                            }
                            return;
                        }
                    }
                } else {
                    warn!("[AlgoEngine] No executor set, cannot send VWAP child order");
                    return;
                }

                if slice_idx < config.slice_count - 1 {
                    interval_timer.tick().await;
                }
            }

            info!("[AlgoEngine] VWAP algo {} all {} slices sent", algo_id, config.slice_count);
        });
    }

    /// Process a trade event to update algo fill status
    pub fn process_trade(&self, trade: &TradeData) {
        let mapping = self.orderid_to_algo.read().unwrap_or_else(|e| e.into_inner());
        if let Some(&algo_id) = mapping.get(&trade.orderid) {
            drop(mapping);
            
            let mut orders = self.algo_orders.write().unwrap_or_else(|e| e.into_inner());
            if let Some(algo) = orders.get_mut(&algo_id) {
                // Update fill stats
                let new_filled = algo.filled_volume + trade.volume;
                let new_avg_price = if new_filled > 0.0 {
                    (algo.avg_price * algo.filled_volume + trade.price * trade.volume) / new_filled
                } else {
                    0.0
                };
                algo.filled_volume = new_filled;
                algo.avg_price = new_avg_price;
                algo.filled_count += 1;

                info!(
                    "[AlgoEngine] Algo {} filled: +{} @ {} (total {}/{})",
                    algo_id, trade.volume, trade.price, algo.filled_volume, algo.total_volume
                );

                // Check if fully filled
                if algo.filled_volume >= algo.total_volume * 0.9999 {
                    algo.status = AlgoStatus::Completed;
                    algo.completed_at = Some(Utc::now());
                    info!(
                        "[AlgoEngine] Algo {} completed: avg_price={:.4}, total_vol={}",
                        algo_id, algo.avg_price, algo.filled_volume
                    );
                }

                self.notify_callbacks(algo);
            }
        }
    }

    /// Process an order event (for status tracking)
    pub fn process_order(&self, order: &OrderData) {
        let mapping = self.orderid_to_algo.read().unwrap_or_else(|e| e.into_inner());
        if let Some(&algo_id) = mapping.get(&order.orderid) {
            drop(mapping);

            // Check for rejected orders
            if order.status == crate::trader::constant::Status::Rejected {
                let mut orders = self.algo_orders.write().unwrap_or_else(|e| e.into_inner());
                if let Some(_algo) = orders.get_mut(&algo_id) {
                    warn!("[AlgoEngine] Algo {} child order rejected", algo_id);
                    // Don't fail the whole algo on single rejection, just log it
                }
            }
        }
    }

    fn notify_callbacks(&self, state: &AlgoOrderState) {
        let callbacks = self.callbacks.read().unwrap_or_else(|e| e.into_inner());
        for callback in callbacks.iter() {
            callback(state);
        }
    }
}

impl Default for AlgoEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for AlgoEngine {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Trade(trade) => self.process_trade(trade),
            GatewayEvent::Order(order) => self.process_order(order),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_algo_order_state_progress() {
        let state = AlgoOrderState {
            algo_id: 1,
            algo_type: AlgoType::Twap,
            symbol: "btcusdt".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            total_volume: 1.0,
            filled_volume: 0.5,
            avg_price: 50000.0,
            slice_count: 10,
            filled_count: 5,
            status: AlgoStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
            gateway_name: "BINANCE_SPOT".to_string(),
            reference: "test".to_string(),
        };

        assert!((state.progress_pct() - 50.0).abs() < 0.001);
        assert!(state.is_active());
    }

    #[test]
    fn test_twap_config_default() {
        let config = TwapConfig::default();
        assert_eq!(config.slice_count, 10);
        assert_eq!(config.interval_secs, 60);
        assert_eq!(config.order_type, OrderType::Market);
    }

    #[test]
    fn test_vwap_config_default() {
        let config = VwapConfig::default();
        assert_eq!(config.slice_count, 10);
        assert_eq!(config.interval_secs, 60);
        assert!(config.volume_profile.is_none());
    }

    #[test]
    fn test_algo_engine_new() {
        let engine = AlgoEngine::new();
        assert_eq!(engine.engine_name(), "AlgoEngine");
        assert!(engine.get_all_algos().is_empty());
    }

    #[test]
    fn test_algo_engine_start_twap_validation() {
        let engine = AlgoEngine::new();
        
        // Zero volume should fail
        let result = engine.start_twap(
            "btcusdt",
            Exchange::Binance,
            Direction::Long,
            0.0,
            TwapConfig::default(),
            "BINANCE_SPOT",
            "test",
        );
        assert!(result.is_err());

        // Zero slice count should fail
        let mut config = TwapConfig::default();
        config.slice_count = 0;
        let result = engine.start_twap(
            "btcusdt",
            Exchange::Binance,
            Direction::Long,
            1.0,
            config,
            "BINANCE_SPOT",
            "test",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_algo_engine_start_vwap_validation() {
        let engine = AlgoEngine::new();
        
        // Zero volume should fail
        let result = engine.start_vwap(
            "btcusdt",
            Exchange::Binance,
            Direction::Long,
            0.0,
            VwapConfig::default(),
            "BINANCE_SPOT",
            "test",
        );
        assert!(result.is_err());

        // Zero slice count should fail
        let mut config = VwapConfig::default();
        config.slice_count = 0;
        let result = engine.start_vwap(
            "btcusdt",
            Exchange::Binance,
            Direction::Long,
            1.0,
            config,
            "BINANCE_SPOT",
            "test",
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_algo_engine_cancel() {
        let engine = AlgoEngine::new();

        // Create a TWAP algo (it will start in background but won't complete immediately)
        let result = engine.start_twap(
            "btcusdt",
            Exchange::Binance,
            Direction::Long,
            0.1,
            TwapConfig::default(),
            "BINANCE_SPOT",
            "test",
        );
        assert!(result.is_ok());
        let algo_id = result.unwrap();

        // Cancel it
        let cancel_result = engine.cancel_algo(algo_id);
        assert!(cancel_result.is_ok());

        // Verify it's cancelled
        let state = engine.get_algo(algo_id);
        assert!(state.is_some());
        assert_eq!(state.unwrap().status, AlgoStatus::Cancelled);

        // Cancel non-existent algo should fail
        let cancel_result = engine.cancel_algo(999);
        assert!(cancel_result.is_err());
    }

    #[test]
    fn test_algo_status_display() {
        assert_eq!(format!("{}", AlgoStatus::Pending), "Pending");
        assert_eq!(format!("{}", AlgoStatus::Running), "Running");
        assert_eq!(format!("{}", AlgoStatus::Completed), "Completed");
        assert_eq!(format!("{}", AlgoStatus::Cancelled), "Cancelled");
        assert_eq!(format!("{}", AlgoStatus::Failed), "Failed");
    }

    #[test]
    fn test_algo_type_display() {
        assert_eq!(format!("{}", AlgoType::Twap), "TWAP");
        assert_eq!(format!("{}", AlgoType::Vwap), "VWAP");
    }
}
