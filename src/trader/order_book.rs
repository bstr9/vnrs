//! Order Book module — L2 order book management for market data
//!
//! Provides `OrderBook` for maintaining per-symbol order book state and
//! `OrderBookManager` as a BaseEngine sub-engine integrated into MainEngine.
//!
//! ## Design Decisions (from Oracle consultation)
//! - BTreeMap<Reverse<Decimal>, Decimal> for bids (descending order)
//! - BTreeMap<Decimal, Decimal> for asks (ascending order)
//! - std::sync::RwLock (100ms depth updates are low frequency)
//! - OrderBookManager implements BaseEngine, registers as sub-engine
//! - Both on_depth() callback + direct access pattern for strategies

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tracing::warn;

use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::DepthData;
use super::event::EVENT_DEPTH;

// ============================================================================
// OrderBook
// ============================================================================

/// L2 Order Book for a single instrument.
///
/// Maintains bid and ask sides as sorted maps. Bids use `Reverse<Decimal>`
/// so that `BTreeMap` iterates from highest bid to lowest (descending).
/// Asks iterate from lowest ask to highest (ascending) naturally.
pub struct OrderBook {
    /// Instrument identifier (e.g., "BTCUSDT.BINANCE")
    pub vt_symbol: String,
    /// Bid side: Reverse(price) -> volume (descending iteration)
    bids: BTreeMap<Reverse<Decimal>, Decimal>,
    /// Ask side: price -> volume (ascending iteration)
    asks: BTreeMap<Decimal, Decimal>,
    /// Timestamp of last update
    last_update: DateTime<Utc>,
}

impl OrderBook {
    /// Create a new empty OrderBook for the given instrument
    pub fn new(vt_symbol: String) -> Self {
        Self {
            vt_symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update: Utc::now(),
        }
    }

    /// Update the order book from a DepthData snapshot
    pub fn update_from_depth(&mut self, depth: &DepthData) {
        self.last_update = depth.datetime;

        // Replace entire book with snapshot data
        self.bids.clear();
        self.asks.clear();

        for (price, vol) in &depth.bids {
            self.bids.insert(Reverse(*price), *vol);
        }
        for (price, vol) in &depth.asks {
            self.asks.insert(*price, *vol);
        }
    }

    /// Apply an incremental update to a single price level.
    /// If volume is zero, the level is removed.
    pub fn apply_update(
        &mut self,
        bid_updates: &[(Decimal, Decimal)],
        ask_updates: &[(Decimal, Decimal)],
        datetime: DateTime<Utc>,
    ) {
        self.last_update = datetime;

        for (price, vol) in bid_updates {
            if *vol == Decimal::ZERO {
                self.bids.remove(&Reverse(*price));
            } else {
                self.bids.insert(Reverse(*price), *vol);
            }
        }

        for (price, vol) in ask_updates {
            if *vol == Decimal::ZERO {
                self.asks.remove(price);
            } else {
                self.asks.insert(*price, *vol);
            }
        }
    }

    /// Get best bid price (highest bid)
    pub fn best_bid_price(&self) -> Option<Decimal> {
        self.bids.iter().next().map(|(Reverse(p), _)| *p)
    }

    /// Get best ask price (lowest ask)
    pub fn best_ask_price(&self) -> Option<Decimal> {
        self.asks.iter().next().map(|(p, _)| *p)
    }

    /// Get best bid volume
    pub fn best_bid_volume(&self) -> Option<Decimal> {
        self.bids.iter().next().map(|(_, v)| *v)
    }

    /// Get best ask volume
    pub fn best_ask_volume(&self) -> Option<Decimal> {
        self.asks.iter().next().map(|(_, v)| *v)
    }

    /// Calculate mid price = (best_bid + best_ask) / 2
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid_price(), self.best_ask_price()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Calculate spread = best_ask - best_bid
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_ask_price(), self.best_bid_price()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Calculate volume imbalance = (bid_vol - ask_vol) / (bid_vol + ask_vol)
    ///
    /// Returns value in [-1, 1]. Positive means more bid volume (buying pressure),
    /// negative means more ask volume (selling pressure).
    pub fn volume_imbalance(&self) -> Option<Decimal> {
        match (self.best_bid_volume(), self.best_ask_volume()) {
            (Some(bid_vol), Some(ask_vol)) => {
                let total = bid_vol + ask_vol;
                if total == Decimal::ZERO {
                    None
                } else {
                    Some((bid_vol - ask_vol) / total)
                }
            }
            _ => None,
        }
    }

    /// Calculate VWAP (Volume-Weighted Average Price) for a given side and quantity.
    ///
    /// Walks through the book levels accumulating volume until the target quantity
    /// is filled. Returns the average fill price.
    pub fn vwap(&self, is_buy: bool, quantity: Decimal) -> Option<Decimal> {
        let mut remaining = quantity;
        let mut total_cost = Decimal::ZERO;
        let mut total_filled = Decimal::ZERO;

        if is_buy {
            // Buy walks up the ask side (ascending)
            for (price, vol) in &self.asks {
                let fill = std::cmp::min(*vol, remaining);
                total_cost += *price * fill;
                total_filled += fill;
                remaining -= fill;
                if remaining <= Decimal::ZERO {
                    break;
                }
            }
        } else {
            // Sell walks down the bid side (descending)
            for (Reverse(price), vol) in &self.bids {
                let fill = std::cmp::min(*vol, remaining);
                total_cost += *price * fill;
                total_filled += fill;
                remaining -= fill;
                if remaining <= Decimal::ZERO {
                    break;
                }
            }
        }

        if total_filled > Decimal::ZERO {
            Some(total_cost / total_filled)
        } else {
            None
        }
    }

    /// Calculate micro-price — weighted mid price using volume imbalance.
    ///
    /// micro_price = best_bid + spread * (ask_vol / (bid_vol + ask_vol))
    ///
    /// This is more informative than simple mid-price for short-term direction.
    pub fn micro_price(&self) -> Option<Decimal> {
        match (self.best_bid_price(), self.best_ask_price(), self.best_bid_volume(), self.best_ask_volume()) {
            (Some(bid), Some(ask), Some(bid_vol), Some(ask_vol)) => {
                let total = bid_vol + ask_vol;
                if total == Decimal::ZERO {
                    None
                } else {
                    let spread = ask - bid;
                    Some(bid + spread * (ask_vol / total))
                }
            }
            _ => None,
        }
    }

    /// Calculate book pressure = total bid volume at top N levels / total ask volume at top N levels.
    ///
    /// Values > 1.0 indicate buying pressure, < 1.0 indicate selling pressure.
    pub fn book_pressure(&self, levels: usize) -> Option<Decimal> {
        let bid_total: Decimal = self.bids.iter()
            .take(levels)
            .map(|(_, v)| *v)
            .sum();

        let ask_total: Decimal = self.asks.iter()
            .take(levels)
            .map(|(_, v)| *v)
            .sum();

        if ask_total == Decimal::ZERO {
            if bid_total > Decimal::ZERO {
                Some(Decimal::MAX)
            } else {
                None
            }
        } else {
            Some(bid_total / ask_total)
        }
    }

    /// Get the top N bid levels as (price, volume) pairs in descending order
    pub fn bid_levels(&self, n: usize) -> Vec<(Decimal, Decimal)> {
        self.bids.iter()
            .take(n)
            .map(|(Reverse(p), v)| (*p, *v))
            .collect()
    }

    /// Get the top N ask levels as (price, volume) pairs in ascending order
    pub fn ask_levels(&self, n: usize) -> Vec<(Decimal, Decimal)> {
        self.asks.iter()
            .take(n)
            .map(|(p, v)| (*p, *v))
            .collect()
    }

    /// Get total bid volume across all levels
    pub fn total_bid_volume(&self) -> Decimal {
        self.bids.values().sum()
    }

    /// Get total ask volume across all levels
    pub fn total_ask_volume(&self) -> Decimal {
        self.asks.values().sum()
    }

    /// Get number of bid levels
    pub fn bid_depth(&self) -> usize {
        self.bids.len()
    }

    /// Get number of ask levels
    pub fn ask_depth(&self) -> usize {
        self.asks.len()
    }

    /// Get last update timestamp
    pub fn last_update(&self) -> DateTime<Utc> {
        self.last_update
    }
}

// ============================================================================
// OrderBookManager
// ============================================================================

/// Manages order books for all subscribed instruments.
///
/// Registered as a sub-engine in MainEngine, receives DepthBook events
/// and maintains per-symbol OrderBook instances for direct access.
pub struct OrderBookManager {
    /// Order books by vt_symbol
    books: RwLock<HashMap<String, OrderBook>>,
}

impl OrderBookManager {
    /// Create a new OrderBookManager
    pub fn new() -> Self {
        Self {
            books: RwLock::new(HashMap::new()),
        }
    }

    /// Process a DepthData event — create or update the order book
    pub fn process_depth(&self, depth: &DepthData) {
        let vt_symbol = depth.vt_symbol();
        let mut books = self.books.write().unwrap_or_else(|e| {
            warn!("OrderBookManager lock poisoned, recovering");
            e.into_inner()
        });

        match books.get_mut(&vt_symbol) {
            Some(book) => {
                book.update_from_depth(depth);
            }
            None => {
                let mut book = OrderBook::new(vt_symbol.clone());
                book.update_from_depth(depth);
                books.insert(vt_symbol, book);
            }
        }
    }

    /// Get the OrderBook for a given instrument
    pub fn get_book(&self, vt_symbol: &str) -> Option<OrderBookSnapshot> {
        let books = self.books.read().unwrap_or_else(|e| {
            warn!("OrderBookManager lock poisoned, recovering");
            e.into_inner()
        });

        books.get(vt_symbol).map(|book| OrderBookSnapshot {
            vt_symbol: book.vt_symbol.clone(),
            best_bid: book.best_bid_price(),
            best_ask: book.best_ask_price(),
            best_bid_volume: book.best_bid_volume(),
            best_ask_volume: book.best_ask_volume(),
            mid_price: book.mid_price(),
            spread: book.spread(),
            volume_imbalance: book.volume_imbalance(),
            bid_levels: book.bid_levels(5),
            ask_levels: book.ask_levels(5),
            total_bid_volume: book.total_bid_volume(),
            total_ask_volume: book.total_ask_volume(),
            bid_depth: book.bid_depth(),
            ask_depth: book.ask_depth(),
            last_update: book.last_update(),
        })
    }

    /// Get all managed book symbols
    pub fn get_all_symbols(&self) -> Vec<String> {
        let books = self.books.read().unwrap_or_else(|e| {
            warn!("OrderBookManager lock poisoned, recovering");
            e.into_inner()
        });
        books.keys().cloned().collect()
    }

    /// Remove an order book (e.g., when unsubscribing)
    pub fn remove_book(&self, vt_symbol: &str) -> bool {
        let mut books = self.books.write().unwrap_or_else(|e| {
            warn!("OrderBookManager lock poisoned, recovering");
            e.into_inner()
        });
        books.remove(vt_symbol).is_some()
    }
}

impl Default for OrderBookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for OrderBookManager {
    fn engine_name(&self) -> &str {
        "OrderBookManager"
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        // Handle both symbol-specific and base depth events
        if event_type.starts_with(EVENT_DEPTH) {
            if let GatewayEvent::DepthBook(depth) = event {
                self.process_depth(depth);
            }
        }
    }
}

// ============================================================================
// OrderBookSnapshot — read-only snapshot for external consumers
// ============================================================================

/// A read-only snapshot of an order book at a point in time.
///
/// Safe to pass around without holding the lock. Provides commonly needed
/// computed values (mid_price, spread, imbalance) to avoid recomputation.
#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub vt_symbol: String,
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub best_bid_volume: Option<Decimal>,
    pub best_ask_volume: Option<Decimal>,
    pub mid_price: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub volume_imbalance: Option<Decimal>,
    pub bid_levels: Vec<(Decimal, Decimal)>,
    pub ask_levels: Vec<(Decimal, Decimal)>,
    pub total_bid_volume: Decimal,
    pub total_ask_volume: Decimal,
    pub bid_depth: usize,
    pub ask_depth: usize,
    pub last_update: DateTime<Utc>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::constant::Exchange;

    /// Helper to convert f64 to Decimal (panics on failure, only in tests)
    fn dec(v: f64) -> Decimal {
        Decimal::from_f64_retain(v).expect("invalid decimal from f64")
    }

    fn make_depth(
        symbol: &str,
        bid_levels: Vec<(f64, f64)>,
        ask_levels: Vec<(f64, f64)>,
    ) -> DepthData {
        let mut depth = DepthData::new(
            "test_gateway".to_string(),
            symbol.to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        for (price, vol) in bid_levels {
            depth.bids.insert(dec(price), dec(vol));
        }
        for (price, vol) in ask_levels {
            depth.asks.insert(dec(price), dec(vol));
        }
        depth
    }

    #[test]
    fn test_order_book_basic() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        // Empty book
        assert!(book.best_bid_price().is_none());
        assert!(book.best_ask_price().is_none());
        assert!(book.mid_price().is_none());

        // Update from depth snapshot
        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 1.0), (49998.0, 2.0), (49997.0, 3.0)],
            vec![(50001.0, 1.5), (50002.0, 2.5), (50003.0, 3.5)],
        );
        book.update_from_depth(&depth);

        // Best bid should be highest (49999)
        assert_eq!(book.best_bid_price(), Some(dec(49999.0)));
        // Best ask should be lowest (50001)
        assert_eq!(book.best_ask_price(), Some(dec(50001.0)));
        // Mid price
        let mid = book.mid_price().expect("mid price should exist");
        assert_eq!(mid, dec(50000.0));
        // Spread
        let spread = book.spread().expect("spread should exist");
        assert_eq!(spread, dec(2.0));
    }

    #[test]
    fn test_volume_imbalance() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        // Equal volumes -> imbalance = 0
        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 5.0)],
            vec![(50001.0, 5.0)],
        );
        book.update_from_depth(&depth);
        let imbalance = book.volume_imbalance().expect("imbalance should exist");
        assert_eq!(imbalance, Decimal::ZERO);

        // More bid volume -> positive imbalance
        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 8.0)],
            vec![(50001.0, 2.0)],
        );
        book.update_from_depth(&depth);
        let imbalance = book.volume_imbalance().expect("imbalance should exist");
        assert!(imbalance > Decimal::ZERO);
    }

    #[test]
    fn test_vwap() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 10.0)],
            vec![(50001.0, 5.0), (50002.0, 5.0)],
        );
        book.update_from_depth(&depth);

        // Buy 3.0 at ask: should fill at 50001.0
        let vwap = book.vwap(true, dec(3.0));
        assert_eq!(vwap, Some(dec(50001.0)));

        // Buy 7.0 at ask: fills 5.0 @ 50001 + 2.0 @ 50002 = (250005 + 100004) / 7
        let vwap = book.vwap(true, dec(7.0));
        assert!(vwap.is_some());
        let vwap_val = vwap.expect("vwap should exist");
        assert!(vwap_val > dec(50001.0));
        assert!(vwap_val < dec(50002.0));
    }

    #[test]
    fn test_micro_price() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 3.0)],
            vec![(50001.0, 1.0)],
        );
        book.update_from_depth(&depth);

        let micro = book.micro_price().expect("micro price should exist");
        // micro = 49999 + 2 * (1 / 4) = 49999 + 0.5 = 49999.5
        let expected = dec(49999.0) + dec(2.0) * dec(1.0) / dec(4.0);
        assert_eq!(micro, expected);
    }

    #[test]
    fn test_book_pressure() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 10.0), (49998.0, 5.0)],
            vec![(50001.0, 3.0), (50002.0, 2.0)],
        );
        book.update_from_depth(&depth);

        let pressure = book.book_pressure(2).expect("pressure should exist");
        // (10 + 5) / (3 + 2) = 3.0
        assert_eq!(pressure, dec(3.0));
    }

    #[test]
    fn test_apply_incremental_update() {
        let mut book = OrderBook::new("BTCUSDT.BINANCE".to_string());

        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 1.0), (49998.0, 2.0)],
            vec![(50001.0, 1.5), (50002.0, 2.5)],
        );
        book.update_from_depth(&depth);

        // Incremental: add new bid level, remove ask level, update existing bid
        book.apply_update(
            &[
                (dec(50000.0), dec(3.0)), // new level
                (dec(49999.0), dec(5.0)), // update existing
            ],
            &[
                (dec(50001.0), Decimal::ZERO), // remove level
            ],
            Utc::now(),
        );

        // Best bid should now be 50000 (higher than 49999)
        assert_eq!(book.best_bid_price(), Some(dec(50000.0)));
        // Best ask should now be 50002 (50001 was removed)
        assert_eq!(book.best_ask_price(), Some(dec(50002.0)));
        // Verify bid level update
        let bid_levels = book.bid_levels(3);
        assert_eq!(bid_levels[0].1, dec(3.0)); // 50000 vol
        assert_eq!(bid_levels[1].1, dec(5.0)); // 49999 vol (updated)
    }

    #[test]
    fn test_order_book_manager() {
        let manager = OrderBookManager::new();

        // Process a depth event
        let depth = make_depth(
            "BTCUSDT",
            vec![(49999.0, 1.0)],
            vec![(50001.0, 2.0)],
        );
        manager.process_depth(&depth);

        // Verify book was created
        let snapshot = manager.get_book("BTCUSDT.BINANCE").expect("book should exist");
        assert_eq!(snapshot.best_bid, Some(dec(49999.0)));
        assert_eq!(snapshot.best_ask, Some(dec(50001.0)));

        // Update the book
        let depth2 = make_depth(
            "BTCUSDT",
            vec![(50000.0, 3.0)],
            vec![(50002.0, 4.0)],
        );
        manager.process_depth(&depth2);

        let snapshot2 = manager.get_book("BTCUSDT.BINANCE").expect("book should exist");
        assert_eq!(snapshot2.best_bid, Some(dec(50000.0)));

        // Verify symbol list
        let symbols = manager.get_all_symbols();
        assert_eq!(symbols.len(), 1);

        // Remove book
        assert!(manager.remove_book("BTCUSDT.BINANCE"));
        assert!(manager.get_book("BTCUSDT.BINANCE").is_none());
    }

    #[test]
    fn test_depth_data_from_tick() {
        use super::super::object::TickData;

        let mut tick = TickData::new(
            "BINANCE_SPOT".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        tick.bid_price_1 = 49999.0;
        tick.bid_volume_1 = 1.5;
        tick.ask_price_1 = 50001.0;
        tick.ask_volume_1 = 2.5;
        tick.bid_price_2 = 49998.0;
        tick.bid_volume_2 = 0.5;

        let depth = DepthData::from_tick(&tick);
        assert_eq!(depth.vt_symbol(), "BTCUSDT.BINANCE");
        assert_eq!(depth.bids.len(), 2);
        assert_eq!(depth.asks.len(), 1);
        assert_eq!(depth.best_bid_price(), Some(dec(49999.0)));
        assert_eq!(depth.best_ask_price(), Some(dec(50001.0)));
    }

    #[test]
    fn test_base_engine_dispatch() {
        let manager = OrderBookManager::new();

        let depth = make_depth(
            "ETHUSDT",
            vec![(2999.0, 10.0)],
            vec![(3001.0, 20.0)],
        );

        // Simulate dispatch from MainEngine
        manager.process_event("eDepth.ETHUSDT.BINANCE", &GatewayEvent::DepthBook(depth.clone()));

        let snapshot = manager.get_book("ETHUSDT.BINANCE").expect("book should exist");
        assert_eq!(snapshot.best_bid, Some(dec(2999.0)));
        assert_eq!(snapshot.best_ask, Some(dec(3001.0)));
    }
}
