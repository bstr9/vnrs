//! Order Emulator Engine - Local simulation of advanced order types not natively supported by exchanges.
//!
//! This module provides local emulation for:
//! - **Trailing Stop Orders** (percentage-based and absolute)
//! - **Stop-Limit Orders** (trigger price -> limit order)
//! - **Iceberg Orders** (hidden quantity sliced into visible portions)
//! - **MIT (Market-If-Touched)** orders
//! - **LIT (Limit-If-Touched)** orders
//!
//! Unlike StopOrderEngine which handles native exchange stop orders,
//! OrderEmulator handles order types that exchanges don't support directly.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::constant::{Direction, Exchange, Offset, OrderType, Status};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{BarData, CancelRequest, OrderData, OrderRequest, TickData};

/// Unique identifier for emulated orders
pub type EmulatedOrderId = u64;

/// Types of emulated orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmulatedOrderType {
    /// Trailing stop with percentage distance (long: stop moves up with price, triggers on drop)
    TrailingStopPct,
    /// Trailing stop with absolute price distance
    TrailingStopAbs,
    /// Stop-limit: trigger at stop_price, then submit limit at limit_price
    StopLimit,
    /// Iceberg: display only visible_volume, replenish as fills occur
    Iceberg,
    /// Market-If-Touched: trigger at price, submit market order
    Mit,
    /// Limit-If-Touched: trigger at price, submit limit order
    Lit,
}

impl std::fmt::Display for EmulatedOrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmulatedOrderType::TrailingStopPct => write!(f, "TrailingStopPct"),
            EmulatedOrderType::TrailingStopAbs => write!(f, "TrailingStopAbs"),
            EmulatedOrderType::StopLimit => write!(f, "StopLimit"),
            EmulatedOrderType::Iceberg => write!(f, "Iceberg"),
            EmulatedOrderType::Mit => write!(f, "MIT"),
            EmulatedOrderType::Lit => write!(f, "LIT"),
        }
    }
}

/// Status of an emulated order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmulatedOrderStatus {
    /// Order is active and waiting for trigger conditions
    Pending,
    /// Order has been triggered and real order submitted
    Triggered,
    /// Order has completed (all filled or cancelled)
    Completed,
    /// Order was cancelled before triggering
    Cancelled,
    /// Order expired before triggering
    Expired,
    /// Order was rejected (e.g., invalid parameters)
    Rejected,
}

impl std::fmt::Display for EmulatedOrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmulatedOrderStatus::Pending => write!(f, "Pending"),
            EmulatedOrderStatus::Triggered => write!(f, "Triggered"),
            EmulatedOrderStatus::Completed => write!(f, "Completed"),
            EmulatedOrderStatus::Cancelled => write!(f, "Cancelled"),
            EmulatedOrderStatus::Expired => write!(f, "Expired"),
            EmulatedOrderStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

/// An emulated order being tracked by the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulatedOrder {
    /// Unique identifier for this emulated order
    pub id: EmulatedOrderId,
    /// Type of emulated order
    pub order_type: EmulatedOrderType,
    /// Current status
    pub status: EmulatedOrderStatus,
    /// Symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Direction (Long or Short)
    pub direction: Direction,
    /// Offset (Open/Close/CloseToday/CloseYesterday)
    pub offset: Offset,
    /// Total volume of the order
    pub volume: f64,
    /// Remaining volume (for iceberg, this is hidden quantity)
    pub remaining_volume: f64,
    /// Trailing percentage (for TrailingStopPct)
    pub trail_pct: Option<f64>,
    /// Trailing absolute distance (for TrailingStopAbs)
    pub trail_abs: Option<f64>,
    /// Current stop price (computed for trailing stops)
    pub current_stop: Option<f64>,
    /// Highest price seen (for long trailing stops)
    pub highest_price: Option<f64>,
    /// Lowest price seen (for short trailing stops)
    pub lowest_price: Option<f64>,
    /// Trigger price (for StopLimit, MIT, LIT)
    pub trigger_price: Option<f64>,
    /// Limit price (for StopLimit, LIT)
    pub limit_price: Option<f64>,
    /// Visible volume per slice (for Iceberg)
    pub visible_volume: Option<f64>,
    /// Price for iceberg slices
    pub iceberg_price: Option<f64>,
    /// Real order ID from exchange (after trigger)
    pub real_order_id: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Expiration time (optional)
    pub expires_at: Option<DateTime<Utc>>,
    /// Gateway name to use for order submission
    pub gateway_name: String,
    /// Reference string from strategy
    pub reference: String,
}

impl EmulatedOrder {
    /// Check if the order is still active
    pub fn is_active(&self) -> bool {
        matches!(self.status, EmulatedOrderStatus::Pending | EmulatedOrderStatus::Triggered)
    }

    /// Get the vt_symbol for this order
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Check if the order has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
}

/// Request to create a new emulated order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulatedOrderRequest {
    /// Type of emulated order
    pub order_type: EmulatedOrderType,
    /// Symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Direction
    pub direction: Direction,
    /// Offset
    pub offset: Offset,
    /// Total volume
    pub volume: f64,
    /// Trailing percentage (TrailingStopPct only)
    pub trail_pct: Option<f64>,
    /// Trailing absolute distance (TrailingStopAbs only)
    pub trail_abs: Option<f64>,
    /// Trigger price (StopLimit, MIT, LIT)
    pub trigger_price: Option<f64>,
    /// Limit price (StopLimit, LIT)
    pub limit_price: Option<f64>,
    /// Visible volume per slice (Iceberg only)
    pub visible_volume: Option<f64>,
    /// Price for iceberg slices (Iceberg only)
    pub iceberg_price: Option<f64>,
    /// Expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Gateway name
    pub gateway_name: String,
    /// Reference
    pub reference: String,
}

impl EmulatedOrderRequest {
    /// Create a trailing stop order with percentage distance
    pub fn trailing_stop_pct(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, trail_pct: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::TrailingStopPct,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: Some(trail_pct),
            trail_abs: None,
            trigger_price: None,
            limit_price: None,
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }

    /// Create a trailing stop order with absolute price distance
    pub fn trailing_stop_abs(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, trail_abs: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::TrailingStopAbs,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: Some(trail_abs),
            trigger_price: None,
            limit_price: None,
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }

    /// Create a stop-limit order
    pub fn stop_limit(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, trigger_price: f64, limit_price: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::StopLimit,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: Some(trigger_price),
            limit_price: Some(limit_price),
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }

    /// Create a market-if-touched order
    pub fn market_if_touched(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, trigger_price: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::Mit,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: Some(trigger_price),
            limit_price: None,
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }

    /// Create a limit-if-touched order
    pub fn limit_if_touched(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, trigger_price: f64, limit_price: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::Lit,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: Some(trigger_price),
            limit_price: Some(limit_price),
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }

    /// Create an iceberg order
    pub fn iceberg(
        symbol: &str, exchange: Exchange, direction: Direction, offset: Offset,
        volume: f64, visible_volume: f64, price: f64, gateway_name: &str,
    ) -> Self {
        Self {
            order_type: EmulatedOrderType::Iceberg,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: None,
            limit_price: None,
            visible_volume: Some(visible_volume),
            iceberg_price: Some(price),
            expires_at: None,
            gateway_name: gateway_name.to_string(),
            reference: String::new(),
        }
    }
}

/// Callback type for sending real orders to the exchange
pub type EmulatorSendOrderCallback = Box<dyn Fn(&OrderRequest) -> Result<String, String> + Send + Sync>;

/// Callback type for cancelling real orders on the exchange
pub type EmulatorCancelOrderCallback = Box<dyn Fn(&CancelRequest) -> Result<(), String> + Send + Sync>;

/// Order Emulator Engine - locally simulates advanced order types
///
/// This engine tracks emulated orders and monitors market data to determine
/// when trigger conditions are met. When triggered, it submits real orders
/// via the provided callbacks.
pub struct OrderEmulator {
    /// Engine name
    name: String,
    /// All emulated orders indexed by ID
    orders: RwLock<HashMap<EmulatedOrderId, EmulatedOrder>>,
    /// Symbol-based index for fast tick/bar lookup
    symbol_index: RwLock<HashMap<String, Vec<EmulatedOrderId>>>,
    /// Reverse index from real order ID to emulated order ID
    real_order_index: RwLock<HashMap<String, EmulatedOrderId>>,
    /// Next order ID
    next_id: AtomicU64,
    /// Whether the engine is running
    running: AtomicBool,
    /// Callback for sending orders
    send_callback: RwLock<Option<EmulatorSendOrderCallback>>,
    /// Callback for cancelling orders
    cancel_callback: RwLock<Option<EmulatorCancelOrderCallback>>,
}

impl OrderEmulator {
    /// Create a new OrderEmulator engine
    pub fn new() -> Self {
        Self {
            name: "OrderEmulator".to_string(),
            orders: RwLock::new(HashMap::new()),
            symbol_index: RwLock::new(HashMap::new()),
            real_order_index: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(true),
            send_callback: RwLock::new(None),
            cancel_callback: RwLock::new(None),
        }
    }

    /// Set the callback for sending real orders
    pub fn set_send_order_callback(&self, callback: EmulatorSendOrderCallback) {
        let mut cb = self.send_callback.write().unwrap_or_else(|e| e.into_inner());
        *cb = Some(callback);
    }

    /// Set the callback for cancelling real orders
    pub fn set_cancel_order_callback(&self, callback: EmulatorCancelOrderCallback) {
        let mut cb = self.cancel_callback.write().unwrap_or_else(|e| e.into_inner());
        *cb = Some(callback);
    }

    /// Add a new emulated order
    ///
    /// Returns the assigned EmulatedOrderId on success, or an error message on failure.
    pub fn add_order(&self, req: &EmulatedOrderRequest) -> Result<EmulatedOrderId, String> {
        // Validate parameters based on order type
        match req.order_type {
            EmulatedOrderType::TrailingStopPct => {
                if req.trail_pct.is_none() || req.trail_pct.unwrap_or(0.0) <= 0.0 {
                    return Err("追踪止损百分比必须大于0".to_string());
                }
            }
            EmulatedOrderType::TrailingStopAbs => {
                if req.trail_abs.is_none() || req.trail_abs.unwrap_or(0.0) <= 0.0 {
                    return Err("追踪止损绝对距离必须大于0".to_string());
                }
            }
            EmulatedOrderType::StopLimit => {
                if req.trigger_price.is_none() || req.limit_price.is_none() {
                    return Err("止损限价单必须指定触发价和限价".to_string());
                }
            }
            EmulatedOrderType::Mit => {
                if req.trigger_price.is_none() {
                    return Err("触价单必须指定触发价".to_string());
                }
            }
            EmulatedOrderType::Lit => {
                if req.trigger_price.is_none() || req.limit_price.is_none() {
                    return Err("触价限价单必须指定触发价和限价".to_string());
                }
            }
            EmulatedOrderType::Iceberg => {
                if req.visible_volume.is_none() || req.iceberg_price.is_none() {
                    return Err("冰山单必须指定可见数量和价格".to_string());
                }
                if req.visible_volume.unwrap_or(0.0) <= 0.0 {
                    return Err("冰山单可见数量必须大于0".to_string());
                }
            }
        }

        if req.volume <= 0.0 {
            return Err("委托数量必须大于0".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());

        let order = EmulatedOrder {
            id,
            order_type: req.order_type,
            status: EmulatedOrderStatus::Pending,
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.direction,
            offset: req.offset,
            volume: req.volume,
            remaining_volume: req.volume,
            trail_pct: req.trail_pct,
            trail_abs: req.trail_abs,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            trigger_price: req.trigger_price,
            limit_price: req.limit_price,
            visible_volume: req.visible_volume,
            iceberg_price: req.iceberg_price,
            real_order_id: None,
            created_at: Utc::now(),
            expires_at: req.expires_at,
            gateway_name: req.gateway_name.clone(),
            reference: req.reference.clone(),
        };

        // For iceberg orders, immediately submit the first visible slice
        if req.order_type == EmulatedOrderType::Iceberg {
            if let Err(e) = self.submit_iceberg_slice(&order) {
                return Err(format!("冰山单首笔提交失败: {}", e));
            }
        }

        // Insert into orders map
        {
            let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());
            orders.insert(id, order);
        }

        // Insert into symbol index
        {
            let mut symbol_index = self.symbol_index.write().unwrap_or_else(|e| e.into_inner());
            symbol_index.entry(vt_symbol).or_default().push(id);
        }

        info!(
            "模拟委托已添加: id={}, 类型={}, 方向={:?}, 合约={}.{}, 数量={}",
            id, req.order_type, req.direction, req.symbol, req.exchange.value(), req.volume
        );

        Ok(id)
    }

    /// Cancel an emulated order by ID
    pub fn cancel_order(&self, id: EmulatedOrderId) -> Result<(), String> {
        let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());

        let order = orders.get_mut(&id).ok_or_else(|| format!("找不到模拟委托: {}", id))?;

        if !order.is_active() {
            return Err(format!("模拟委托{}不是活跃状态，无法撤销", id));
        }

        // If there's a real order on the exchange, cancel it
        if let Some(ref real_id) = order.real_order_id {
            let cancel_req = CancelRequest {
                orderid: real_id.clone(),
                symbol: order.symbol.clone(),
                exchange: order.exchange,
            };
            let cb = self.cancel_callback.read().unwrap_or_else(|e| e.into_inner());
            if let Some(ref cancel_fn) = *cb {
                if let Err(e) = cancel_fn(&cancel_req) {
                    warn!("撤销真实委托失败: {}", e);
                }
            }

            // Remove from real_order_index
            let mut real_index = self.real_order_index.write().unwrap_or_else(|e| e.into_inner());
            real_index.remove(real_id);
        }

        order.status = EmulatedOrderStatus::Cancelled;
        info!("模拟委托已撤销: id={}", id);
        Ok(())
    }

    /// Cancel all emulated orders for a specific symbol
    pub fn cancel_orders_for_symbol(&self, vt_symbol: &str) {
        let ids_to_cancel: Vec<EmulatedOrderId> = {
            let symbol_index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            symbol_index.get(vt_symbol).cloned().unwrap_or_default()
        };

        for id in ids_to_cancel {
            if let Err(e) = self.cancel_order(id) {
                warn!("撤销模拟委托{}失败: {}", id, e);
            }
        }
    }

    /// Get an emulated order by ID
    pub fn get_order(&self, id: EmulatedOrderId) -> Option<EmulatedOrder> {
        let orders = self.orders.read().unwrap_or_else(|e| e.into_inner());
        orders.get(&id).cloned()
    }

    /// Get all emulated orders
    pub fn get_all_orders(&self) -> Vec<EmulatedOrder> {
        let orders = self.orders.read().unwrap_or_else(|e| e.into_inner());
        orders.values().cloned().collect()
    }

    /// Get all active emulated orders
    pub fn get_active_orders(&self) -> Vec<EmulatedOrder> {
        let orders = self.orders.read().unwrap_or_else(|e| e.into_inner());
        orders.values().filter(|o| o.is_active()).cloned().collect()
    }

    /// Get emulated orders for a specific symbol
    pub fn get_orders_for_symbol(&self, vt_symbol: &str) -> Vec<EmulatedOrder> {
        let ids: Vec<EmulatedOrderId> = {
            let symbol_index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            symbol_index.get(vt_symbol).cloned().unwrap_or_default()
        };

        let orders = self.orders.read().unwrap_or_else(|e| e.into_inner());
        ids.iter().filter_map(|id| orders.get(id).cloned()).collect()
    }

    /// Remove completed/cancelled/expired orders from tracking
    pub fn cleanup(&self) {
        let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());
        let mut symbol_index = self.symbol_index.write().unwrap_or_else(|e| e.into_inner());

        let inactive_ids: Vec<EmulatedOrderId> = orders.iter()
            .filter(|(_, o)| !o.is_active())
            .map(|(id, _)| *id)
            .collect();

        for id in &inactive_ids {
            if let Some(order) = orders.remove(id) {
                let vt_symbol = order.vt_symbol();
                if let Some(ids) = symbol_index.get_mut(&vt_symbol) {
                    ids.retain(|i| i != id);
                    if ids.is_empty() {
                        symbol_index.remove(&vt_symbol);
                    }
                }
            }
        }

        if !inactive_ids.is_empty() {
            info!("清理了{}个非活跃模拟委托", inactive_ids.len());
        }
    }

    // ========================================================================
    // Private methods
    // ========================================================================

    /// Process a tick update — check triggers for all orders on this symbol
    fn process_tick_internal(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();

        let ids: Vec<EmulatedOrderId> = {
            let symbol_index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            symbol_index.get(&vt_symbol).cloned().unwrap_or_default()
        };

        for id in ids {
            let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());
            if let Some(order) = orders.get_mut(&id) {
                if !order.is_active() {
                    continue;
                }

                // Check expiration
                if order.is_expired() {
                    order.status = EmulatedOrderStatus::Expired;
                    info!("模拟委托已过期: id={}", id);
                    continue;
                }

                // Update trailing stop levels
                self.update_trailing(order, tick.last_price);

                // Check trigger conditions
                if self.check_trigger(order, tick.last_price) {
                    self.trigger_order(order);
                }
            }
        }
    }

    /// Process a bar update — check triggers for all orders on this symbol
    fn process_bar_internal(&self, bar: &BarData) {
        let vt_symbol = bar.vt_symbol();

        let ids: Vec<EmulatedOrderId> = {
            let symbol_index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            symbol_index.get(&vt_symbol).cloned().unwrap_or_default()
        };

        for id in ids {
            let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());
            if let Some(order) = orders.get_mut(&id) {
                if !order.is_active() {
                    continue;
                }

                if order.is_expired() {
                    order.status = EmulatedOrderStatus::Expired;
                    info!("模拟委托已过期: id={}", id);
                    continue;
                }

                // Use high/low for bar-based trigger checking
                self.update_trailing(order, bar.high_price);
                self.update_trailing(order, bar.low_price);

                // Check against high and low to see if trigger was hit within the bar
                let triggered = match order.direction {
                    Direction::Long => {
                        // For long orders: check high for upward triggers, low for downward triggers
                        match order.order_type {
                            EmulatedOrderType::TrailingStopPct |
                            EmulatedOrderType::TrailingStopAbs => {
                                // Update highest with bar high
                                if order.highest_price.is_none() || bar.high_price > order.highest_price.unwrap_or(0.0) {
                                    order.highest_price = Some(bar.high_price);
                                    self.update_trailing(order, bar.high_price);
                                }
                                // Check if low hit the stop
                                if let Some(stop) = order.current_stop {
                                    bar.low_price <= stop
                                } else {
                                    false
                                }
                            }
                            EmulatedOrderType::StopLimit => {
                                bar.high_price >= order.trigger_price.unwrap_or(f64::MAX)
                            }
                            EmulatedOrderType::Mit | EmulatedOrderType::Lit => {
                                bar.low_price <= order.trigger_price.unwrap_or(f64::MAX)
                            }
                            EmulatedOrderType::Iceberg => false, // Iceberg doesn't trigger on bars
                        }
                    }
                    Direction::Short => {
                        match order.order_type {
                            EmulatedOrderType::TrailingStopPct |
                            EmulatedOrderType::TrailingStopAbs => {
                                if order.lowest_price.is_none() || bar.low_price < order.lowest_price.unwrap_or(f64::MAX) {
                                    order.lowest_price = Some(bar.low_price);
                                    self.update_trailing(order, bar.low_price);
                                }
                                if let Some(stop) = order.current_stop {
                                    bar.high_price >= stop
                                } else {
                                    false
                                }
                            }
                            EmulatedOrderType::StopLimit => {
                                bar.low_price <= order.trigger_price.unwrap_or(0.0)
                            }
                            EmulatedOrderType::Mit | EmulatedOrderType::Lit => {
                                bar.high_price >= order.trigger_price.unwrap_or(0.0)
                            }
                            EmulatedOrderType::Iceberg => false,
                        }
                    }
                    Direction::Net => false, // Net position not supported for emulated orders
                };

                if triggered {
                    self.trigger_order(order);
                }
            }
        }
    }

    /// Process a real order update from the exchange (for iceberg slice tracking)
    fn process_order_update(&self, order_data: &OrderData) {
        let vt_orderid = order_data.vt_orderid();

        // Check if this is a real order we're tracking
        let emulated_id = {
            let real_index = self.real_order_index.read().unwrap_or_else(|e| e.into_inner());
            real_index.get(&vt_orderid).cloned()
        };

        if let Some(emulated_id) = emulated_id {
            let mut orders = self.orders.write().unwrap_or_else(|e| e.into_inner());
            if let Some(emulated_order) = orders.get_mut(&emulated_id) {
                match order_data.status {
                    Status::AllTraded => {
                        // For iceberg orders, check if there's more to submit
                        if emulated_order.order_type == EmulatedOrderType::Iceberg {
                            emulated_order.remaining_volume -= order_data.traded;
                            if emulated_order.remaining_volume > 0.0 {
                                // Submit next iceberg slice
                                if let Err(e) = self.submit_iceberg_slice(emulated_order) {
                                    warn!("冰山单下一笔提交失败: {}", e);
                                }
                            } else {
                                emulated_order.status = EmulatedOrderStatus::Completed;
                                info!("冰山单已全部成交: id={}", emulated_id);
                            }
                        } else {
                            // Non-iceberg: order completed
                            emulated_order.status = EmulatedOrderStatus::Completed;
                            info!("模拟委托已完成(全部成交): id={}", emulated_id);
                        }

                        // Clean up real order index
                        let mut real_index = self.real_order_index.write().unwrap_or_else(|e| e.into_inner());
                        real_index.remove(&vt_orderid);
                        emulated_order.real_order_id = None;
                    }
                    Status::Cancelled => {
                        if emulated_order.order_type == EmulatedOrderType::Iceberg
                            && emulated_order.remaining_volume > 0.0 {
                            emulated_order.status = EmulatedOrderStatus::Completed;
                            info!("冰山单已撤销(剩余不再提交): id={}", emulated_id);
                        } else {
                            emulated_order.status = EmulatedOrderStatus::Completed;
                            info!("模拟委托已完成(已撤销): id={}", emulated_id);
                        }

                        let mut real_index = self.real_order_index.write().unwrap_or_else(|e| e.into_inner());
                        real_index.remove(&vt_orderid);
                        emulated_order.real_order_id = None;
                    }
                    Status::Rejected => {
                        emulated_order.status = EmulatedOrderStatus::Rejected;
                        warn!("模拟委托被拒绝: id={}, 真实委托={}", emulated_id, vt_orderid);

                        let mut real_index = self.real_order_index.write().unwrap_or_else(|e| e.into_inner());
                        real_index.remove(&vt_orderid);
                        emulated_order.real_order_id = None;
                    }
                    _ => {
                        // Partially filled or other status — update remaining for iceberg
                        if emulated_order.order_type == EmulatedOrderType::Iceberg {
                            emulated_order.remaining_volume -= order_data.traded;
                        }
                    }
                }
            }
        }
    }

    /// Update trailing stop level based on current price
    fn update_trailing(&self, order: &mut EmulatedOrder, price: f64) {
        match order.order_type {
            EmulatedOrderType::TrailingStopPct => {
                let trail_pct = order.trail_pct.unwrap_or(0.0);
                match order.direction {
                    Direction::Long => {
                        // Ratchet: only move stop upward
                        if order.highest_price.is_none() || price > order.highest_price.unwrap_or(0.0) {
                            order.highest_price = Some(price);
                            let new_stop = price * (1.0 - trail_pct / 100.0);
                            if order.current_stop.is_none() || new_stop > order.current_stop.unwrap_or(0.0) {
                                order.current_stop = Some(new_stop);
                            }
                        }
                    }
                    Direction::Short => {
                        // Ratchet: only move stop downward
                        if order.lowest_price.is_none() || price < order.lowest_price.unwrap_or(f64::MAX) {
                            order.lowest_price = Some(price);
                            let new_stop = price * (1.0 + trail_pct / 100.0);
                            if order.current_stop.is_none() || new_stop < order.current_stop.unwrap_or(f64::MAX) {
                                order.current_stop = Some(new_stop);
                            }
                        }
                    }
                    Direction::Net => {} // Net not supported for trailing stops
                }
            }
            EmulatedOrderType::TrailingStopAbs => {
                let trail_abs = order.trail_abs.unwrap_or(0.0);
                match order.direction {
                    Direction::Long => {
                        if order.highest_price.is_none() || price > order.highest_price.unwrap_or(0.0) {
                            order.highest_price = Some(price);
                            let new_stop = price - trail_abs;
                            if order.current_stop.is_none() || new_stop > order.current_stop.unwrap_or(0.0) {
                                order.current_stop = Some(new_stop);
                            }
                        }
                    }
                    Direction::Short => {
                        if order.lowest_price.is_none() || price < order.lowest_price.unwrap_or(f64::MAX) {
                            order.lowest_price = Some(price);
                            let new_stop = price + trail_abs;
                            if order.current_stop.is_none() || new_stop < order.current_stop.unwrap_or(f64::MAX) {
                                order.current_stop = Some(new_stop);
                            }
                        }
                    }
                    Direction::Net => {} // Net not supported for trailing stops
                }
            }
            _ => {}
        }
    }

    /// Check if trigger conditions are met for an order
    fn check_trigger(&self, order: &EmulatedOrder, price: f64) -> bool {
        match order.order_type {
            EmulatedOrderType::TrailingStopPct | EmulatedOrderType::TrailingStopAbs => {
                if let Some(stop) = order.current_stop {
                    match order.direction {
                        Direction::Long => price <= stop,
                        Direction::Short => price >= stop,
                        Direction::Net => false,
                    }
                } else {
                    false
                }
            }
            EmulatedOrderType::StopLimit => {
                let trigger = order.trigger_price.unwrap_or(f64::NAN);
                match order.direction {
                    Direction::Long => price >= trigger,
                    Direction::Short => price <= trigger,
                    Direction::Net => false,
                }
            }
            EmulatedOrderType::Mit | EmulatedOrderType::Lit => {
                let trigger = order.trigger_price.unwrap_or(f64::NAN);
                match order.direction {
                    Direction::Long => price <= trigger,
                    Direction::Short => price >= trigger,
                    Direction::Net => false,
                }
            }
            EmulatedOrderType::Iceberg => false, // Iceberg doesn't trigger on price
        }
    }

    /// Trigger an emulated order — submit a real order to the exchange
    fn trigger_order(&self, order: &mut EmulatedOrder) {
        let order_req = match self.create_order_request(order) {
            Some(req) => req,
            None => {
                warn!("无法创建真实委托: id={}", order.id);
                order.status = EmulatedOrderStatus::Rejected;
                return;
            }
        };

        let cb = self.send_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref send_fn) = *cb {
            match send_fn(&order_req) {
                Ok(real_orderid) => {
                    order.status = EmulatedOrderStatus::Triggered;
                    order.real_order_id = Some(real_orderid.clone());
                    info!(
                        "模拟委托已触发: id={}, 类型={}, 真实委托={}",
                        order.id, order.order_type, real_orderid
                    );

                    // Update real order index
                    let mut real_index = self.real_order_index.write().unwrap_or_else(|e| e.into_inner());
                    real_index.insert(real_orderid, order.id);
                }
                Err(e) => {
                    order.status = EmulatedOrderStatus::Rejected;
                    warn!("模拟委托触发后下单失败: id={}, 错误={}", order.id, e);
                }
            }
        } else {
            warn!("模拟委托触发但未设置下单回调: id={}", order.id);
            order.status = EmulatedOrderStatus::Rejected;
        }
    }

    /// Submit a visible slice of an iceberg order
    fn submit_iceberg_slice(&self, order: &EmulatedOrder) -> Result<(), String> {
        let visible_vol = order.visible_volume.unwrap_or(0.0);
        let price = order.iceberg_price.unwrap_or(0.0);
        let slice_volume = visible_vol.min(order.remaining_volume);

        if slice_volume <= 0.0 {
            return Ok(());
        }

        let order_req = OrderRequest {
            symbol: order.symbol.clone(),
            exchange: order.exchange,
            direction: order.direction,
            order_type: OrderType::Limit,
            volume: slice_volume,
            price,
            offset: order.offset,
            reference: format!("iceberg_{}", order.id),
        };

        let cb = self.send_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref send_fn) = *cb {
            match send_fn(&order_req) {
                Ok(real_orderid) => {
                    info!(
                        "冰山单可见切片已提交: id={}, 切片数量={}, 真实委托={}",
                        order.id, slice_volume, real_orderid
                    );
                    Ok(())
                }
                Err(e) => Err(format!("冰山单下单失败: {}", e)),
            }
        } else {
            Err("未设置下单回调".to_string())
        }
    }

    /// Create an OrderRequest from an emulated order
    fn create_order_request(&self, order: &EmulatedOrder) -> Option<OrderRequest> {
        let (order_type, price) = match order.order_type {
            EmulatedOrderType::TrailingStopPct |
            EmulatedOrderType::TrailingStopAbs |
            EmulatedOrderType::Mit => (OrderType::Market, 0.0),
            EmulatedOrderType::StopLimit |
            EmulatedOrderType::Lit => (OrderType::Limit, order.limit_price.unwrap_or(0.0)),
            EmulatedOrderType::Iceberg => (OrderType::Limit, order.iceberg_price.unwrap_or(0.0)),
        };

        Some(OrderRequest {
            symbol: order.symbol.clone(),
            exchange: order.exchange,
            direction: order.direction,
            order_type: order_type,
            volume: order.volume,
            price,
            offset: order.offset,
            reference: format!("emulator_{}", order.id),
        })
    }
}

impl Default for OrderEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for OrderEmulator {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);

        // Cancel all active emulated orders
        let active_ids: Vec<EmulatedOrderId> = {
            let orders = self.orders.read().unwrap_or_else(|e| e.into_inner());
            orders.values()
                .filter(|o| o.is_active())
                .map(|o| o.id)
                .collect()
        };

        for id in active_ids {
            if let Err(e) = self.cancel_order(id) {
                warn!("关闭时撤销模拟委托{}失败: {}", id, e);
            }
        }

        info!("订单模拟引擎已关闭");
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        match (event_type, event) {
            ("tick", GatewayEvent::Tick(tick)) => {
                self.process_tick_internal(tick);
            }
            ("bar", GatewayEvent::Bar(bar)) => {
                self.process_bar_internal(bar);
            }
            ("order", GatewayEvent::Order(order)) => {
                self.process_order_update(order);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::Exchange;

    #[test]
    fn test_order_emulator_new() {
        let emulator = OrderEmulator::new();
        assert_eq!(emulator.engine_name(), "OrderEmulator");
        assert!(emulator.get_all_orders().is_empty());
        assert!(emulator.get_active_orders().is_empty());
    }

    #[test]
    fn test_emulated_order_type_display() {
        assert_eq!(format!("{}", EmulatedOrderType::TrailingStopPct), "TrailingStopPct");
        assert_eq!(format!("{}", EmulatedOrderType::TrailingStopAbs), "TrailingStopAbs");
        assert_eq!(format!("{}", EmulatedOrderType::StopLimit), "StopLimit");
        assert_eq!(format!("{}", EmulatedOrderType::Iceberg), "Iceberg");
        assert_eq!(format!("{}", EmulatedOrderType::Mit), "MIT");
        assert_eq!(format!("{}", EmulatedOrderType::Lit), "LIT");
    }

    #[test]
    fn test_emulated_order_status_display() {
        assert_eq!(format!("{}", EmulatedOrderStatus::Pending), "Pending");
        assert_eq!(format!("{}", EmulatedOrderStatus::Triggered), "Triggered");
        assert_eq!(format!("{}", EmulatedOrderStatus::Completed), "Completed");
        assert_eq!(format!("{}", EmulatedOrderStatus::Cancelled), "Cancelled");
        assert_eq!(format!("{}", EmulatedOrderStatus::Expired), "Expired");
        assert_eq!(format!("{}", EmulatedOrderStatus::Rejected), "Rejected");
    }

    #[test]
    fn test_trailing_stop_pct_request() {
        let req = EmulatedOrderRequest::trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            1.0, 5.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::TrailingStopPct);
        assert_eq!(req.trail_pct, Some(5.0));
        assert!(req.trail_abs.is_none());
    }

    #[test]
    fn test_trailing_stop_abs_request() {
        let req = EmulatedOrderRequest::trailing_stop_abs(
            "ETHUSDT", Exchange::Binance, Direction::Short, Offset::Open,
            2.0, 100.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::TrailingStopAbs);
        assert_eq!(req.trail_abs, Some(100.0));
        assert!(req.trail_pct.is_none());
    }

    #[test]
    fn test_stop_limit_request() {
        let req = EmulatedOrderRequest::stop_limit(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            1.0, 50000.0, 49500.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::StopLimit);
        assert_eq!(req.trigger_price, Some(50000.0));
        assert_eq!(req.limit_price, Some(49500.0));
    }

    #[test]
    fn test_mit_request() {
        let req = EmulatedOrderRequest::market_if_touched(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            1.0, 40000.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::Mit);
        assert_eq!(req.trigger_price, Some(40000.0));
        assert!(req.limit_price.is_none());
    }

    #[test]
    fn test_lit_request() {
        let req = EmulatedOrderRequest::limit_if_touched(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            1.0, 40000.0, 39500.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::Lit);
        assert_eq!(req.trigger_price, Some(40000.0));
        assert_eq!(req.limit_price, Some(39500.0));
    }

    #[test]
    fn test_iceberg_request() {
        let req = EmulatedOrderRequest::iceberg(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            10.0, 1.0, 50000.0, "binance"
        );
        assert_eq!(req.order_type, EmulatedOrderType::Iceberg);
        assert_eq!(req.visible_volume, Some(1.0));
        assert_eq!(req.iceberg_price, Some(50000.0));
    }

    #[test]
    fn test_add_order_validation() {
        let emulator = OrderEmulator::new();

        // Test trailing stop pct validation
        let req = EmulatedOrderRequest::trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            1.0, 0.0, "binance" // Invalid: 0% trail
        );
        assert!(emulator.add_order(&req).is_err());

        // Test stop-limit validation
        let req = EmulatedOrderRequest {
            order_type: EmulatedOrderType::StopLimit,
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            offset: Offset::Open,
            volume: 1.0,
            trail_pct: None,
            trail_abs: None,
            trigger_price: None, // Missing trigger
            limit_price: Some(50000.0),
            visible_volume: None,
            iceberg_price: None,
            expires_at: None,
            gateway_name: "binance".to_string(),
            reference: String::new(),
        };
        assert!(emulator.add_order(&req).is_err());

        // Test volume validation
        let req = EmulatedOrderRequest::trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long, Offset::Open,
            0.0, 5.0, "binance" // Invalid: 0 volume
        );
        assert!(emulator.add_order(&req).is_err());
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let emulator = OrderEmulator::new();
        let result = emulator.cancel_order(99999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_active_orders_empty() {
        let emulator = OrderEmulator::new();
        let active = emulator.get_active_orders();
        assert!(active.is_empty());
    }

    #[test]
    fn test_cleanup_no_active() {
        let emulator = OrderEmulator::new();
        emulator.cleanup(); // Should not panic
        assert!(emulator.get_all_orders().is_empty());
    }

    #[test]
    fn test_emulated_order_is_active() {
        let order = EmulatedOrder {
            id: 1,
            order_type: EmulatedOrderType::TrailingStopPct,
            status: EmulatedOrderStatus::Pending,
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            offset: Offset::Open,
            volume: 1.0,
            remaining_volume: 1.0,
            trail_pct: Some(5.0),
            trail_abs: None,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            trigger_price: None,
            limit_price: None,
            visible_volume: None,
            iceberg_price: None,
            real_order_id: None,
            created_at: Utc::now(),
            expires_at: None,
            gateway_name: "binance".to_string(),
            reference: String::new(),
        };
        assert!(order.is_active());
    }

    #[test]
    fn test_trigger_logic_trailing_stop_long() {
        let emulator = OrderEmulator::new();
        let mut order = EmulatedOrder {
            id: 1,
            order_type: EmulatedOrderType::TrailingStopPct,
            status: EmulatedOrderStatus::Pending,
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            offset: Offset::Open,
            volume: 1.0,
            remaining_volume: 1.0,
            trail_pct: Some(5.0),
            trail_abs: None,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            trigger_price: None,
            limit_price: None,
            visible_volume: None,
            iceberg_price: None,
            real_order_id: None,
            created_at: Utc::now(),
            expires_at: None,
            gateway_name: "binance".to_string(),
            reference: String::new(),
        };

        // Update with price 100, should set stop at 95
        emulator.update_trailing(&mut order, 100.0);
        assert_eq!(order.highest_price, Some(100.0));
        assert_eq!(order.current_stop, Some(95.0));

        // Update with higher price 110, stop should move to 104.5
        emulator.update_trailing(&mut order, 110.0);
        assert_eq!(order.highest_price, Some(110.0));
        assert_eq!(order.current_stop, Some(104.5));

        // Update with lower price 105, stop should stay at 104.5 (ratchet)
        emulator.update_trailing(&mut order, 105.0);
        assert_eq!(order.highest_price, Some(110.0));
        assert_eq!(order.current_stop, Some(104.5));
    }

    #[test]
    fn test_process_unrelated_event() {
        let emulator = OrderEmulator::new();

        // Process an order event that's not tracked — should not panic
        let order = OrderData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "order123".to_string(),
        );
        emulator.process_event("order", &GatewayEvent::Order(order));
    }
}
