//! Fill Models for Backtesting
//!
//! Inspired by nautilus_trader's FillModel system
//! Provides realistic order fill simulation with multiple strategies

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::trader::{BarData, Direction, OrderData, TickData};

/// Result of a fill simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillResult {
    /// Whether the order was filled
    pub filled: bool,
    /// Fill price (may differ from order price due to slippage)
    pub fill_price: f64,
    /// Fill quantity (may be partial)
    pub fill_qty: f64,
    /// Slippage amount
    pub slippage: f64,
    /// Liquidity side (maker or taker)
    pub liquidity_side: LiquiditySide,
    /// Probability of fill (0.0 to 1.0)
    pub prob_fill: f64,
}

impl FillResult {
    /// Create a no-fill result
    pub fn no_fill() -> Self {
        Self {
            filled: false,
            fill_price: 0.0,
            fill_qty: 0.0,
            slippage: 0.0,
            liquidity_side: LiquiditySide::NoLiquidity,
            prob_fill: 0.0,
        }
    }

    /// Create a full fill result
    pub fn full_fill(price: f64, qty: f64, slippage: f64, side: LiquiditySide) -> Self {
        Self {
            filled: true,
            fill_price: price,
            fill_qty: qty,
            slippage,
            liquidity_side: side,
            prob_fill: 1.0,
        }
    }

    /// Create a partial fill result
    pub fn partial_fill(
        price: f64,
        qty: f64,
        slippage: f64,
        side: LiquiditySide,
        prob: f64,
    ) -> Self {
        Self {
            filled: true,
            fill_price: price,
            fill_qty: qty,
            slippage,
            liquidity_side: side,
            prob_fill: prob,
        }
    }
}

/// Liquidity side (maker vs taker)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquiditySide {
    /// No liquidity involved
    NoLiquidity,
    /// Maker (limit order that provides liquidity)
    Maker,
    /// Taker (market order that takes liquidity)
    Taker,
}

/// Base trait for fill models
///
/// Fill models determine how orders are filled in backtesting:
/// - Fill probability
/// - Slippage calculation
/// - Partial fill handling
pub trait FillModel: Send + Sync + fmt::Debug {
    /// Get model name
    fn name(&self) -> &str;

    /// Simulate fill for a limit order on bar data
    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult;

    /// Simulate fill for a market order on bar data
    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult;

    /// Simulate fill for a stop order on bar data
    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult;

    /// Simulate fill on tick data
    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult;

    /// Check if a limit order is marketable (should fill as market)
    fn is_limit_marketable(&self, order: &OrderData, bar: &BarData) -> bool {
        match order.direction {
            Some(Direction::Long) => order.price >= bar.high_price, // Buy at or above high
            Some(Direction::Short) => order.price <= bar.low_price, // Sell at or below low
            _ => false,
        }
    }

    /// Check if stop is triggered
    fn is_stop_triggered(&self, direction: Direction, trigger_price: f64, bar: &BarData) -> bool {
        match direction {
            Direction::Long => bar.high_price >= trigger_price, // Buy stop
            Direction::Short => bar.low_price <= trigger_price, // Sell stop
            Direction::Net => false, // Net direction cannot have stop orders
        }
    }

    /// Clone the model (for use in trait objects)
    fn clone_box(&self) -> Box<dyn FillModel>;
}

impl Clone for Box<dyn FillModel> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// ============================================================================
// Fill Model Implementations
// ============================================================================

/// Best price fill model - optimistic fills at best possible price
///
/// Assumes orders always fill at the best price within the bar.
/// Good for quick testing, but not realistic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestPriceFillModel {
    /// Slippage per unit
    slippage: f64,
}

impl BestPriceFillModel {
    pub fn new(slippage: f64) -> Self {
        Self { slippage }
    }
}

impl FillModel for BestPriceFillModel {
    fn name(&self) -> &str {
        "BestPriceFillModel"
    }

    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        // Check if price is within bar range
        let in_range = match order.direction {
            Some(Direction::Long) => order.price >= bar.low_price,
            Some(Direction::Short) => order.price <= bar.high_price,
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        // Fill at order price (best case)
        let fill_price = order.price;
        let slippage = match order.direction {
            Some(Direction::Long) => self.slippage,
            Some(Direction::Short) => -self.slippage,
            _ => 0.0,
        };

        FillResult::full_fill(
            fill_price + slippage,
            order.volume,
            slippage.abs(),
            LiquiditySide::Maker,
        )
    }

    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        // Market orders fill at market price with slippage
        let (fill_price, slippage) = match order.direction {
            Some(Direction::Long) => {
                // Buy at ask (high) + slippage
                let price = bar.high_price;
                (price + self.slippage, self.slippage)
            }
            Some(Direction::Short) => {
                // Sell at bid (low) - slippage
                let price = bar.low_price;
                (price - self.slippage, self.slippage)
            }
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(fill_price, order.volume, slippage, LiquiditySide::Taker)
    }

    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult {
        // Check if stop triggered
        let triggered = match order.direction {
            Some(Direction::Long) => bar.high_price >= trigger_price,
            Some(Direction::Short) => bar.low_price <= trigger_price,
            _ => return FillResult::no_fill(),
        };

        if !triggered {
            return FillResult::no_fill();
        }

        // Stop fills at trigger price (can be worse due to gaps)
        // More realistic: fill at market after trigger
        let fill_price = match order.direction {
            Some(Direction::Long) => {
                // For buy stop, if we trigger, we might fill at a worse price
                // Use max of trigger price and close price (gap handling)
                let worst_price = trigger_price.max(bar.close_price);
                worst_price + self.slippage
            }
            Some(Direction::Short) => {
                // For sell stop, use min of trigger and close
                let worst_price = trigger_price.min(bar.close_price);
                worst_price - self.slippage
            }
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(
            fill_price,
            order.volume,
            self.slippage,
            LiquiditySide::Taker,
        )
    }

    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult {
        let (in_range, market_price) = match order.direction {
            Some(Direction::Long) => {
                // Buy limit: fill if price >= bid (can buy at or below limit)
                let can_fill = order.price >= tick.bid_price_1;
                (can_fill, tick.ask_price_1) // Pay ask
            }
            Some(Direction::Short) => {
                // Sell limit: fill if price <= ask (can sell at or above limit)
                let can_fill = order.price <= tick.ask_price_1;
                (can_fill, tick.bid_price_1) // Receive bid
            }
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        let slippage = match order.direction {
            Some(Direction::Long) => self.slippage,
            Some(Direction::Short) => -self.slippage,
            _ => 0.0,
        };

        FillResult::full_fill(
            market_price + slippage,
            order.volume,
            slippage.abs(),
            LiquiditySide::Taker,
        )
    }

    fn clone_box(&self) -> Box<dyn FillModel> {
        Box::new(self.clone())
    }
}

/// Two-tier fill model - simulates liquidity tiers
///
/// Provides more realistic fills by considering:
/// - Fill probability based on order size
/// - Two-tier slippage for large orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoTierFillModel {
    /// Base slippage for small orders
    slippage_base: f64,
    /// Additional slippage for large orders
    slippage_extra: f64,
    /// Order size threshold for extra slippage
    size_threshold: f64,
    /// Probability of fill for small orders
    prob_base: f64,
    /// Probability of fill for large orders
    prob_large: f64,
}

impl TwoTierFillModel {
    pub fn new(
        slippage_base: f64,
        slippage_extra: f64,
        size_threshold: f64,
        prob_base: f64,
        prob_large: f64,
    ) -> Self {
        Self {
            slippage_base,
            slippage_extra,
            size_threshold,
            prob_base,
            prob_large,
        }
    }

    /// Calculate effective slippage based on order size
    fn get_slippage(&self, volume: f64) -> f64 {
        if volume <= self.size_threshold {
            self.slippage_base
        } else {
            self.slippage_base + self.slippage_extra
        }
    }

    /// Get fill probability based on order size
    fn get_fill_probability(&self, volume: f64) -> f64 {
        if volume <= self.size_threshold {
            self.prob_base
        } else {
            self.prob_large
        }
    }
}

impl FillModel for TwoTierFillModel {
    fn name(&self) -> &str {
        "TwoTierFillModel"
    }

    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let in_range = match order.direction {
            Some(Direction::Long) => order.price >= bar.low_price,
            Some(Direction::Short) => order.price <= bar.high_price,
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        let slippage = self.get_slippage(order.volume);
        let prob = self.get_fill_probability(order.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => order.price + slippage,
            Some(Direction::Short) => order.price - slippage,
            _ => return FillResult::no_fill(),
        };

        if prob >= 1.0 {
            FillResult::full_fill(fill_price, order.volume, slippage, LiquiditySide::Maker)
        } else {
            FillResult::partial_fill(
                fill_price,
                order.volume,
                slippage,
                LiquiditySide::Maker,
                prob,
            )
        }
    }

    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let slippage = self.get_slippage(order.volume);
        let prob = self.get_fill_probability(order.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => bar.high_price + slippage,
            Some(Direction::Short) => bar.low_price - slippage,
            _ => return FillResult::no_fill(),
        };

        if prob >= 1.0 {
            FillResult::full_fill(fill_price, order.volume, slippage, LiquiditySide::Taker)
        } else {
            FillResult::partial_fill(
                fill_price,
                order.volume,
                slippage,
                LiquiditySide::Taker,
                prob,
            )
        }
    }

    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult {
        let triggered = match order.direction {
            Some(Direction::Long) => bar.high_price >= trigger_price,
            Some(Direction::Short) => bar.low_price <= trigger_price,
            _ => return FillResult::no_fill(),
        };

        if !triggered {
            return FillResult::no_fill();
        }

        let slippage = self.get_slippage(order.volume);

        // Use worse price for stops (gap handling)
        let fill_price = match order.direction {
            Some(Direction::Long) => trigger_price.max(bar.close_price) + slippage,
            Some(Direction::Short) => trigger_price.min(bar.close_price) - slippage,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(fill_price, order.volume, slippage, LiquiditySide::Taker)
    }

    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult {
        let (in_range, market_price) = match order.direction {
            Some(Direction::Long) => (order.price >= tick.bid_price_1, tick.ask_price_1),
            Some(Direction::Short) => (order.price <= tick.ask_price_1, tick.bid_price_1),
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        let slippage = self.get_slippage(order.volume);
        let fill_price = match order.direction {
            Some(Direction::Long) => market_price + slippage,
            Some(Direction::Short) => market_price - slippage,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(fill_price, order.volume, slippage, LiquiditySide::Taker)
    }

    fn clone_box(&self) -> Box<dyn FillModel> {
        Box::new(self.clone())
    }
}

/// Size-aware fill model - adjusts fill based on order size relative to volume
///
/// Considers:
/// - Order size vs bar volume
/// - Market impact for large orders
/// - Partial fill probability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeAwareFillModel {
    /// Base slippage
    base_slippage: f64,
    /// Maximum slippage for very large orders
    max_slippage: f64,
    /// Volume impact coefficient
    impact_coefficient: f64,
    /// Maximum fill percentage for large orders
    max_fill_pct: f64,
}

impl SizeAwareFillModel {
    pub fn new(
        base_slippage: f64,
        max_slippage: f64,
        impact_coefficient: f64,
        max_fill_pct: f64,
    ) -> Self {
        Self {
            base_slippage,
            max_slippage,
            impact_coefficient,
            max_fill_pct,
        }
    }

    /// Calculate market impact based on order size vs volume
    fn calculate_impact(&self, order_size: f64, bar_volume: f64) -> f64 {
        if bar_volume <= 0.0 {
            return self.max_slippage;
        }

        let size_ratio = order_size / bar_volume;
        let impact = self.base_slippage + self.impact_coefficient * size_ratio;
        impact.min(self.max_slippage)
    }
}

impl FillModel for SizeAwareFillModel {
    fn name(&self) -> &str {
        "SizeAwareFillModel"
    }

    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let in_range = match order.direction {
            Some(Direction::Long) => order.price >= bar.low_price,
            Some(Direction::Short) => order.price <= bar.high_price,
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        let impact = self.calculate_impact(order.volume, bar.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => order.price + impact,
            Some(Direction::Short) => order.price - impact,
            _ => return FillResult::no_fill(),
        };

        // Calculate fill percentage based on size vs volume
        let fill_pct = if bar.volume > 0.0 {
            (1.0 - (order.volume / bar.volume) * 0.5).max(self.max_fill_pct)
        } else {
            self.max_fill_pct
        };

        let fill_qty = order.volume * fill_pct;

        FillResult::partial_fill(fill_price, fill_qty, impact, LiquiditySide::Maker, fill_pct)
    }

    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let impact = self.calculate_impact(order.volume, bar.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => bar.high_price + impact,
            Some(Direction::Short) => bar.low_price - impact,
            _ => return FillResult::no_fill(),
        };

        let fill_pct = if bar.volume > 0.0 {
            (1.0 - (order.volume / bar.volume) * 0.3).max(self.max_fill_pct)
        } else {
            self.max_fill_pct
        };

        let fill_qty = order.volume * fill_pct;

        FillResult::partial_fill(fill_price, fill_qty, impact, LiquiditySide::Taker, fill_pct)
    }

    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult {
        let triggered = match order.direction {
            Some(Direction::Long) => bar.high_price >= trigger_price,
            Some(Direction::Short) => bar.low_price <= trigger_price,
            _ => return FillResult::no_fill(),
        };

        if !triggered {
            return FillResult::no_fill();
        }

        let impact = self.calculate_impact(order.volume, bar.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => trigger_price.max(bar.close_price) + impact,
            Some(Direction::Short) => trigger_price.min(bar.close_price) - impact,
            _ => return FillResult::no_fill(),
        };

        // Stops often have worse fill rates due to market conditions
        let fill_pct = self.max_fill_pct.max(0.7);
        let fill_qty = order.volume * fill_pct;

        FillResult::partial_fill(fill_price, fill_qty, impact, LiquiditySide::Taker, fill_pct)
    }

    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult {
        let (in_range, market_price) = match order.direction {
            Some(Direction::Long) => (order.price >= tick.bid_price_1, tick.ask_price_1),
            Some(Direction::Short) => (order.price <= tick.ask_price_1, tick.bid_price_1),
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        // Use tick volume for impact calculation
        let impact = self.calculate_impact(order.volume, tick.volume);

        let fill_price = match order.direction {
            Some(Direction::Long) => market_price + impact,
            Some(Direction::Short) => market_price - impact,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(fill_price, order.volume, impact, LiquiditySide::Taker)
    }

    fn clone_box(&self) -> Box<dyn FillModel> {
        Box::new(self.clone())
    }
}

/// Probabilistic fill model - uses random fill probability
///
/// Based on nautilus_trader's prob_fill_on_limit logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilisticFillModel {
    /// Base slippage
    slippage: f64,
    /// Probability of fill for limit orders in spread
    prob_fill_on_limit: f64,
    /// Probability of slippage
    prob_slippage: f64,
    /// Random seed for reproducibility
    seed: Option<u64>,
}

impl ProbabilisticFillModel {
    pub fn new(slippage: f64, prob_fill_on_limit: f64, prob_slippage: f64) -> Self {
        Self {
            slippage,
            prob_fill_on_limit,
            prob_slippage,
            seed: None,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

impl FillModel for ProbabilisticFillModel {
    fn name(&self) -> &str {
        "ProbabilisticFillModel"
    }

    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let in_range = match order.direction {
            Some(Direction::Long) => order.price >= bar.low_price,
            Some(Direction::Short) => order.price <= bar.high_price,
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        // Use probability for fill
        // In production, this would use a random number generator
        // For deterministic backtesting, we use the probability as-is
        let prob_fill = self.prob_fill_on_limit;
        let apply_slippage = self.prob_slippage > 0.0;

        let fill_price = if apply_slippage {
            match order.direction {
                Some(Direction::Long) => order.price + self.slippage,
                Some(Direction::Short) => order.price - self.slippage,
                _ => order.price,
            }
        } else {
            order.price
        };

        if prob_fill >= 1.0 {
            FillResult::full_fill(
                fill_price,
                order.volume,
                self.slippage,
                LiquiditySide::Maker,
            )
        } else {
            FillResult::partial_fill(
                fill_price,
                order.volume,
                self.slippage,
                LiquiditySide::Maker,
                prob_fill,
            )
        }
    }

    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let fill_price = match order.direction {
            Some(Direction::Long) => bar.high_price + self.slippage,
            Some(Direction::Short) => bar.low_price - self.slippage,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(
            fill_price,
            order.volume,
            self.slippage,
            LiquiditySide::Taker,
        )
    }

    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult {
        let triggered = match order.direction {
            Some(Direction::Long) => bar.high_price >= trigger_price,
            Some(Direction::Short) => bar.low_price <= trigger_price,
            _ => return FillResult::no_fill(),
        };

        if !triggered {
            return FillResult::no_fill();
        }

        // Stop fills with worse price (gap handling)
        let fill_price = match order.direction {
            Some(Direction::Long) => trigger_price.max(bar.close_price) + self.slippage,
            Some(Direction::Short) => trigger_price.min(bar.close_price) - self.slippage,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(
            fill_price,
            order.volume,
            self.slippage,
            LiquiditySide::Taker,
        )
    }

    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult {
        let (in_range, market_price) = match order.direction {
            Some(Direction::Long) => (order.price >= tick.bid_price_1, tick.ask_price_1),
            Some(Direction::Short) => (order.price <= tick.ask_price_1, tick.bid_price_1),
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        let fill_price = match order.direction {
            Some(Direction::Long) => market_price + self.slippage,
            Some(Direction::Short) => market_price - self.slippage,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(
            fill_price,
            order.volume,
            self.slippage,
            LiquiditySide::Taker,
        )
    }

    fn clone_box(&self) -> Box<dyn FillModel> {
        Box::new(self.clone())
    }
}

/// No-slippage fill model - ideal fills with zero slippage
///
/// Useful for testing strategies in ideal conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdealFillModel;

impl IdealFillModel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IdealFillModel {
    fn default() -> Self {
        Self::new()
    }
}

impl FillModel for IdealFillModel {
    fn name(&self) -> &str {
        "IdealFillModel"
    }

    fn simulate_limit_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let in_range = match order.direction {
            Some(Direction::Long) => order.price >= bar.low_price,
            Some(Direction::Short) => order.price <= bar.high_price,
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        FillResult::full_fill(order.price, order.volume, 0.0, LiquiditySide::Maker)
    }

    fn simulate_market_fill(&self, order: &OrderData, bar: &BarData) -> FillResult {
        let fill_price = match order.direction {
            Some(Direction::Long) => bar.close_price, // Fill at close
            Some(Direction::Short) => bar.close_price,
            _ => return FillResult::no_fill(),
        };

        FillResult::full_fill(fill_price, order.volume, 0.0, LiquiditySide::Taker)
    }

    fn simulate_stop_fill(
        &self,
        order: &OrderData,
        bar: &BarData,
        trigger_price: f64,
    ) -> FillResult {
        let triggered = match order.direction {
            Some(Direction::Long) => bar.high_price >= trigger_price,
            Some(Direction::Short) => bar.low_price <= trigger_price,
            _ => return FillResult::no_fill(),
        };

        if !triggered {
            return FillResult::no_fill();
        }

        FillResult::full_fill(trigger_price, order.volume, 0.0, LiquiditySide::Taker)
    }

    fn simulate_tick_fill(&self, order: &OrderData, tick: &TickData) -> FillResult {
        let (in_range, market_price) = match order.direction {
            Some(Direction::Long) => (order.price >= tick.bid_price_1, tick.ask_price_1),
            Some(Direction::Short) => (order.price <= tick.ask_price_1, tick.bid_price_1),
            _ => return FillResult::no_fill(),
        };

        if !in_range {
            return FillResult::no_fill();
        }

        FillResult::full_fill(market_price, order.volume, 0.0, LiquiditySide::Taker)
    }

    fn clone_box(&self) -> Box<dyn FillModel> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Exchange, OrderType, Status};

    fn create_order(direction: Direction, price: f64, volume: f64) -> OrderData {
        OrderData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORDER_1".to_string(),
            order_type: OrderType::Limit,
            direction: Some(direction),
            offset: crate::trader::Offset::Open,
            price,
            volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            extra: None,
        }
    }

    fn create_bar(open: f64, high: f64, low: f64, close: f64, volume: f64) -> BarData {
        BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: chrono::Utc::now(),
            interval: Some(crate::trader::Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover: 0.0,
            open_interest: 0.0,
            extra: None,
        }
    }

    #[test]
    fn test_best_price_fill_model() {
        let model = BestPriceFillModel::new(0.1);
        let order = create_order(Direction::Long, 100.0, 10.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        assert_eq!(result.fill_price, 100.1); // Order price + slippage
    }

    #[test]
    fn test_order_out_of_range() {
        let model = BestPriceFillModel::new(0.1);
        let order = create_order(Direction::Long, 98.0, 10.0); // Below bar low
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(!result.filled);
    }

    #[test]
    fn test_two_tier_fill_model() {
        let model = TwoTierFillModel::new(0.1, 0.2, 100.0, 1.0, 0.8);

        // Small order
        let small_order = create_order(Direction::Long, 100.0, 50.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);
        let result = model.simulate_limit_fill(&small_order, &bar);
        assert!(result.filled);
        assert_eq!(result.fill_price, 100.1); // Base slippage

        // Large order
        let large_order = create_order(Direction::Long, 100.0, 200.0);
        let result = model.simulate_limit_fill(&large_order, &bar);
        assert!(result.filled);
        assert_eq!(result.fill_price, 100.3); // Base + extra slippage
        assert_eq!(result.prob_fill, 0.8); // Lower probability
    }

    #[test]
    fn test_size_aware_fill_model() {
        let model = SizeAwareFillModel::new(0.1, 1.0, 0.5, 0.5);

        // Order is 50% of bar volume - high impact
        let order = create_order(Direction::Long, 100.0, 500.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);
        let result = model.simulate_limit_fill(&order, &bar);

        assert!(result.filled);
        // Impact = 0.1 + 0.5 * (500/1000) = 0.35
        assert!((result.slippage - 0.35).abs() < 0.01);
        // Fill percentage < 100% due to size
        assert!(result.fill_qty < order.volume);
    }

    #[test]
    fn test_stop_fill_with_gap() {
        let model = BestPriceFillModel::new(0.1);
        let order = create_order(Direction::Long, 0.0, 10.0); // Stop order
        let bar = create_bar(105.0, 105.0, 95.0, 95.0, 1000.0); // Gap down

        let result = model.simulate_stop_fill(&order, &bar, 100.0);
        assert!(result.filled);
        // Stop at 100, but market gapped to 95, should fill at worse price
        assert!(result.fill_price >= 100.0);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_best_price_fill_model_sell_direction() {
        let model = BestPriceFillModel::new(0.1);
        let order = create_order(Direction::Short, 100.0, 10.0);
        let bar = create_bar(98.0, 102.0, 97.0, 99.0, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        // Sell limit order: price <= bar.high means it can fill
        // Fill price should be order price - slippage for short
        assert!((result.fill_price - 99.9).abs() < 0.01); // 100.0 - 0.1
        assert_eq!(result.liquidity_side, LiquiditySide::Maker);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_best_price_fill_model_with_slippage_buy() {
        let model = BestPriceFillModel::new(0.5);
        let order = create_order(Direction::Long, 100.0, 10.0);
        let bar = create_bar(99.0, 102.0, 98.0, 101.0, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        // Buy with slippage: fill price should be higher than order price
        assert!(result.fill_price > order.price);
        assert!((result.fill_price - 100.5).abs() < 0.01);
        assert!((result.slippage - 0.5).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_ideal_fill_model_buy() {
        let model = IdealFillModel::new();
        let order = create_order(Direction::Long, 100.0, 10.0);
        let bar = create_bar(99.0, 102.0, 98.0, 101.0, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        // Ideal fill: fills at order price with zero slippage
        assert!((result.fill_price - 100.0).abs() < 0.01);
        assert!((result.slippage - 0.0).abs() < 0.01);
        assert_eq!(result.fill_qty, 10.0);
        assert_eq!(result.liquidity_side, LiquiditySide::Maker);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_ideal_fill_model_sell() {
        let model = IdealFillModel::new();
        let order = create_order(Direction::Short, 100.0, 10.0);
        let bar = create_bar(98.0, 102.0, 97.0, 99.0, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        // Ideal fill: fills at order price with zero slippage
        assert!((result.fill_price - 100.0).abs() < 0.01);
        assert!((result.slippage - 0.0).abs() < 0.01);
        assert_eq!(result.fill_qty, 10.0);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_ideal_fill_model_market_order() {
        let model = IdealFillModel::new();
        let order = create_order(Direction::Long, 0.0, 10.0);
        let bar = create_bar(99.0, 102.0, 98.0, 101.0, 1000.0);

        let result = model.simulate_market_fill(&order, &bar);
        assert!(result.filled);
        // Market order fills at close price
        assert!((result.fill_price - bar.close_price).abs() < 0.01);
        assert!((result.slippage - 0.0).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Taker);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_two_tier_fill_model_partial_fill() {
        // prob_large < 1.0 should produce partial fill
        let model = TwoTierFillModel::new(0.1, 0.2, 100.0, 1.0, 0.8);
        let large_order = create_order(Direction::Long, 100.0, 200.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);

        let result = model.simulate_limit_fill(&large_order, &bar);
        assert!(result.filled);
        // Partial fill because prob < 1.0
        assert!((result.prob_fill - 0.8).abs() < 0.01);
        assert_eq!(result.fill_qty, 200.0);
        assert_eq!(result.liquidity_side, LiquiditySide::Maker);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_size_aware_fill_model_small_volume_ratio() {
        let model = SizeAwareFillModel::new(0.1, 1.0, 0.5, 0.5);

        // Order is 1% of bar volume - low impact
        let order = create_order(Direction::Long, 100.0, 10.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);
        let result = model.simulate_limit_fill(&order, &bar);

        assert!(result.filled);
        // Impact = 0.1 + 0.5 * (10/1000) = 0.105
        assert!((result.slippage - 0.105).abs() < 0.01);
        // Fill percentage should be high
        assert!(result.fill_qty > 9.0);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_probabilistic_fill_model_structure() {
        let model = ProbabilisticFillModel::new(0.2, 0.9, 0.5);
        let order = create_order(Direction::Long, 100.0, 10.0);
        let bar = create_bar(101.0, 102.0, 99.0, 101.5, 1000.0);

        let result = model.simulate_limit_fill(&order, &bar);
        assert!(result.filled);
        // Check fill result structure
        assert_eq!(result.fill_qty, 10.0);
        assert!((result.prob_fill - 0.9).abs() < 0.01);
        assert!((result.slippage - 0.2).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Maker);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_probabilistic_fill_model_market_order() {
        let model = ProbabilisticFillModel::new(0.2, 0.9, 0.5);
        let order = create_order(Direction::Short, 0.0, 10.0);
        let bar = create_bar(99.0, 102.0, 98.0, 101.0, 1000.0);

        let result = model.simulate_market_fill(&order, &bar);
        assert!(result.filled);
        // Market order should fill at bar.low - slippage for short
        assert!((result.fill_price - (bar.low_price - 0.2)).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Taker);
        assert!((result.prob_fill - 1.0).abs() < 0.01); // Full fill for market orders
    }

    #[test]
    fn test_fill_result_no_fill() {
        let result = FillResult::no_fill();
        assert!(!result.filled);
        assert!((result.fill_price - 0.0).abs() < 0.01);
        assert!((result.fill_qty - 0.0).abs() < 0.01);
        assert!((result.slippage - 0.0).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::NoLiquidity);
        assert!((result.prob_fill - 0.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_fill_result_full_fill() {
        let result = FillResult::full_fill(100.0, 10.0, 0.5, LiquiditySide::Maker);
        assert!(result.filled);
        assert!((result.fill_price - 100.0).abs() < 0.01);
        assert!((result.fill_qty - 10.0).abs() < 0.01);
        assert!((result.slippage - 0.5).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Maker);
        assert!((result.prob_fill - 1.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_fill_result_partial_fill() {
        let result = FillResult::partial_fill(100.0, 5.0, 0.3, LiquiditySide::Taker, 0.5);
        assert!(result.filled);
        assert!((result.fill_price - 100.0).abs() < 0.01);
        assert!((result.fill_qty - 5.0).abs() < 0.01);
        assert!((result.slippage - 0.3).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Taker);
        assert!((result.prob_fill - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_liquidity_side_variants() {
        // Test that LiquiditySide variants are distinct
        assert_ne!(LiquiditySide::NoLiquidity, LiquiditySide::Maker);
        assert_ne!(LiquiditySide::Maker, LiquiditySide::Taker);
        assert_ne!(LiquiditySide::Taker, LiquiditySide::NoLiquidity);

        // Test Copy and Clone
        let side = LiquiditySide::Maker;
        let side_copy = side;
        assert_eq!(side, side_copy);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_best_price_fill_model_market_order_buy() {
        let model = BestPriceFillModel::new(0.2);
        let order = create_order(Direction::Long, 0.0, 10.0);
        let bar = create_bar(100.0, 105.0, 98.0, 103.0, 1000.0);

        let result = model.simulate_market_fill(&order, &bar);
        assert!(result.filled);
        // Market buy: fills at high + slippage
        assert!((result.fill_price - (bar.high_price + 0.2)).abs() < 0.01);
        assert_eq!(result.liquidity_side, LiquiditySide::Taker);
    }
}
