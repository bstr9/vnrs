//! Paper Trading Engine
//!
//! A lightweight engine that intercepts strategy orders and performs local
//! fill matching against live market data, without sending orders to the
//! exchange. Uses the `FillModel` trait from the backtesting module for
//! realistic fill simulation.
//!
//! This enables the same `StrategyTemplate` strategy to run without
//! modification in Backtest, Paper, and Live modes.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;

use crate::backtesting::fill_model::{BestPriceFillModel, FillModel};
use crate::trader::{
    Direction, Exchange, Offset, OrderData, OrderRequest, OrderType, Status,
    TickData, TradeData, BarData,
};

/// Virtual position tracked by the paper engine
#[derive(Debug, Clone)]
pub struct PaperPosition {
    /// Position direction (Long = positive, Short = negative)
    pub direction: Direction,
    /// Total volume held
    pub volume: f64,
    /// Volume-weighted average entry price
    pub avg_price: f64,
}

impl PaperPosition {
    fn new(direction: Direction, volume: f64, avg_price: f64) -> Self {
        Self { direction, volume, avg_price }
    }

    /// Unrealized PnL given current market price
    pub fn unrealized_pnl(&self, market_price: f64) -> f64 {
        match self.direction {
            Direction::Long => (market_price - self.avg_price) * self.volume,
            Direction::Short => (self.avg_price - market_price) * self.volume,
            Direction::Net => 0.0,
        }
    }
}

/// A virtual order tracked by the paper engine
#[derive(Debug, Clone)]
pub struct PaperOrder {
    /// Unique virtual order ID
    pub vt_orderid: String,
    /// Symbol in vt_symbol format (e.g., "BTCUSDT.BINANCE")
    pub vt_symbol: String,
    /// Order direction
    pub direction: Direction,
    /// Offset type
    pub offset: Offset,
    /// Order type (Market, Limit, Stop)
    pub order_type: OrderType,
    /// Limit price (for limit/stop-limit orders)
    pub price: f64,
    /// Stop/trigger price (for stop orders)
    pub stop_price: f64,
    /// Order volume
    pub volume: f64,
    /// Filled volume so far
    pub traded: f64,
    /// Current status
    pub status: Status,
    /// Creation time
    pub created_at: chrono::DateTime<Utc>,
}

impl PaperOrder {
    /// Convert to OrderData for strategy callback
    pub fn to_order_data(&self) -> OrderData {
        OrderData {
            gateway_name: "PAPER".to_string(),
            symbol: self.vt_symbol.split('.').next().unwrap_or(&self.vt_symbol).to_string(),
            exchange: self.vt_symbol.split('.').nth(1)
                .and_then(|s| match s.to_uppercase().as_str() {
                    "BINANCE" => Some(Exchange::Binance),
                    "BINANCE_USDM" => Some(Exchange::BinanceUsdm),
                    "BINANCE_COINM" => Some(Exchange::BinanceCoinm),
                    "OKX" => Some(Exchange::Okx),
                    "BYBIT" => Some(Exchange::Bybit),
                    _ => None,
                })
                .unwrap_or(Exchange::Binance),
            orderid: self.vt_orderid.clone(),
            order_type: self.order_type,
            direction: Some(self.direction),
            offset: self.offset,
            price: self.price,
            volume: self.volume,
            traded: self.traded,
            status: self.status,
            datetime: Some(self.created_at),
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        }
    }
}

/// Paper trading engine — intercepts orders and does local matching
pub struct PaperTradingEngine {
    /// Fill model for simulating order fills
    fill_model: Box<dyn FillModel>,
    /// Virtual positions: vt_symbol -> PaperPosition
    virtual_positions: Arc<RwLock<HashMap<String, PaperPosition>>>,
    /// Pending orders: vt_orderid -> PaperOrder
    pending_orders: Arc<RwLock<HashMap<String, PaperOrder>>>,
    /// All completed trades
    trades: Arc<RwLock<Vec<TradeData>>>,
    /// Whether the engine is active
    active: Arc<RwLock<bool>>,
    /// Order ID counter
    order_counter: AtomicU64,
    /// Trade ID counter
    trade_counter: AtomicU64,
}

impl PaperTradingEngine {
    /// Create a new paper trading engine with the default `BestPriceFillModel`
    pub fn new() -> Self {
        Self {
            fill_model: Box::new(BestPriceFillModel::new(0.0)),
            virtual_positions: Arc::new(RwLock::new(HashMap::new())),
            pending_orders: Arc::new(RwLock::new(HashMap::new())),
            trades: Arc::new(RwLock::new(Vec::new())),
            active: Arc::new(RwLock::new(false)),
            order_counter: AtomicU64::new(1),
            trade_counter: AtomicU64::new(1),
        }
    }

    /// Create a paper trading engine with a custom fill model
    pub fn with_fill_model(fill_model: Box<dyn FillModel>) -> Self {
        Self {
            fill_model,
            virtual_positions: Arc::new(RwLock::new(HashMap::new())),
            pending_orders: Arc::new(RwLock::new(HashMap::new())),
            trades: Arc::new(RwLock::new(Vec::new())),
            active: Arc::new(RwLock::new(false)),
            order_counter: AtomicU64::new(1),
            trade_counter: AtomicU64::new(1),
        }
    }

    /// Activate paper trading mode
    pub fn start(&self) {
        *self.active.write().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        tracing::info!("PaperTradingEngine started");
    }

    /// Deactivate paper trading mode
    pub fn stop(&self) {
        *self.active.write().unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        tracing::info!("PaperTradingEngine stopped");
    }

    /// Check if the engine is active
    pub fn is_active(&self) -> bool {
        *self.active.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Submit a virtual order. Returns the virtual vt_orderid.
    ///
    /// Market orders are immediately matched against the latest market data
    /// if available. Limit and stop orders are queued for matching on
    /// subsequent tick/bar updates.
    pub fn submit_order(&self, req: OrderRequest) -> String {
        let order_id = self.order_counter.fetch_add(1, Ordering::Relaxed);
        let vt_orderid = format!("PAPER.{}", order_id);
        let vt_symbol = req.vt_symbol();

        let (order_type, price, stop_price) = match req.order_type {
            OrderType::Market => (OrderType::Market, req.price, 0.0),
            OrderType::Limit => (OrderType::Limit, req.price, 0.0),
            OrderType::Stop => (OrderType::Stop, 0.0, req.price),
            OrderType::StopLimit => (OrderType::StopLimit, req.price, req.price), // stop_price = price for stop-limit
            OrderType::Fak => (OrderType::Fak, req.price, 0.0),
            OrderType::Fok => (OrderType::Fok, req.price, 0.0),
            _ => (req.order_type, req.price, 0.0), // Default: treat as limit-like
        };

        let paper_order = PaperOrder {
            vt_orderid: vt_orderid.clone(),
            vt_symbol: vt_symbol.clone(),
            direction: req.direction,
            offset: req.offset,
            order_type,
            price,
            stop_price,
            volume: req.volume,
            traded: 0.0,
            status: Status::NotTraded,
            created_at: Utc::now(),
        };

        self.pending_orders
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_orderid.clone(), paper_order);

        tracing::debug!(
            "Paper order submitted: {} {:?} {} @ {} vol={}",
            vt_orderid, req.direction, vt_symbol, req.price, req.volume
        );

        vt_orderid
    }

    /// Cancel a virtual order
    pub fn cancel_order(&self, vt_orderid: &str) -> Result<(), String> {
        let mut pending = self.pending_orders.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(order) = pending.remove(vt_orderid) {
            tracing::debug!("Paper order cancelled: {}", vt_orderid);
            drop(pending);
            // Update virtual position if it was a partially filled order
            if order.traded > 0.0 {
                // Already partially filled — position was already updated on partial fills
                // No further action needed
            }
            Ok(())
        } else {
            Err(format!("Paper order {vt_orderid} not found in pending orders"))
        }
    }

    /// Process a tick update — check pending orders against current market data
    ///
    /// Returns a list of (vt_orderid, TradeData, OrderData) tuples for filled orders.
    /// The caller (StrategyEngine) is responsible for dispatching callbacks to strategies.
    pub fn process_tick(&self, tick: &TickData) -> Vec<(String, TradeData, OrderData)> {
        if !self.is_active() {
            return Vec::new();
        }

        let vt_symbol = tick.vt_symbol();
        let mut filled = Vec::new();

        // Collect order IDs that need processing
        let order_ids: Vec<String> = {
            let pending = self.pending_orders.read().unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.keys().cloned().collect()
        };

        for vt_orderid in order_ids {
            let mut pending = self.pending_orders.write().unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(order) = pending.get_mut(&vt_orderid) else {
                continue;
            };

            if order.vt_symbol != vt_symbol {
                continue;
            }

            // Build a temporary OrderData for the fill model
            let order_data = order.to_order_data();
            let fill_result = self.fill_model.simulate_tick_fill(&order_data, tick);

            if fill_result.filled {
                order.traded = fill_result.fill_qty;
                order.status = Status::AllTraded;

                // Generate trade
                let trade_id = self.trade_counter.fetch_add(1, Ordering::Relaxed);
                let trade = TradeData {
                    gateway_name: "PAPER".to_string(),
                    symbol: tick.symbol.clone(),
                    exchange: tick.exchange,
                    orderid: order.vt_orderid.clone(),
                    tradeid: format!("PAPER_TRADE.{}", trade_id),
                    direction: Some(order.direction),
                    offset: order.offset,
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,
                    datetime: Some(tick.datetime),
                    extra: None,
                };

                // Update virtual position
                self.update_position(&order.vt_symbol, order.direction, order.offset, fill_result.fill_qty, fill_result.fill_price);

                // Record trade
                self.trades.write().unwrap_or_else(std::sync::PoisonError::into_inner).push(trade.clone());

                // Build final order data for callback
                let final_order_data = order.to_order_data();

                // Remove from pending
                pending.remove(&vt_orderid);

                filled.push((vt_orderid, trade, final_order_data));
            }
        }

        filled
    }

    /// Process a bar update — check pending orders (especially limit/stop orders)
    ///
    /// Returns a list of (vt_orderid, TradeData, OrderData) tuples for filled orders.
    pub fn process_bar(&self, bar: &BarData) -> Vec<(String, TradeData, OrderData)> {
        if !self.is_active() {
            return Vec::new();
        }

        let vt_symbol = bar.vt_symbol();
        let mut filled = Vec::new();

        let order_ids: Vec<String> = {
            let pending = self.pending_orders.read().unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.keys().cloned().collect()
        };

        for vt_orderid in order_ids {
            let mut pending = self.pending_orders.write().unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(order) = pending.get_mut(&vt_orderid) else {
                continue;
            };

            if order.vt_symbol != vt_symbol {
                continue;
            }

            let order_data = order.to_order_data();

            let fill_result = match order.order_type {
                OrderType::Market => self.fill_model.simulate_market_fill(&order_data, bar),
                OrderType::Limit | OrderType::Fak | OrderType::Fok => {
                    self.fill_model.simulate_limit_fill(&order_data, bar)
                }
                OrderType::Stop | OrderType::StopLimit => {
                    self.fill_model.simulate_stop_fill(&order_data, bar, order.stop_price)
                }
                _ => {
                    // Treat unknown/rare order types as market orders
                    self.fill_model.simulate_market_fill(&order_data, bar)
                }
            };

            if fill_result.filled {
                order.traded = fill_result.fill_qty;
                order.status = Status::AllTraded;

                let trade_id = self.trade_counter.fetch_add(1, Ordering::Relaxed);
                let trade = TradeData {
                    gateway_name: "PAPER".to_string(),
                    symbol: bar.symbol.clone(),
                    exchange: bar.exchange,
                    orderid: order.vt_orderid.clone(),
                    tradeid: format!("PAPER_TRADE.{}", trade_id),
                    direction: Some(order.direction),
                    offset: order.offset,
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,
                    datetime: Some(bar.datetime),
                    extra: None,
                };

                self.update_position(&order.vt_symbol, order.direction, order.offset, fill_result.fill_qty, fill_result.fill_price);

                self.trades.write().unwrap_or_else(std::sync::PoisonError::into_inner).push(trade.clone());

                let final_order_data = order.to_order_data();

                pending.remove(&vt_orderid);

                filled.push((vt_orderid, trade, final_order_data));
            }
        }

        filled
    }

    /// Get the virtual position for a symbol
    pub fn get_position(&self, vt_symbol: &str) -> PaperPosition {
        let positions = self.virtual_positions.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        positions.get(vt_symbol).cloned().unwrap_or(PaperPosition {
            direction: Direction::Net,
            volume: 0.0,
            avg_price: 0.0,
        })
    }

    /// Get all virtual positions
    pub fn get_all_positions(&self) -> HashMap<String, PaperPosition> {
        self.virtual_positions.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Get all trades
    pub fn get_all_trades(&self) -> Vec<TradeData> {
        self.trades.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Get unrealized PnL for a symbol given the current market price
    pub fn get_unrealized_pnl(&self, vt_symbol: &str, market_price: f64) -> f64 {
        let positions = self.virtual_positions.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        positions.get(vt_symbol).map(|p| p.unrealized_pnl(market_price)).unwrap_or(0.0)
    }

    /// Get the number of pending orders
    pub fn pending_order_count(&self) -> usize {
        self.pending_orders.read().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Clear all virtual state (for reset)
    pub fn reset(&self) {
        self.virtual_positions.write().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        self.pending_orders.write().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        self.trades.write().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
    }

    /// Update virtual position after a fill
    fn update_position(
        &self,
        vt_symbol: &str,
        direction: Direction,
        offset: Offset,
        volume: f64,
        price: f64,
    ) {
        let mut positions = self.virtual_positions.write().unwrap_or_else(std::sync::PoisonError::into_inner);

        let current = positions.get(vt_symbol).cloned().unwrap_or(PaperPosition {
            direction: Direction::Net,
            volume: 0.0,
            avg_price: 0.0,
        });

        // Determine signed volume change
        let signed_change = match direction {
            Direction::Long => volume,
            Direction::Short => -volume,
            Direction::Net => 0.0,
        };

        // For close offsets, we reverse the sign (closing reduces position)
        let effective_change = if Self::is_close_offset(offset) {
            -signed_change.abs()
            // Close long: reduce positive position => effective_change = -volume
            // Close short: reduce negative position => effective_change = +volume
        } else {
            signed_change
        };

        let new_volume = match current.direction {
            Direction::Long => current.volume + effective_change,
            Direction::Short => current.volume - effective_change,
            Direction::Net => effective_change,
        };

        if new_volume <= 0.0 {
            // Flat or flipped
            if new_volume < 0.0 {
                // Position flipped to short
                positions.insert(vt_symbol.to_string(), PaperPosition::new(
                    Direction::Short,
                    new_volume.abs(),
                    price,
                ));
            } else {
                // Flat
                positions.remove(vt_symbol);
            }
        } else if current.volume == 0.0 || current.direction == Direction::Net {
            // Opening new position
            positions.insert(vt_symbol.to_string(), PaperPosition::new(
                direction,
                volume,
                price,
            ));
        } else {
            // Adding to existing position — recalculate average price
            let old_total = current.avg_price * current.volume;
            let add_total = price * volume;
            let new_avg = (old_total + add_total) / (current.volume + volume);
            positions.insert(vt_symbol.to_string(), PaperPosition::new(
                current.direction,
                new_volume,
                new_avg,
            ));
        }
    }

    /// Check if an offset represents a closing order
    fn is_close_offset(offset: Offset) -> bool {
        matches!(offset, Offset::Close | Offset::CloseYesterday)
    }
}

impl Default for PaperTradingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paper_position_unrealized_pnl_long() {
        let pos = PaperPosition::new(Direction::Long, 10.0, 100.0);
        let pnl = pos.unrealized_pnl(110.0);
        assert!((pnl - 100.0).abs() < 0.01); // (110 - 100) * 10
    }

    #[test]
    fn test_paper_position_unrealized_pnl_short() {
        let pos = PaperPosition::new(Direction::Short, 10.0, 100.0);
        let pnl = pos.unrealized_pnl(90.0);
        assert!((pnl - 100.0).abs() < 0.01); // (100 - 90) * 10
    }

    #[test]
    fn test_submit_order() {
        let engine = PaperTradingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );
        let vt_orderid = engine.submit_order(req);
        assert!(vt_orderid.starts_with("PAPER."));
        assert_eq!(engine.pending_order_count(), 1);
    }

    #[test]
    fn test_cancel_order() {
        let engine = PaperTradingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );
        let vt_orderid = engine.submit_order(req);
        assert!(engine.cancel_order(&vt_orderid).is_ok());
        assert_eq!(engine.pending_order_count(), 0);
    }

    #[test]
    fn test_start_stop() {
        let engine = PaperTradingEngine::new();
        assert!(!engine.is_active());
        engine.start();
        assert!(engine.is_active());
        engine.stop();
        assert!(!engine.is_active());
    }

    #[test]
    fn test_reset() {
        let engine = PaperTradingEngine::new();
        engine.start();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );
        engine.submit_order(req);
        assert_eq!(engine.pending_order_count(), 1);
        engine.reset();
        assert_eq!(engine.pending_order_count(), 0);
    }
}
