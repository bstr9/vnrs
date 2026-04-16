//! Enhanced Position Tracking
//!
//! Inspired by nautilus_trader's Position class
//! Tracks average entry price, realized PnL, and handles position flips correctly

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::trader::{Direction, Exchange, Offset, TradeData};

/// Position event for tracking fills and adjustments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PositionEvent {
    /// Fill event from trade
    Fill(TradeData),
    /// Commission adjustment
    Commission(f64),
    /// Funding/overnight adjustment
    Funding(f64),
}

/// Enhanced position tracking with average price and PnL calculation
///
/// This struct properly tracks:
/// - Average entry price (volume-weighted)
/// - Realized PnL from closed positions
/// - Position flips (long to short or vice versa)
/// - All fill events for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Unique position identifier
    pub position_id: String,

    /// Symbol (e.g., "BTCUSDT")
    pub symbol: String,

    /// Exchange
    pub exchange: Exchange,

    /// Position direction (Long or Short)
    /// None means flat/no position
    pub direction: Option<Direction>,

    /// Current signed quantity (positive for long, negative for short)
    signed_qty: f64,

    /// Absolute quantity
    quantity: f64,

    /// Peak quantity held during this position
    peak_qty: f64,

    /// Average open price (volume-weighted)
    avg_px_open: f64,

    /// Average close price (volume-weighted)
    avg_px_close: f64,

    /// Realized PnL from closed trades
    realized_pnl: f64,

    /// Total realized PnL including commissions
    realized_return: f64,

    /// Number of fills
    fill_count: u64,

    /// All fill events
    fills: VecDeque<TradeData>,

    /// Adjustment events (commissions, funding)
    adjustments: VecDeque<PositionEvent>,

    /// Timestamp when position opened
    ts_opened: Option<DateTime<Utc>>,

    /// Timestamp when position closed
    ts_closed: Option<DateTime<Utc>>,

    /// Contract size multiplier
    size_multiplier: f64,
}

impl Position {
    /// Create a new position
    pub fn new(position_id: String, symbol: String, exchange: Exchange) -> Self {
        Self {
            position_id,
            symbol,
            exchange,
            direction: None,
            signed_qty: 0.0,
            quantity: 0.0,
            peak_qty: 0.0,
            avg_px_open: 0.0,
            avg_px_close: 0.0,
            realized_pnl: 0.0,
            realized_return: 0.0,
            fill_count: 0,
            fills: VecDeque::new(),
            adjustments: VecDeque::new(),
            ts_opened: None,
            ts_closed: None,
            size_multiplier: 1.0,
        }
    }

    /// Create position with contract size
    pub fn with_size_multiplier(mut self, multiplier: f64) -> Self {
        self.size_multiplier = multiplier;
        self
    }

    /// Get signed quantity (positive = long, negative = short)
    pub fn signed_qty(&self) -> f64 {
        self.signed_qty
    }

    /// Get absolute quantity
    pub fn quantity(&self) -> f64 {
        self.quantity
    }

    /// Get average open price
    pub fn avg_px_open(&self) -> f64 {
        self.avg_px_open
    }

    /// Get average close price
    pub fn avg_px_close(&self) -> f64 {
        self.avg_px_close
    }

    /// Get realized PnL
    pub fn realized_pnl(&self) -> f64 {
        self.realized_pnl
    }

    /// Check if position is flat (no holdings)
    pub fn is_flat(&self) -> bool {
        self.signed_qty == 0.0
    }

    /// Check if position is long
    pub fn is_long(&self) -> bool {
        self.signed_qty > 0.0
    }

    /// Check if position is short
    pub fn is_short(&self) -> bool {
        self.signed_qty < 0.0
    }

    /// Check if position was opened (had any fills)
    pub fn is_opened(&self) -> bool {
        self.ts_opened.is_some()
    }

    /// Check if position is currently closed
    pub fn is_closed(&self) -> bool {
        self.ts_closed.is_some() && self.is_flat()
    }

    /// Calculate unrealized PnL at given price
    pub fn unrealized_pnl(&self, mark_price: f64) -> f64 {
        if self.is_flat() || self.avg_px_open == 0.0 {
            return 0.0;
        }

        let pnl = match self.direction {
            Some(Direction::Long) => (mark_price - self.avg_px_open) * self.quantity,
            Some(Direction::Short) => (self.avg_px_open - mark_price) * self.quantity,
            Some(Direction::Net) => 0.0, // Net direction has no unrealized PnL
            None => 0.0,
        };

        pnl * self.size_multiplier
    }

    /// Get position duration in nanoseconds
    pub fn duration_ns(&self) -> Option<i64> {
        match (self.ts_opened, self.ts_closed) {
            (Some(opened), Some(closed)) => Some((closed - opened).num_nanoseconds().unwrap_or(0)),
            (Some(opened), None) => Some((Utc::now() - opened).num_nanoseconds().unwrap_or(0)),
            _ => None,
        }
    }

    /// Apply a trade/fill to the position
    ///
    /// This handles:
    /// - Opening new positions
    /// - Adding to existing positions
    /// - Closing positions (realizing PnL)
    /// - Position flips (close + reopen in opposite direction)
    pub fn apply_fill(&mut self, trade: &TradeData) -> Result<(), String> {
        if trade.symbol != self.symbol || trade.exchange != self.exchange {
            return Err(format!(
                "Trade symbol/exchange mismatch: expected {}.{} got {}.{}",
                self.symbol, self.exchange, trade.symbol, trade.exchange
            ));
        }

        let trade_qty = trade.volume;
        let trade_price = trade.price;

        // Determine trade direction effect
        let delta_qty = match trade.direction {
            Some(Direction::Long) => {
                match trade.offset {
                    Offset::Open => trade_qty, // Open long: increase position
                    Offset::Close | Offset::CloseToday | Offset::CloseYesterday => -trade_qty, // Close long: decrease
                    Offset::None => trade_qty, // Default: treat as open
                }
            }
            Some(Direction::Short) => {
                match trade.offset {
                    Offset::Open => -trade_qty, // Open short: decrease position (negative)
                    Offset::Close | Offset::CloseToday | Offset::CloseYesterday => trade_qty, // Close short: increase
                    Offset::None => -trade_qty, // Default: treat as open
                }
            }
            Some(Direction::Net) => {
                return Err("Net direction not supported for trades".to_string())
            }
            None => return Err("Trade has no direction".to_string()),
        };

        let prev_signed_qty = self.signed_qty;
        let new_signed_qty = self.signed_qty + delta_qty;

        // Check for position flip
        let is_flip = (prev_signed_qty > 0.0 && new_signed_qty < 0.0)
            || (prev_signed_qty < 0.0 && new_signed_qty > 0.0);

        // Handle position flip
        if is_flip {
            // First close existing position at flip price
            self.realize_pnl_for_qty(trade_price, prev_signed_qty.abs())?;

            // Then open new position in opposite direction
            self.signed_qty = new_signed_qty;
            self.quantity = new_signed_qty.abs();
            self.direction = Some(if new_signed_qty > 0.0 {
                Direction::Long
            } else {
                Direction::Short
            });
            self.avg_px_open = trade_price;
            self.peak_qty = self.quantity;
            self.ts_opened = Some(trade.datetime.unwrap_or_else(Utc::now));
            self.ts_closed = None;

            tracing::debug!(
                "Position flip: {} from {} to {} at price {}",
                self.symbol,
                prev_signed_qty,
                new_signed_qty,
                trade_price
            );
        } else {
            // Normal position update
            match (prev_signed_qty, new_signed_qty) {
                // Opening new position
                (0.0, new_qty) if new_qty != 0.0 => {
                    self.direction = Some(if new_qty > 0.0 {
                        Direction::Long
                    } else {
                        Direction::Short
                    });
                    self.avg_px_open = trade_price;
                    self.ts_opened = Some(trade.datetime.unwrap_or_else(Utc::now));
                    self.ts_closed = None;
                }

                // Adding to position (same direction)
                (prev, new) if prev * new > 0.0 => {
                    // Update average entry price (volume-weighted)
                    let prev_value = prev.abs() * self.avg_px_open;
                    let new_value = delta_qty.abs() * trade_price;
                    let total_qty = new.abs();
                    self.avg_px_open = (prev_value + new_value) / total_qty;
                }

                // Reducing position (opposite direction)
                (prev, new) if prev * new <= 0.0 && new != 0.0 => {
                    // Realize PnL for the closed portion
                    let closed_qty = delta_qty.abs().min(prev.abs());
                    self.realize_pnl_for_qty(trade_price, closed_qty)?;
                }

                // Closing position completely
                (_, 0.0) => {
                    // Realize remaining PnL
                    if prev_signed_qty != 0.0 {
                        self.realize_pnl_for_qty(trade_price, prev_signed_qty.abs())?;
                    }
                    self.direction = None;
                    self.avg_px_open = 0.0;
                    self.ts_closed = Some(trade.datetime.unwrap_or_else(Utc::now));
                }

                _ => {}
            }

            self.signed_qty = new_signed_qty;
            self.quantity = new_signed_qty.abs();
            self.peak_qty = self.peak_qty.max(self.quantity);
        }

        // Record the fill
        self.fill_count += 1;
        self.fills.push_back(trade.clone());

        Ok(())
    }

    /// Realize PnL for a specific quantity at given price
    fn realize_pnl_for_qty(&mut self, close_price: f64, qty: f64) -> Result<(), String> {
        if self.avg_px_open == 0.0 || qty <= 0.0 {
            return Ok(());
        }

        let pnl = match self.direction {
            Some(Direction::Long) => (close_price - self.avg_px_open) * qty,
            Some(Direction::Short) => (self.avg_px_open - close_price) * qty,
            Some(Direction::Net) => return Err("Cannot realize PnL with Net direction".to_string()),
            None => return Err("Cannot realize PnL with no direction".to_string()),
        };

        self.realized_pnl += pnl * self.size_multiplier;

        // Update average close price (volume-weighted)
        let prev_close_value = self.avg_px_close * (self.quantity - qty);
        let new_close_value = close_price * qty;
        let total_closed = self.quantity;
        if total_closed > 0.0 {
            self.avg_px_close = (prev_close_value + new_close_value) / total_closed;
        }

        tracing::trace!(
            "Realized PnL: {} {} @ {} close @ {} = {}",
            self.symbol,
            qty,
            self.avg_px_open,
            close_price,
            pnl * self.size_multiplier
        );

        Ok(())
    }

    /// Apply an adjustment (commission, funding, etc.)
    pub fn apply_adjustment(&mut self, event: PositionEvent) {
        match &event {
            PositionEvent::Commission(amt) => {
                self.realized_return -= amt;
            }
            PositionEvent::Funding(amt) => {
                self.realized_pnl += amt;
            }
            _ => {}
        }
        self.adjustments.push_back(event);
    }

    /// Add commission
    pub fn add_commission(&mut self, commission: f64) {
        self.apply_adjustment(PositionEvent::Commission(commission));
    }

    /// Add funding/overnight cost
    pub fn add_funding(&mut self, funding: f64) {
        self.apply_adjustment(PositionEvent::Funding(funding));
    }

    /// Reset position state
    pub fn reset(&mut self) {
        self.direction = None;
        self.signed_qty = 0.0;
        self.quantity = 0.0;
        self.peak_qty = 0.0;
        self.avg_px_open = 0.0;
        self.avg_px_close = 0.0;
        self.realized_pnl = 0.0;
        self.realized_return = 0.0;
        self.fill_count = 0;
        self.fills.clear();
        self.adjustments.clear();
        self.ts_opened = None;
        self.ts_closed = None;
    }

    /// Get all fills
    pub fn fills(&self) -> &VecDeque<TradeData> {
        &self.fills
    }

    /// Get all adjustments
    pub fn adjustments(&self) -> &VecDeque<PositionEvent> {
        &self.adjustments
    }

    /// Get total fill count
    pub fn fill_count(&self) -> u64 {
        self.fill_count
    }

    /// Get timestamp opened
    pub fn ts_opened(&self) -> Option<DateTime<Utc>> {
        self.ts_opened
    }

    /// Get timestamp closed
    pub fn ts_closed(&self) -> Option<DateTime<Utc>> {
        self.ts_closed
    }

    /// Get peak quantity
    pub fn peak_qty(&self) -> f64 {
        self.peak_qty
    }

    /// Generate unique position ID
    pub fn generate_position_id(symbol: &str, exchange: Exchange, index: u64) -> String {
        format!("{}.{}.{}", symbol, exchange.value(), index)
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new(String::new(), String::new(), Exchange::Binance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Direction, Exchange, Offset, TradeData};

    fn create_trade(direction: Direction, offset: Offset, price: f64, volume: f64) -> TradeData {
        TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORDER_1".to_string(),
            tradeid: "TRADE_1".to_string(),
            direction: Some(direction),
            offset,
            price,
            volume,
            datetime: Some(Utc::now()),
            extra: None,
        }
    }

    #[test]
    fn test_open_long_position() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        let trade = create_trade(Direction::Long, Offset::Open, 100.0, 10.0);
        pos.apply_fill(&trade).unwrap();

        assert!(pos.is_long());
        assert_eq!(pos.quantity(), 10.0);
        assert_eq!(pos.avg_px_open(), 100.0);
        assert!(pos.ts_opened().is_some());
    }

    #[test]
    fn test_close_long_position() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        // Open long
        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 100.0, 10.0))
            .unwrap();

        // Close long
        pos.apply_fill(&create_trade(Direction::Long, Offset::Close, 110.0, 10.0))
            .unwrap();

        assert!(pos.is_flat());
        assert!(pos.is_closed());
        // PnL = (110 - 100) * 10 = 100
        assert_eq!(pos.realized_pnl(), 100.0);
    }

    #[test]
    fn test_add_to_position() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        // Open long
        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 100.0, 10.0))
            .unwrap();

        // Add to position
        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 110.0, 10.0))
            .unwrap();

        assert_eq!(pos.quantity(), 20.0);
        // Average = (100*10 + 110*10) / 20 = 105
        assert_eq!(pos.avg_px_open(), 105.0);
    }

    #[test]
    fn test_position_flip() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        // Open long
        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 100.0, 10.0))
            .unwrap();

        // Flip to short (close 10 long + open 10 short)
        pos.apply_fill(&create_trade(Direction::Short, Offset::Open, 90.0, 20.0))
            .unwrap();

        assert!(pos.is_short());
        assert_eq!(pos.quantity(), 10.0);
        // First 10 long closed with loss: (90-100)*10 = -100
        assert_eq!(pos.realized_pnl(), -100.0);
        // New short position at 90
        assert_eq!(pos.avg_px_open(), 90.0);
    }

    #[test]
    fn test_unrealized_pnl() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 100.0, 10.0))
            .unwrap();

        // Mark price at 110
        let unrealized = pos.unrealized_pnl(110.0);
        // (110 - 100) * 10 = 100
        assert_eq!(unrealized, 100.0);
    }

    #[test]
    fn test_short_position_pnl() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        // Open short
        pos.apply_fill(&create_trade(Direction::Short, Offset::Open, 100.0, 10.0))
            .unwrap();

        // Close short at 90 (profit)
        pos.apply_fill(&create_trade(Direction::Short, Offset::Close, 90.0, 10.0))
            .unwrap();

        assert!(pos.is_flat());
        // PnL = (100 - 90) * 10 = 100
        assert_eq!(pos.realized_pnl(), 100.0);
    }

    #[test]
    fn test_commission() {
        let mut pos = Position::new("P001".to_string(), "BTCUSDT".to_string(), Exchange::Binance);

        pos.apply_fill(&create_trade(Direction::Long, Offset::Open, 100.0, 10.0))
            .unwrap();
        pos.add_commission(10.0);
        pos.apply_fill(&create_trade(Direction::Long, Offset::Close, 110.0, 10.0))
            .unwrap();

        // PnL = 100, commission = 10
        assert_eq!(pos.realized_pnl(), 100.0);
        assert_eq!(pos.realized_return, -10.0);
    }
}
