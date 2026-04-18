//! Stop order engine for managing conditional orders (stop-loss, take-profit, trailing-stop).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::constant::{Direction, Exchange, Offset, OrderType};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{BarData, OrderRequest, TickData};

/// Type of stop order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StopOrderType {
    StopMarket,
    StopLimit,
    TrailingStopPct,
    TrailingStopAbs,
    TakeProfit,
}

impl std::fmt::Display for StopOrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopOrderType::StopMarket => write!(f, "StopMarket"),
            StopOrderType::StopLimit => write!(f, "StopLimit"),
            StopOrderType::TrailingStopPct => write!(f, "TrailingStopPct"),
            StopOrderType::TrailingStopAbs => write!(f, "TrailingStopAbs"),
            StopOrderType::TakeProfit => write!(f, "TakeProfit"),
        }
    }
}

/// Status of a stop order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StopOrderStatus {
    Pending,
    Triggered,
    Cancelled,
    Expired,
}

impl std::fmt::Display for StopOrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopOrderStatus::Pending => write!(f, "Pending"),
            StopOrderStatus::Triggered => write!(f, "Triggered"),
            StopOrderStatus::Cancelled => write!(f, "Cancelled"),
            StopOrderStatus::Expired => write!(f, "Expired"),
        }
    }
}

/// Unique stop order ID
pub type StopOrderId = u64;

/// A stop order with all state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopOrder {
    pub id: StopOrderId,
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub stop_type: StopOrderType,
    pub stop_price: f64,
    pub limit_price: f64,
    pub volume: f64,
    pub offset: Offset,
    pub status: StopOrderStatus,
    pub trail_pct: f64,
    pub trail_abs: f64,
    pub highest_price: f64,
    pub lowest_price: f64,
    pub gateway_name: String,
    pub reference: String,
    pub created_at: DateTime<Utc>,
    pub triggered_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub tag: String,
}

impl StopOrder {
    pub fn is_active(&self) -> bool {
        self.status == StopOrderStatus::Pending
    }

    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }
}

/// Request to create a new stop order
#[derive(Debug, Clone)]
pub struct StopOrderRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub stop_type: StopOrderType,
    pub stop_price: f64,
    pub limit_price: f64,
    pub volume: f64,
    pub offset: Offset,
    pub trail_pct: f64,
    pub trail_abs: f64,
    pub gateway_name: String,
    pub reference: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub tag: String,
}

impl StopOrderRequest {
    pub fn stop_market(
        symbol: &str, exchange: Exchange, direction: Direction,
        stop_price: f64, volume: f64, gateway_name: &str,
    ) -> Self {
        Self {
            symbol: symbol.to_string(), exchange, direction,
            stop_type: StopOrderType::StopMarket, stop_price,
            limit_price: 0.0, volume, offset: Offset::None,
            trail_pct: 0.0, trail_abs: 0.0,
            gateway_name: gateway_name.to_string(), reference: String::new(),
            expires_at: None, tag: String::new(),
        }
    }

    pub fn take_profit(
        symbol: &str, exchange: Exchange, direction: Direction,
        target_price: f64, volume: f64, gateway_name: &str,
    ) -> Self {
        Self {
            symbol: symbol.to_string(), exchange, direction,
            stop_type: StopOrderType::TakeProfit, stop_price: target_price,
            limit_price: 0.0, volume, offset: Offset::None,
            trail_pct: 0.0, trail_abs: 0.0,
            gateway_name: gateway_name.to_string(), reference: String::new(),
            expires_at: None, tag: String::new(),
        }
    }

    pub fn trailing_stop_pct(
        symbol: &str, exchange: Exchange, direction: Direction,
        trail_pct: f64, volume: f64, gateway_name: &str,
    ) -> Self {
        Self {
            symbol: symbol.to_string(), exchange, direction,
            stop_type: StopOrderType::TrailingStopPct, stop_price: 0.0,
            limit_price: 0.0, volume, offset: Offset::None,
            trail_pct, trail_abs: 0.0,
            gateway_name: gateway_name.to_string(), reference: String::new(),
            expires_at: None, tag: String::new(),
        }
    }

    pub fn trailing_stop_abs(
        symbol: &str, exchange: Exchange, direction: Direction,
        trail_abs: f64, volume: f64, gateway_name: &str,
    ) -> Self {
        Self {
            symbol: symbol.to_string(), exchange, direction,
            stop_type: StopOrderType::TrailingStopAbs, stop_price: 0.0,
            limit_price: 0.0, volume, offset: Offset::None,
            trail_pct: 0.0, trail_abs,
            gateway_name: gateway_name.to_string(), reference: String::new(),
            expires_at: None, tag: String::new(),
        }
    }
}

/// Callback for when a stop order is triggered
pub type StopOrderCallback = Box<dyn Fn(&StopOrder, OrderRequest) + Send + Sync>;

/// Stop order engine — manages conditional orders triggered by market data.
pub struct StopOrderEngine {
    name: String,
    stop_orders: RwLock<HashMap<StopOrderId, StopOrder>>,
    symbol_index: RwLock<HashMap<String, Vec<StopOrderId>>>,
    next_id: AtomicU64,
    running: AtomicBool,
    callbacks: Arc<RwLock<Vec<StopOrderCallback>>>,
}

impl StopOrderEngine {
    pub fn new() -> Self {
        Self {
            name: "StopOrderEngine".to_string(),
            stop_orders: RwLock::new(HashMap::new()),
            symbol_index: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            callbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn register_callback(&self, callback: StopOrderCallback) {
        let mut callbacks = self.callbacks.write().unwrap_or_else(|e| e.into_inner());
        callbacks.push(callback);
    }

    pub fn add_stop_order(&self, req: StopOrderRequest) -> Result<StopOrderId, String> {
        if req.volume <= 0.0 {
            return Err("Stop order volume must be positive".to_string());
        }
        if req.stop_type == StopOrderType::StopLimit && req.limit_price <= 0.0 {
            return Err("StopLimit requires a valid limit_price".to_string());
        }
        if req.stop_type == StopOrderType::TrailingStopPct && (req.trail_pct <= 0.0 || req.trail_pct >= 1.0) {
            return Err("TrailingStopPct requires trail_pct in (0, 1)".to_string());
        }
        if req.stop_type == StopOrderType::TrailingStopAbs && req.trail_abs <= 0.0 {
            return Err("TrailingStopAbs requires positive trail_abs".to_string());
        }
        if req.stop_type == StopOrderType::StopMarket && req.stop_price <= 0.0 {
            return Err("StopMarket requires positive stop_price".to_string());
        }
        if req.stop_type == StopOrderType::TakeProfit && req.stop_price <= 0.0 {
            return Err("TakeProfit requires positive stop_price".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());

        let order = StopOrder {
            id, symbol: req.symbol.clone(), exchange: req.exchange,
            direction: req.direction, stop_type: req.stop_type,
            stop_price: req.stop_price, limit_price: req.limit_price,
            volume: req.volume, offset: req.offset,
            status: StopOrderStatus::Pending,
            trail_pct: req.trail_pct, trail_abs: req.trail_abs,
            highest_price: 0.0, lowest_price: f64::MAX,
            gateway_name: req.gateway_name.clone(),
            reference: req.reference.clone(),
            created_at: Utc::now(), triggered_at: None,
            expires_at: req.expires_at, tag: req.tag.clone(),
        };

        {
            let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
            orders.insert(id, order);
        }
        {
            let mut index = self.symbol_index.write().unwrap_or_else(|e| e.into_inner());
            index.entry(vt_symbol.clone()).or_default().push(id);
        }

        info!("[StopOrderEngine] Added {} stop order #{} for {} @ {}", req.stop_type, id, vt_symbol, req.stop_price);
        Ok(id)
    }

    pub fn cancel_stop_order(&self, id: StopOrderId) -> Result<(), String> {
        let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
        if let Some(order) = orders.get_mut(&id) {
            if order.status != StopOrderStatus::Pending {
                return Err(format!("Stop order #{} is not pending (status: {})", id, order.status));
            }
            order.status = StopOrderStatus::Cancelled;
            info!("[StopOrderEngine] Cancelled stop order #{}", id);
            Ok(())
        } else {
            Err(format!("Stop order #{} not found", id))
        }
    }

    pub fn cancel_orders_for_symbol(&self, symbol: &str, exchange: Exchange) -> usize {
        let vt_symbol = format!("{}.{}", symbol, exchange.value());
        let ids: Vec<StopOrderId> = {
            let index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            index.get(&vt_symbol).cloned().unwrap_or_default()
        };
        let mut cancelled = 0;
        let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
        for id in ids {
            if let Some(order) = orders.get_mut(&id) {
                if order.status == StopOrderStatus::Pending {
                    order.status = StopOrderStatus::Cancelled;
                    cancelled += 1;
                }
            }
        }
        if cancelled > 0 {
            info!("[StopOrderEngine] Cancelled {} stop orders for {}", cancelled, vt_symbol);
        }
        cancelled
    }

    pub fn get_stop_order(&self, id: StopOrderId) -> Option<StopOrder> {
        self.stop_orders.read().unwrap_or_else(|e| e.into_inner()).get(&id).cloned()
    }

    pub fn get_all_stop_orders(&self) -> Vec<StopOrder> {
        self.stop_orders.read().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    pub fn get_active_stop_orders(&self) -> Vec<StopOrder> {
        self.stop_orders.read().unwrap_or_else(|e| e.into_inner()).values().filter(|o| o.is_active()).cloned().collect()
    }

    pub fn get_stop_orders_for_symbol(&self, symbol: &str, exchange: Exchange) -> Vec<StopOrder> {
        let vt_symbol = format!("{}.{}", symbol, exchange.value());
        let index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
        let ids = index.get(&vt_symbol).cloned().unwrap_or_default();
        let orders = self.stop_orders.read().unwrap_or_else(|e| e.into_inner());
        ids.iter().filter_map(|id| orders.get(id).cloned()).collect()
    }

    fn check_trigger(&self, order: &StopOrder, price: f64, high: f64, low: f64) -> bool {
        match order.stop_type {
            StopOrderType::StopMarket | StopOrderType::StopLimit => {
                match order.direction {
                    Direction::Short => price <= order.stop_price,
                    Direction::Long => price >= order.stop_price,
                    Direction::Net => price <= order.stop_price,
                }
            }
            StopOrderType::TakeProfit => {
                match order.direction {
                    Direction::Short => price >= order.stop_price,
                    Direction::Long => price <= order.stop_price,
                    Direction::Net => price >= order.stop_price,
                }
            }
            StopOrderType::TrailingStopPct | StopOrderType::TrailingStopAbs => {
                if order.stop_price <= 0.0 { return false; }
                match order.direction {
                    Direction::Short => low <= order.stop_price,
                    Direction::Long => high >= order.stop_price,
                    Direction::Net => low <= order.stop_price,
                }
            }
        }
    }

    fn update_trailing_stop(&self, order: &mut StopOrder, high: f64, low: f64) {
        if high > order.highest_price { order.highest_price = high; }
        if low < order.lowest_price { order.lowest_price = low; }

        match order.stop_type {
            StopOrderType::TrailingStopPct => {
                match order.direction {
                    Direction::Short | Direction::Net => {
                        let new_stop = order.highest_price * (1.0 - order.trail_pct);
                        if new_stop > order.stop_price || order.stop_price == 0.0 {
                            order.stop_price = new_stop;
                        }
                    }
                    Direction::Long => {
                        let new_stop = order.lowest_price * (1.0 + order.trail_pct);
                        if new_stop < order.stop_price || order.stop_price == 0.0 {
                            order.stop_price = new_stop;
                        }
                    }
                }
            }
            StopOrderType::TrailingStopAbs => {
                match order.direction {
                    Direction::Short | Direction::Net => {
                        let new_stop = order.highest_price - order.trail_abs;
                        if new_stop > order.stop_price || order.stop_price == 0.0 {
                            order.stop_price = new_stop;
                        }
                    }
                    Direction::Long => {
                        let new_stop = order.lowest_price + order.trail_abs;
                        if new_stop < order.stop_price || order.stop_price == 0.0 {
                            order.stop_price = new_stop;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn create_order_request(order: &StopOrder) -> OrderRequest {
        let order_type = match order.stop_type {
            StopOrderType::StopLimit => OrderType::Limit,
            _ => OrderType::Market,
        };
        let price = match order.stop_type {
            StopOrderType::StopLimit => order.limit_price,
            _ => 0.0,
        };
        OrderRequest {
            symbol: order.symbol.clone(), exchange: order.exchange,
            direction: order.direction, order_type, volume: order.volume,
            price, offset: order.offset,
            reference: if order.reference.is_empty() { format!("STOP_{}", order.id) } else { order.reference.clone() },
        }
    }

    fn process_tick_internal(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();
        let ids: Vec<StopOrderId> = {
            let index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            index.get(&vt_symbol).cloned().unwrap_or_default()
        };

        let mut triggered_ids: Vec<StopOrderId> = Vec::new();
        {
            let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
            for id in ids {
                if let Some(order) = orders.get_mut(&id) {
                    if !order.is_active() { continue; }
                    if order.is_expired() {
                        order.status = StopOrderStatus::Expired;
                        warn!("[StopOrderEngine] Stop order #{} expired", id);
                        continue;
                    }
                    self.update_trailing_stop(order, tick.last_price, tick.last_price);
                    if self.check_trigger(order, tick.last_price, tick.last_price, tick.last_price) {
                        order.status = StopOrderStatus::Triggered;
                        order.triggered_at = Some(Utc::now());
                        triggered_ids.push(id);
                        info!("[StopOrderEngine] Stop order #{} TRIGGERED at {}", id, tick.last_price);
                    }
                }
            }
        }

        if !triggered_ids.is_empty() {
            let orders = self.stop_orders.read().unwrap_or_else(|e| e.into_inner());
            let callbacks = self.callbacks.read().unwrap_or_else(|e| e.into_inner());
            for id in triggered_ids {
                if let Some(order) = orders.get(&id) {
                    let req = Self::create_order_request(order);
                    for callback in callbacks.iter() { callback(order, req.clone()); }
                }
            }
        }
    }

    fn process_bar_internal(&self, bar: &BarData) {
        let vt_symbol = bar.vt_symbol();
        let ids: Vec<StopOrderId> = {
            let index = self.symbol_index.read().unwrap_or_else(|e| e.into_inner());
            index.get(&vt_symbol).cloned().unwrap_or_default()
        };

        let mut triggered_ids: Vec<StopOrderId> = Vec::new();
        {
            let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
            for id in ids {
                if let Some(order) = orders.get_mut(&id) {
                    if !order.is_active() { continue; }
                    if order.is_expired() { order.status = StopOrderStatus::Expired; continue; }
                    self.update_trailing_stop(order, bar.high_price, bar.low_price);
                    if self.check_trigger(order, bar.close_price, bar.high_price, bar.low_price) {
                        order.status = StopOrderStatus::Triggered;
                        order.triggered_at = Some(Utc::now());
                        triggered_ids.push(id);
                        info!("[StopOrderEngine] Stop order #{} TRIGGERED (bar) stop={}", id, order.stop_price);
                    }
                }
            }
        }

        if !triggered_ids.is_empty() {
            let orders = self.stop_orders.read().unwrap_or_else(|e| e.into_inner());
            let callbacks = self.callbacks.read().unwrap_or_else(|e| e.into_inner());
            for id in triggered_ids {
                if let Some(order) = orders.get(&id) {
                    let req = Self::create_order_request(order);
                    for callback in callbacks.iter() { callback(order, req.clone()); }
                }
            }
        }
    }

    pub fn cleanup(&self) {
        let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
        let mut index = self.symbol_index.write().unwrap_or_else(|e| e.into_inner());
        let inactive: Vec<StopOrderId> = orders.iter()
            .filter(|(_, o)| !o.is_active()).map(|(id, _)| *id).collect();
        for id in &inactive {
            if let Some(order) = orders.remove(id) {
                if let Some(ids) = index.get_mut(&order.vt_symbol()) {
                    ids.retain(|i| i != id);
                    if ids.is_empty() { index.remove(&order.vt_symbol()); }
                }
            }
        }
        if !inactive.is_empty() {
            info!("[StopOrderEngine] Cleaned up {} inactive orders", inactive.len());
        }
    }
}

impl Default for StopOrderEngine {
    fn default() -> Self { Self::new() }
}

impl BaseEngine for StopOrderEngine {
    fn engine_name(&self) -> &str { &self.name }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
        let mut orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
        for (_, order) in orders.iter_mut() {
            if order.status == StopOrderStatus::Pending {
                order.status = StopOrderStatus::Cancelled;
            }
        }
        info!("[StopOrderEngine] Closed and cancelled all pending orders");
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Tick(tick) => self.process_tick_internal(tick),
            GatewayEvent::Bar(bar) => self.process_bar_internal(bar),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_order_engine_new() {
        let engine = StopOrderEngine::new();
        assert_eq!(engine.engine_name(), "StopOrderEngine");
        assert!(engine.get_all_stop_orders().is_empty());
    }

    #[test]
    fn test_add_stop_market_order() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.1, "BINANCE_SPOT");
        let result = engine.add_stop_order(req);
        assert!(result.is_ok());
        assert_eq!(engine.get_stop_order(result.unwrap()).unwrap().stop_type, StopOrderType::StopMarket);
    }

    #[test]
    fn test_add_stop_order_validation() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.0, "BINANCE_SPOT");
        assert!(engine.add_stop_order(req).is_err());
    }

    #[test]
    fn test_cancel_stop_order() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();
        assert!(engine.cancel_stop_order(id).is_ok());
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Cancelled);
        assert!(engine.cancel_stop_order(id).is_err());
        assert!(engine.cancel_stop_order(999).is_err());
    }

    #[test]
    fn test_trigger_sell_stop() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();

        let mut tick = TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, Utc::now());
        tick.last_price = 50000.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Pending);

        tick.last_price = 48500.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Triggered);
    }

    #[test]
    fn test_trigger_buy_stop() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Long, 51000.0, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();

        let mut tick = TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, Utc::now());
        tick.last_price = 50000.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Pending);

        tick.last_price = 51500.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Triggered);
    }

    #[test]
    fn test_take_profit() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::take_profit("BTCUSDT", Exchange::Binance, Direction::Short, 55000.0, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();

        let mut tick = TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, Utc::now());
        tick.last_price = 54000.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Pending);

        tick.last_price = 55000.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Triggered);
    }

    #[test]
    fn test_trailing_stop_pct() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::trailing_stop_pct("BTCUSDT", Exchange::Binance, Direction::Short, 0.02, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();

        let mut tick = TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, Utc::now());
        tick.last_price = 50000.0;
        engine.process_tick_internal(&tick);
        assert!((engine.get_stop_order(id).unwrap().stop_price - 49000.0).abs() < 0.01);

        tick.last_price = 52000.0;
        engine.process_tick_internal(&tick);
        assert!((engine.get_stop_order(id).unwrap().stop_price - 50960.0).abs() < 0.01);

        tick.last_price = 50900.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Triggered);
    }

    #[test]
    fn test_trailing_stop_abs() {
        let engine = StopOrderEngine::new();
        let req = StopOrderRequest::trailing_stop_abs("BTCUSDT", Exchange::Binance, Direction::Short, 1000.0, 0.1, "BINANCE_SPOT");
        let id = engine.add_stop_order(req).unwrap();

        let mut tick = TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, Utc::now());
        tick.last_price = 50000.0;
        engine.process_tick_internal(&tick);
        assert!((engine.get_stop_order(id).unwrap().stop_price - 49000.0).abs() < 0.01);

        tick.last_price = 52000.0;
        engine.process_tick_internal(&tick);
        assert!((engine.get_stop_order(id).unwrap().stop_price - 51000.0).abs() < 0.01);

        tick.last_price = 50500.0;
        engine.process_tick_internal(&tick);
        assert_eq!(engine.get_stop_order(id).unwrap().status, StopOrderStatus::Triggered);
    }

    #[test]
    fn test_cancel_orders_for_symbol() {
        let engine = StopOrderEngine::new();
        engine.add_stop_order(StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.1, "BINANCE_SPOT")).unwrap();
        engine.add_stop_order(StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 48000.0, 0.1, "BINANCE_SPOT")).unwrap();
        let eth_id = engine.add_stop_order(StopOrderRequest::stop_market("ETHUSDT", Exchange::Binance, Direction::Short, 3000.0, 1.0, "BINANCE_SPOT")).unwrap();

        let cancelled = engine.cancel_orders_for_symbol("BTCUSDT", Exchange::Binance);
        assert_eq!(cancelled, 2);
        assert_eq!(engine.get_stop_order(eth_id).unwrap().status, StopOrderStatus::Pending);
    }

    #[test]
    fn test_cleanup() {
        let engine = StopOrderEngine::new();
        let id = engine.add_stop_order(StopOrderRequest::stop_market("BTCUSDT", Exchange::Binance, Direction::Short, 49000.0, 0.1, "BINANCE_SPOT")).unwrap();
        engine.cancel_stop_order(id).unwrap();
        engine.cleanup();
        assert!(engine.get_stop_order(id).is_none());
    }
}
