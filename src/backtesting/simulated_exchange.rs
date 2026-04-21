//! SimulatedExchange - per-instrument matching engine with configurable fee/latency models.
//!
//! Extracts order matching from BacktestingEngine into a separate exchange abstraction,
//! following nautilus_trader's architecture. Each instrument gets its own matching engine
//! with independent fill models, fee schedules, and latency characteristics.

use std::collections::HashMap;
use std::fmt;

use crate::trader::{
    BarData, Clock, Direction, Exchange, Offset, OrderData, OrderRequest, OrderType, Status,
    TickData, TradeData,
};
use crate::strategy::{StopOrder, StopOrderStatus};
use super::fill_model::{FillModel, LiquiditySide};
use super::position::Position;
use super::risk_engine::{RiskConfig, RiskEngine};

// ============================================================================
// Fee Model Trait and Implementations
// ============================================================================

/// Fee model trait for calculating transaction costs.
///
/// FeeModel is separate from FillModel:
/// - FillModel determines IF and AT WHAT PRICE an order fills
/// - FeeModel calculates the commission for that fill
pub trait FeeModel: Send + Sync + fmt::Debug {
    /// Get model name for logging/debugging
    fn name(&self) -> &str;

    /// Calculate fee for a fill. Returns fee amount (always positive).
    fn calculate_fee(
        &self,
        fill_price: f64,
        quantity: f64,
        direction: Direction,
        liquidity_side: LiquiditySide,
    ) -> f64;

    /// Clone the model (for trait objects)
    fn clone_box(&self) -> Box<dyn FeeModel>;
}

impl Clone for Box<dyn FeeModel> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Maker/taker fee model with different rates for maker vs taker liquidity.
#[derive(Debug, Clone)]
pub struct MakerTakerFeeModel {
    /// Maker fee rate (e.g., 0.001 = 0.1%)
    pub maker_rate: f64,
    /// Taker fee rate (e.g., 0.001 = 0.1%)
    pub taker_rate: f64,
}

impl MakerTakerFeeModel {
    pub fn new(maker_rate: f64, taker_rate: f64) -> Self {
        Self { maker_rate, taker_rate }
    }

    /// Binance default fee model (0.1% maker, 0.1% taker).
    pub fn binance_default() -> Self {
        Self::new(0.001, 0.001)
    }
}

impl FeeModel for MakerTakerFeeModel {
    fn name(&self) -> &str {
        "MakerTakerFeeModel"
    }

    fn calculate_fee(
        &self,
        fill_price: f64,
        quantity: f64,
        _direction: Direction,
        liquidity_side: LiquiditySide,
    ) -> f64 {
        let rate = match liquidity_side {
            LiquiditySide::Maker => self.maker_rate,
            LiquiditySide::Taker => self.taker_rate,
            LiquiditySide::NoLiquidity => 0.0,
        };
        fill_price * quantity * rate
    }

    fn clone_box(&self) -> Box<dyn FeeModel> {
        Box::new(self.clone())
    }
}

/// Flat fee model with fixed fee per trade.
#[derive(Debug, Clone)]
pub struct FlatFeeModel {
    /// Fixed fee per trade
    pub fee_per_trade: f64,
}

impl FlatFeeModel {
    pub fn new(fee_per_trade: f64) -> Self {
        Self { fee_per_trade }
    }
}

impl FeeModel for FlatFeeModel {
    fn name(&self) -> &str {
        "FlatFeeModel"
    }

    fn calculate_fee(
        &self,
        _fill_price: f64,
        _quantity: f64,
        _direction: Direction,
        _liquidity_side: LiquiditySide,
    ) -> f64 {
        self.fee_per_trade
    }

    fn clone_box(&self) -> Box<dyn FeeModel> {
        Box::new(self.clone())
    }
}

/// Percentage fee model with rate as percentage of trade value.
#[derive(Debug, Clone)]
pub struct PercentFeeModel {
    /// Fee rate as decimal (e.g., 0.0005 = 0.05%)
    pub rate: f64,
}

impl PercentFeeModel {
    pub fn new(rate: f64) -> Self {
        Self { rate }
    }
}

impl FeeModel for PercentFeeModel {
    fn name(&self) -> &str {
        "PercentFeeModel"
    }

    fn calculate_fee(
        &self,
        fill_price: f64,
        quantity: f64,
        _direction: Direction,
        _liquidity_side: LiquiditySide,
    ) -> f64 {
        fill_price * quantity * self.rate
    }

    fn clone_box(&self) -> Box<dyn FeeModel> {
        Box::new(self.clone())
    }
}

/// No fee model - zero fees for all trades (for testing).
#[derive(Debug, Clone, Default)]
pub struct NoFeeModel;

impl NoFeeModel {
    pub fn new() -> Self {
        Self
    }
}

impl FeeModel for NoFeeModel {
    fn name(&self) -> &str {
        "NoFeeModel"
    }

    fn calculate_fee(
        &self,
        _fill_price: f64,
        _quantity: f64,
        _direction: Direction,
        _liquidity_side: LiquiditySide,
    ) -> f64 {
        0.0
    }

    fn clone_box(&self) -> Box<dyn FeeModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Latency Model Trait and Implementations
// ============================================================================

/// Latency model trait for simulating order execution delays.
pub trait LatencyModel: Send + Sync + fmt::Debug {
    /// Get latency in milliseconds.
    fn latency_ms(&self) -> u64;

    /// Clone the model (for trait objects)
    fn clone_box(&self) -> Box<dyn LatencyModel>;
}

impl Clone for Box<dyn LatencyModel> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Zero latency model - instant execution.
#[derive(Debug, Clone, Default)]
pub struct ZeroLatency;

impl ZeroLatency {
    pub fn new() -> Self {
        Self
    }
}

impl LatencyModel for ZeroLatency {
    fn latency_ms(&self) -> u64 {
        0
    }

    fn clone_box(&self) -> Box<dyn LatencyModel> {
        Box::new(self.clone())
    }
}

/// Fixed latency model - constant delay for all orders.
#[derive(Debug, Clone)]
pub struct FixedLatency {
    /// Latency in milliseconds
    pub latency_ms: u64,
}

impl FixedLatency {
    pub fn new(latency_ms: u64) -> Self {
        Self { latency_ms }
    }
}

impl LatencyModel for FixedLatency {
    fn latency_ms(&self) -> u64 {
        self.latency_ms
    }

    fn clone_box(&self) -> Box<dyn LatencyModel> {
        Box::new(self.clone())
    }
}

/// Random latency model with uniform distribution in a range.
///
/// Returns the midpoint of the range for deterministic backtesting.
#[derive(Debug, Clone)]
pub struct RandomLatency {
    /// Minimum latency in milliseconds
    pub min_ms: u64,
    /// Maximum latency in milliseconds
    pub max_ms: u64,
}

impl RandomLatency {
    pub fn new(min_ms: u64, max_ms: u64) -> Self {
        Self { min_ms, max_ms }
    }
}

impl LatencyModel for RandomLatency {
    fn latency_ms(&self) -> u64 {
        (self.min_ms + self.max_ms) / 2
    }

    fn clone_box(&self) -> Box<dyn LatencyModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Instrument Configuration
// ============================================================================

/// Per-instrument configuration for matching engine.
#[derive(Clone)]
pub struct InstrumentConfig {
    /// Instrument identifier (e.g., "BTCUSDT.BINANCE")
    pub vt_symbol: String,
    /// Minimum price movement
    pub pricetick: f64,
    /// Contract size multiplier (1.0 for spot, may differ for futures)
    pub size: f64,
    /// Fill model for this instrument
    pub fill_model: Box<dyn FillModel>,
    /// Fee model for this instrument (optional, falls back to exchange default)
    pub fee_model: Option<Box<dyn FeeModel>>,
}

impl fmt::Debug for InstrumentConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstrumentConfig")
            .field("vt_symbol", &self.vt_symbol)
            .field("pricetick", &self.pricetick)
            .field("size", &self.size)
            .field("fill_model", &self.fill_model.name())
            .field("fee_model", &self.fee_model.as_ref().map(|m| m.name()))
            .finish()
    }
}

impl InstrumentConfig {
    /// Create a new instrument configuration.
    pub fn new(vt_symbol: String, pricetick: f64, size: f64, fill_model: Box<dyn FillModel>) -> Self {
        Self {
            vt_symbol,
            pricetick,
            size,
            fill_model,
            fee_model: None,
        }
    }

    /// Set the fee model.
    pub fn with_fee_model(mut self, fee_model: Box<dyn FeeModel>) -> Self {
        self.fee_model = Some(fee_model);
        self
    }
}

// ============================================================================
// Instrument Matching Engine
// ============================================================================

/// Per-instrument order matching engine.
///
/// Owns the order book for one instrument and handles:
/// - Order submission and cancellation
/// - Limit order matching with bar/tick data
/// - Stop order triggering
/// - Position tracking
///
/// The matching logic is extracted from BacktestingEngine::cross_limit_order
/// and BacktestingEngine::cross_stop_order.
pub struct InstrumentMatchingEngine {
    /// Instrument configuration
    pub config: InstrumentConfig,
    /// All limit orders (including inactive)
    limit_orders: HashMap<String, OrderData>,
    /// Active limit orders waiting to be filled
    active_limit_orders: HashMap<String, OrderData>,
    /// All stop orders (including inactive)
    stop_orders: HashMap<String, StopOrder>,
    /// Active stop orders waiting to be triggered
    active_stop_orders: HashMap<String, StopOrder>,
    /// Trade counter for generating trade IDs
    trade_count: u64,
    /// Limit order counter for generating order IDs
    limit_order_count: u64,
    /// Stop order counter for generating order IDs
    stop_order_count: u64,
    /// Position for this instrument
    position: Position,
}

impl fmt::Debug for InstrumentMatchingEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstrumentMatchingEngine")
            .field("vt_symbol", &self.config.vt_symbol)
            .field("active_limit_orders", &self.active_limit_orders.len())
            .field("active_stop_orders", &self.active_stop_orders.len())
            .field("trade_count", &self.trade_count)
            .field("position", &self.position.signed_qty())
            .finish()
    }
}

impl InstrumentMatchingEngine {
    /// Create a new instrument matching engine.
    pub fn new(config: InstrumentConfig) -> Self {
        let (symbol, exchange) = crate::trader::utility::extract_vt_symbol(&config.vt_symbol)
            .unwrap_or_else(|| {
                // Fallback: split from left, default exchange to Binance
                let symbol = config.vt_symbol.split('.').next().unwrap_or("").to_string();
                (symbol, Exchange::Binance)
            });

        let position = Position::new(
            Position::generate_position_id(&symbol, exchange, 0),
            symbol,
            exchange,
        ).with_size_multiplier(config.size);

        Self {
            config,
            limit_orders: HashMap::new(),
            active_limit_orders: HashMap::new(),
            stop_orders: HashMap::new(),
            active_stop_orders: HashMap::new(),
            trade_count: 0,
            limit_order_count: 0,
            stop_order_count: 0,
            position,
        }
    }

    /// Submit a limit order.
    pub fn submit_order(
        &mut self,
        req: OrderRequest,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        clock: &dyn Clock,
    ) -> Result<OrderData, String> {
        self.limit_order_count += 1;
        let vt_orderid = format!("{}_{}", self.config.vt_symbol, self.limit_order_count);

        let order = OrderData {
            gateway_name: "SIMULATED_EXCHANGE".to_string(),
            symbol: req.symbol,
            exchange,
            orderid: vt_orderid.clone(),
            order_type: req.order_type,
            direction: Some(direction),
            offset,
            price: req.price,
            volume: req.volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: Some(clock.now()),
            reference: req.reference,
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        self.limit_orders.insert(vt_orderid.clone(), order.clone());
        self.active_limit_orders.insert(vt_orderid, order.clone());

        tracing::debug!(
            "订单提交成功: {} {} {}@{}",
            order.orderid, order.symbol, order.volume, order.price
        );

        Ok(order)
    }

    /// Submit a stop order.
    pub fn submit_stop_order(
        &mut self,
        req: OrderRequest,
        direction: Direction,
        offset: Offset,
        clock: &dyn Clock,
    ) -> Result<StopOrder, String> {
        self.stop_order_count += 1;
        let stop_orderid = format!("STOP_{}_{}", self.config.vt_symbol, self.stop_order_count);

        let stop_order = StopOrder {
            stop_orderid: stop_orderid.clone(),
            vt_symbol: self.config.vt_symbol.clone(),
            direction,
            offset: Some(offset),
            price: req.price,
            volume: req.volume,
            order_type: req.order_type,
            limit_price: None,
            strategy_name: req.reference.clone(),
            lock: false,
            vt_orderid: None,
            status: StopOrderStatus::Waiting,
            datetime: clock.now(),
        };

        self.stop_orders.insert(stop_orderid.clone(), stop_order.clone());
        self.active_stop_orders.insert(stop_orderid, stop_order.clone());

        tracing::debug!(
            "止损单提交成功: {} trigger@{} vol={}",
            stop_order.stop_orderid, stop_order.price, stop_order.volume
        );

        Ok(stop_order)
    }

    /// Cancel an order.
    pub fn cancel_order(&mut self, vt_orderid: &str) -> Result<OrderData, String> {
        if let Some(order) = self.active_limit_orders.remove(vt_orderid) {
            tracing::debug!("订单取消成功: {}", vt_orderid);
            Ok(order)
        } else if let Some(stop_order) = self.active_stop_orders.remove(vt_orderid) {
            let (symbol, exchange_val) = crate::trader::utility::extract_vt_symbol(&self.config.vt_symbol)
                .unwrap_or((self.config.vt_symbol.split('.').next().unwrap_or("").to_string(), Exchange::Binance));
            Ok(OrderData {
                gateway_name: "SIMULATED_EXCHANGE".to_string(),
                symbol,
                exchange: exchange_val,
                orderid: stop_order.stop_orderid.clone(),
                order_type: stop_order.order_type,
                direction: Some(stop_order.direction),
                offset: stop_order.offset.unwrap_or(Offset::Open),
                price: stop_order.price,
                volume: stop_order.volume,
                traded: 0.0,
                status: Status::Cancelled,
                datetime: Some(stop_order.datetime),
                reference: stop_order.strategy_name,
                post_only: false,
                reduce_only: false,
                expire_time: None,
                extra: None,
            })

        } else {
            Err(format!("订单不存在: {}", vt_orderid))
        }
    }

    /// Process a bar - match pending orders, return fills.
    ///
    /// Core matching logic extracted from BacktestingEngine::cross_limit_order
    /// and BacktestingEngine::cross_stop_order.
    pub fn process_bar(
        &mut self,
        bar: &BarData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        let mut trades = Vec::new();
        self.cross_limit_order(bar, exchange, clock, &mut trades);
        self.cross_stop_order(bar, exchange, clock, &mut trades);
        trades
    }

    /// Process a tick - match pending orders, return fills.
    pub fn process_tick(
        &mut self,
        tick: &TickData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        let mut trades = Vec::new();
        self.cross_limit_order_tick(tick, exchange, clock, &mut trades);
        self.cross_stop_order_tick(tick, exchange, clock, &mut trades);
        trades
    }

    /// Cross limit orders with bar data using FillModel.
    ///
    /// Replicates the logic from BacktestingEngine::cross_limit_order.
    fn cross_limit_order(
        &mut self,
        bar: &BarData,
        exchange: Exchange,
        clock: &dyn Clock,
        trades: &mut Vec<TradeData>,
    ) {
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut to_remove = Vec::new();

        for (vt_orderid, order) in active_orders {
            let result = self.config.fill_model.simulate_limit_fill(&order, bar);

            if result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "SIMULATED_EXCHANGE".to_string(),
                    symbol: order.symbol.clone(),
                    exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}_{}", self.config.vt_symbol, self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: result.fill_price,
                    volume: result.fill_qty,
                    datetime: Some(clock.now()),
                    extra: None,
                };

                if let Err(e) = self.position.apply_fill(&trade) {
                    tracing::error!("仓位更新失败: {}", e);
                    continue;
                }

                trades.push(trade);

                let remaining = order.volume - order.traded - result.fill_qty;
                if remaining > 1e-10 {
                    if let Some(active_order) = self.active_limit_orders.get_mut(&vt_orderid) {
                        active_order.traded += result.fill_qty;
                    }
                } else {
                    to_remove.push(vt_orderid.clone());
                }
            }
        }

        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with bar data using FillModel.
    ///
    /// Replicates the logic from BacktestingEngine::cross_stop_order.
      /// Evaluate stop orders against the current bar and trigger matches.
    ///
    /// **Bar-mode stop trigger (backtesting) vs tick-mode trigger (live trading):**
    ///
    /// In backtesting, stop orders are evaluated once per bar using the bar's
    /// high/low prices. A buy stop triggers when `bar.high >= stop_price`,
    /// and a sell stop triggers when `bar.low <= stop_price`. This is an
    /// approximation — in reality, the exact intra-bar price path is unknown,
    /// so the fill price may differ from what would happen in live trading
    /// where stops trigger on every tick.
    ///
    /// **Implications:**
    /// - In a single-bar scenario, both a buy stop and a sell stop could
    ///   trigger on the same bar (e.g., a large wick). In live trading,
    ///   only one would execute first depending on tick ordering.
    /// - The fill price is determined by the `FillModel` (e.g., worst-case
    ///   for stop fills to account for slippage), not the exact trigger price.
    /// - For strategies that rely on precise stop execution timing (e.g.,
    ///   scalping), bar-mode backtesting will overestimate fill quality.
    ///   Use tick-level data for such strategies.
    fn cross_stop_order(
        &mut self,
        bar: &BarData,
        exchange: Exchange,
        clock: &dyn Clock,
        trades: &mut Vec<TradeData>,
    ) {
        let mut to_trigger = Vec::new();

        for (stop_orderid, stop_order) in self.active_stop_orders.iter() {
            let should_trigger = match stop_order.direction {
                Direction::Long => bar.high_price >= stop_order.price,
                Direction::Short => bar.low_price <= stop_order.price,
                _ => false,
            };

            if should_trigger {
                to_trigger.push((stop_orderid.clone(), stop_order.clone()));
            }
        }

        for (stop_orderid, mut stop_order) in to_trigger {
            stop_order.status = StopOrderStatus::Triggered;

            let symbol = self.config.vt_symbol.split('.').next().unwrap_or("").to_string();
            let order = OrderData {
                gateway_name: "SIMULATED_EXCHANGE".to_string(),
                symbol,
                exchange,
                orderid: stop_orderid.clone(),
                order_type: stop_order.order_type,
                direction: Some(stop_order.direction),
                offset: stop_order.offset.unwrap_or(Offset::Open),
                price: stop_order.limit_price.unwrap_or(stop_order.price),
                volume: stop_order.volume,
                traded: 0.0,
                status: Status::NotTraded,
                datetime: Some(bar.datetime),
                reference: String::new(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
                extra: None,
            };

            // Use appropriate fill simulation based on order type
            let fill_result = match stop_order.order_type {
                OrderType::StopLimit => {
                    // StopLimit: after trigger, behaves like a limit order at limit_price
                    self.config.fill_model.simulate_limit_fill(&order, bar)
                }
                _ => {
                    // Stop (market): fill at market price after trigger
                    self.config.fill_model.simulate_stop_fill(&order, bar, stop_order.price)
                }
            };

            if fill_result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "SIMULATED_EXCHANGE".to_string(),
                    symbol: order.symbol.clone(),
                    exchange,
                    orderid: stop_orderid.clone(),
                    tradeid: format!("{}_{}", self.config.vt_symbol, self.trade_count),
                    direction: Some(stop_order.direction),
                    offset: stop_order.offset.unwrap_or(Offset::Open),
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,
                    datetime: Some(clock.now()),
                    extra: None,
                };

                if let Err(e) = self.position.apply_fill(&trade) {
                    tracing::error!("仓位更新失败: {}", e);
                } else {
                    trades.push(trade);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            } else {
                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            }
        }
    }

    /// Cross limit orders with tick data using FillModel.
    fn cross_limit_order_tick(
        &mut self,
        tick: &TickData,
        exchange: Exchange,
        clock: &dyn Clock,
        trades: &mut Vec<TradeData>,
    ) {
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut to_remove = Vec::new();

        for (vt_orderid, order) in active_orders {
            let result = self.config.fill_model.simulate_tick_fill(&order, tick);

            if result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "SIMULATED_EXCHANGE".to_string(),
                    symbol: order.symbol.clone(),
                    exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}_{}", self.config.vt_symbol, self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: result.fill_price,
                    volume: result.fill_qty,
                    datetime: Some(clock.now()),
                    extra: None,
                };

                if let Err(e) = self.position.apply_fill(&trade) {
                    tracing::error!("仓位更新失败: {}", e);
                    continue;
                }

                trades.push(trade);

                let remaining = order.volume - order.traded - result.fill_qty;
                if remaining > 1e-10 {
                    if let Some(active_order) = self.active_limit_orders.get_mut(&vt_orderid) {
                        active_order.traded += result.fill_qty;
                    }
                } else {
                    to_remove.push(vt_orderid.clone());
                }
            }
        }

        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with tick data using FillModel.
    fn cross_stop_order_tick(
        &mut self,
        tick: &TickData,
        exchange: Exchange,
        clock: &dyn Clock,
        trades: &mut Vec<TradeData>,
    ) {
        let mut to_trigger = Vec::new();

        for (stop_orderid, stop_order) in self.active_stop_orders.iter() {
            let should_trigger = match stop_order.direction {
                Direction::Long => tick.last_price >= stop_order.price,
                Direction::Short => tick.last_price <= stop_order.price,
                _ => false,
            };

            if should_trigger {
                to_trigger.push((stop_orderid.clone(), stop_order.clone()));
            }
        }

        for (stop_orderid, mut stop_order) in to_trigger {
            stop_order.status = StopOrderStatus::Triggered;

            let symbol = self.config.vt_symbol.split('.').next().unwrap_or("").to_string();
            let order = OrderData {
                gateway_name: "SIMULATED_EXCHANGE".to_string(),
                symbol,
                exchange,
                orderid: stop_orderid.clone(),
                order_type: stop_order.order_type,
                direction: Some(stop_order.direction),
                offset: stop_order.offset.unwrap_or(Offset::Open),
                price: stop_order.limit_price.unwrap_or(stop_order.price),
                volume: stop_order.volume,
                traded: 0.0,
                status: Status::NotTraded,
                datetime: Some(tick.datetime),
                reference: String::new(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
                extra: None,
            };

            let fill_result = self.config.fill_model.simulate_tick_fill(&order, tick);

            if fill_result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "SIMULATED_EXCHANGE".to_string(),
                    symbol: order.symbol.clone(),
                    exchange,
                    orderid: stop_orderid.clone(),
                    tradeid: format!("{}_{}", self.config.vt_symbol, self.trade_count),
                    direction: Some(stop_order.direction),
                    offset: stop_order.offset.unwrap_or(Offset::Open),
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,
                    datetime: Some(clock.now()),
                    extra: None,
                };

                if let Err(e) = self.position.apply_fill(&trade) {
                    tracing::error!("仓位更新失败: {}", e);
                } else {
                    trades.push(trade);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            } else {
                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            }
        }
    }

    /// Get active limit orders.
    pub fn active_limit_orders(&self) -> &HashMap<String, OrderData> {
        &self.active_limit_orders
    }

    /// Get active stop orders.
    pub fn active_stop_orders(&self) -> &HashMap<String, StopOrder> {
        &self.active_stop_orders
    }

    /// Get position.
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Get mutable position.
    pub fn position_mut(&mut self) -> &mut Position {
        &mut self.position
    }

    /// Get trade count.
    pub fn trade_count(&self) -> u64 {
        self.trade_count
    }
}

// ============================================================================
// Simulated Exchange
// ============================================================================

/// Top-level simulated exchange that routes orders to per-instrument matching engines.
///
/// The SimulatedExchange:
/// - Maintains a collection of InstrumentMatchingEngines, one per instrument
/// - Routes order submissions/cancellations to the correct engine by vt_symbol
/// - Applies pre-trade risk checks via RiskEngine before submitting orders
/// - Applies fee calculation using per-instrument or default fee models
/// - Processes market data (bars/ticks) across all instruments
pub struct SimulatedExchange {
    /// Exchange name (e.g., "SIMULATED_BINANCE")
    name: String,
    /// Per-instrument matching engines, keyed by vt_symbol
    instruments: HashMap<String, InstrumentMatchingEngine>,
    /// Default fee model (used when instrument has no specific fee model)
    default_fee_model: Box<dyn FeeModel>,
    /// Latency model for order execution delays
    latency_model: Box<dyn LatencyModel>,
    /// Risk engine for pre-trade checks
    risk_engine: RiskEngine,
    /// Default contract size multiplier
    #[allow(dead_code)]
    default_size: f64,
}

impl fmt::Debug for SimulatedExchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimulatedExchange")
            .field("name", &self.name)
            .field("instruments", &self.instruments.len())
            .field("default_fee_model", &self.default_fee_model.name())
            .field("latency_model", &format!("{}ms", self.latency_model.latency_ms()))
            .finish()
    }
}

impl SimulatedExchange {
    /// Create a new simulated exchange.
    pub fn new(name: String) -> Self {
        Self {
            name,
            instruments: HashMap::new(),
            default_fee_model: Box::new(NoFeeModel::new()),
            latency_model: Box::new(ZeroLatency::new()),
            risk_engine: RiskEngine::new_unrestricted(),
            default_size: 1.0,
        }
    }

    /// Create a new simulated exchange with custom fee and latency models.
    pub fn with_models(
        name: String,
        fee_model: Box<dyn FeeModel>,
        latency_model: Box<dyn LatencyModel>,
    ) -> Self {
        Self {
            name,
            instruments: HashMap::new(),
            default_fee_model: fee_model,
            latency_model,
            risk_engine: RiskEngine::new_unrestricted(),
            default_size: 1.0,
        }
    }

    /// Add an instrument to the exchange.
    pub fn add_instrument(&mut self, config: InstrumentConfig) {
        let vt_symbol = config.vt_symbol.clone();
        let engine = InstrumentMatchingEngine::new(config);
        self.instruments.insert(vt_symbol, engine);
    }

    /// Submit a limit order to the exchange.
    ///
    /// Routes to the correct InstrumentMatchingEngine by vt_symbol.
    /// Performs pre-trade risk check before submission.
    pub fn submit_order(
        &mut self,
        req: OrderRequest,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        clock: &dyn Clock,
    ) -> Result<OrderData, String> {
        let vt_symbol = format!("{}.{}", req.symbol, exchange.value());

        let instrument = self.instruments.get(&vt_symbol).ok_or_else(|| {
            format!("合约不存在: {}", vt_symbol)
        })?;

        // Create a temporary OrderData for risk check
        let temp_order = OrderData {
            gateway_name: self.name.clone(),
            symbol: req.symbol.clone(),
            exchange,
            orderid: String::new(),
            order_type: req.order_type,
            direction: Some(direction),
            offset,
            price: req.price,
            volume: req.volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: req.reference.clone(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        let active_count = instrument.active_limit_orders().len() + instrument.active_stop_orders().len();
        let risk_result = self.risk_engine.check_order(
            &temp_order,
            instrument.position(),
            active_count,
            instrument.config.size,
        );

        if !risk_result.is_approved {
            tracing::warn!(
                "风险检查拒绝: {} - {:?}",
                vt_symbol, risk_result.reason
            );
            return Err(format!(
                "风险检查拒绝: {}",
                risk_result.reason.unwrap_or_else(|| "未知原因".to_string())
            ));
        }

        let instrument = self.instruments.get_mut(&vt_symbol).ok_or_else(|| {
            format!("合约不存在: {}", vt_symbol)
        })?;

        instrument.submit_order(req, exchange, direction, offset, clock)
    }

    /// Submit a stop order to the exchange.
    pub fn submit_stop_order(
        &mut self,
        req: OrderRequest,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        clock: &dyn Clock,
    ) -> Result<StopOrder, String> {
        let vt_symbol = format!("{}.{}", req.symbol, exchange.value());

        if !self.instruments.contains_key(&vt_symbol) {
            return Err(format!("合约不存在: {}", vt_symbol));
        }

        let instrument = self.instruments.get_mut(&vt_symbol).ok_or_else(|| {
            format!("合约不存在: {}", vt_symbol)
        })?;

        instrument.submit_stop_order(req, direction, offset, clock)
    }

    /// Cancel an order by vt_orderid.
    ///
    /// Searches across all instruments since we may not know which one holds the order.
    pub fn cancel_order(&mut self, vt_orderid: &str) -> Result<OrderData, String> {
        for instrument in self.instruments.values_mut() {
            if let Ok(order) = instrument.cancel_order(vt_orderid) {
                tracing::debug!("订单取消成功: {}", vt_orderid);
                return Ok(order);
            }
        }
        Err(format!("订单不存在: {}", vt_orderid))
    }

    /// Process a bar for a specific instrument.
    pub fn process_bar(
        &mut self,
        vt_symbol: &str,
        bar: &BarData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        if let Some(instrument) = self.instruments.get_mut(vt_symbol) {
            instrument.process_bar(bar, exchange, clock)
        } else {
            Vec::new()
        }
    }

    /// Process a bar for all instruments.
    pub fn process_bar_all(
        &mut self,
        bar: &BarData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        let mut all_trades = Vec::new();
        for instrument in self.instruments.values_mut() {
            let trades = instrument.process_bar(bar, exchange, clock);
            all_trades.extend(trades);
        }
        all_trades
    }

    /// Process a tick for a specific instrument.
    pub fn process_tick(
        &mut self,
        vt_symbol: &str,
        tick: &TickData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        if let Some(instrument) = self.instruments.get_mut(vt_symbol) {
            instrument.process_tick(tick, exchange, clock)
        } else {
            Vec::new()
        }
    }

    /// Process a tick for all instruments.
    pub fn process_tick_all(
        &mut self,
        tick: &TickData,
        exchange: Exchange,
        clock: &dyn Clock,
    ) -> Vec<TradeData> {
        let mut all_trades = Vec::new();
        for instrument in self.instruments.values_mut() {
            let trades = instrument.process_tick(tick, exchange, clock);
            all_trades.extend(trades);
        }
        all_trades
    }

    /// Set the default fee model.
    pub fn set_default_fee_model(&mut self, fee_model: Box<dyn FeeModel>) {
        self.default_fee_model = fee_model;
    }

    /// Set the latency model.
    pub fn set_latency_model(&mut self, latency_model: Box<dyn LatencyModel>) {
        self.latency_model = latency_model;
    }

    /// Set risk engine configuration.
    pub fn set_risk_config(&mut self, config: RiskConfig) {
        self.risk_engine = RiskEngine::new(config);
    }

    /// Get instrument matching engine by vt_symbol.
    pub fn get_instrument(&self, vt_symbol: &str) -> Option<&InstrumentMatchingEngine> {
        self.instruments.get(vt_symbol)
    }

    /// Get mutable instrument matching engine by vt_symbol.
    pub fn get_instrument_mut(&mut self, vt_symbol: &str) -> Option<&mut InstrumentMatchingEngine> {
        self.instruments.get_mut(vt_symbol)
    }

    /// Get exchange name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get reference to default fee model.
    pub fn default_fee_model(&self) -> &dyn FeeModel {
        self.default_fee_model.as_ref()
    }

    /// Get reference to latency model.
    pub fn latency_model(&self) -> &dyn LatencyModel {
        self.latency_model.as_ref()
    }

    /// Calculate fee for a trade using instrument-specific or default fee model.
    pub fn calculate_fee(
        &self,
        vt_symbol: &str,
        fill_price: f64,
        quantity: f64,
        direction: Direction,
        liquidity_side: LiquiditySide,
    ) -> f64 {
        if let Some(instrument) = self.instruments.get(vt_symbol) {
            if let Some(ref fee_model) = instrument.config.fee_model {
                return fee_model.calculate_fee(fill_price, quantity, direction, liquidity_side);
            }
        }
        self.default_fee_model.calculate_fee(fill_price, quantity, direction, liquidity_side)
    }

    /// Get reference to risk engine.
    pub fn risk_engine(&self) -> &RiskEngine {
        &self.risk_engine
    }

    /// Get mutable reference to risk engine.
    pub fn risk_engine_mut(&mut self) -> &mut RiskEngine {
        &mut self.risk_engine
    }

    /// Get all instrument vt_symbols.
    pub fn instrument_symbols(&self) -> Vec<String> {
        self.instruments.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtesting::fill_model::BestPriceFillModel;
    use crate::trader::{OrderType, TestClock};
    use chrono::{TimeZone, Utc};

    // === Test helpers ===

    fn create_test_clock() -> TestClock {
        TestClock::new(Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap())
    }

    fn create_bar(
        symbol: &str,
        exchange: Exchange,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> BarData {
        let dt = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        BarData {
            gateway_name: "TEST".to_string(),
            symbol: symbol.to_string(),
            exchange,
            datetime: dt,
            interval: None,
            volume: 1000.0,
            turnover: 0.0,
            open_interest: 0.0,
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            extra: None,
        }
    }

    fn create_tick(
        symbol: &str,
        exchange: Exchange,
        last_price: f64,
        bid: f64,
        ask: f64,
    ) -> TickData {
        let dt = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let mut tick = TickData::new("TEST".to_string(), symbol.to_string(), exchange, dt);
        tick.last_price = last_price;
        tick.bid_price_1 = bid;
        tick.ask_price_1 = ask;
        tick.high_price = last_price;
        tick.low_price = last_price;
        tick
    }

    fn make_order_req(
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        order_type: OrderType,
        volume: f64,
        price: f64,
    ) -> OrderRequest {
        OrderRequest {
            symbol: symbol.to_string(),
            exchange,
            direction,
            order_type,
            volume,
            price,
            offset: Offset::Open,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
        }
    }

    fn default_instrument_config(vt_symbol: &str) -> InstrumentConfig {
        InstrumentConfig::new(
            vt_symbol.to_string(),
            0.01,
            1.0,
            Box::new(BestPriceFillModel::new(0.0)),
        )
    }

    // === Fee Model Tests ===

    #[test]
    fn test_fee_model_maker_taker() {
        let model = MakerTakerFeeModel::new(0.001, 0.002);
        assert_eq!(model.name(), "MakerTakerFeeModel");

        let maker_fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::Maker);
        assert!((maker_fee - 1.0).abs() < 1e-10); // 100 * 10 * 0.001

        let taker_fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::Taker);
        assert!((taker_fee - 2.0).abs() < 1e-10); // 100 * 10 * 0.002

        let no_liq_fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::NoLiquidity);
        assert!((no_liq_fee - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_fee_model_flat() {
        let model = FlatFeeModel::new(5.0);
        assert_eq!(model.name(), "FlatFeeModel");

        let fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::Maker);
        assert!((fee - 5.0).abs() < 1e-10);

        let fee2 = model.calculate_fee(500.0, 2.0, Direction::Short, LiquiditySide::Taker);
        assert!((fee2 - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_fee_model_percent() {
        let model = PercentFeeModel::new(0.0005);
        assert_eq!(model.name(), "PercentFeeModel");

        let fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::Maker);
        assert!((fee - 0.5).abs() < 1e-10); // 100 * 10 * 0.0005
    }

    #[test]
    fn test_fee_model_no_fee() {
        let model = NoFeeModel::new();
        assert_eq!(model.name(), "NoFeeModel");

        let fee = model.calculate_fee(100.0, 10.0, Direction::Long, LiquiditySide::Maker);
        assert!((fee - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_fee_model_clone_box() {
        let model: Box<dyn FeeModel> = Box::new(MakerTakerFeeModel::binance_default());
        let cloned = model.clone();
        assert_eq!(cloned.name(), "MakerTakerFeeModel");
    }

    // === Latency Model Tests ===

    #[test]
    fn test_latency_model_zero() {
        let model = ZeroLatency::new();
        assert_eq!(model.latency_ms(), 0);
    }

    #[test]
    fn test_latency_model_fixed() {
        let model = FixedLatency::new(150);
        assert_eq!(model.latency_ms(), 150);
    }

    #[test]
    fn test_latency_model_random() {
        let model = RandomLatency::new(50, 150);
        // Returns midpoint
        assert_eq!(model.latency_ms(), 100);
    }

    #[test]
    fn test_latency_model_clone_box() {
        let model: Box<dyn LatencyModel> = Box::new(FixedLatency::new(100));
        let cloned = model.clone();
        assert_eq!(cloned.latency_ms(), 100);
    }

    // === Instrument Config Tests ===

    #[test]
    fn test_instrument_config_with_fee_model() {
        let config = InstrumentConfig::new(
            "BTCUSDT.BINANCE".to_string(),
            0.01,
            1.0,
            Box::new(BestPriceFillModel::new(0.0)),
        ).with_fee_model(Box::new(MakerTakerFeeModel::binance_default()));

        assert_eq!(config.vt_symbol, "BTCUSDT.BINANCE");
        assert!(config.fee_model.is_some());
    }

    // === InstrumentMatchingEngine Tests ===

    #[test]
    fn test_instrument_matching_engine_new() {
        let config = default_instrument_config("BTCUSDT.BINANCE");
        let engine = InstrumentMatchingEngine::new(config);

        assert_eq!(engine.config.vt_symbol, "BTCUSDT.BINANCE");
        assert!(engine.active_limit_orders().is_empty());
        assert!(engine.active_stop_orders().is_empty());
        assert!(engine.position().is_flat());
        assert_eq!(engine.trade_count(), 0);
    }

    #[test]
    fn test_instrument_matching_engine_limit_order_fill() {
        let config = default_instrument_config("BTCUSDT.BINANCE");
        let mut engine = InstrumentMatchingEngine::new(config);
        let clock = create_test_clock();

        // Submit buy limit at 50000
        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let order = engine.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);
        assert!(order.is_ok());
        assert_eq!(engine.active_limit_orders().len(), 1);

        // Process bar that crosses the limit price
        let bar = create_bar("BTCUSDT", Exchange::Binance, 49900.0, 50100.0, 49800.0, 50050.0);
        let trades = engine.process_bar(&bar, Exchange::Binance, &clock);

        assert_eq!(trades.len(), 1);
        assert!((trades[0].price - 50000.0).abs() < 1e-10);
        assert!((trades[0].volume - 1.0).abs() < 1e-10);
        assert!(engine.active_limit_orders().is_empty());
        assert!(engine.position().is_long());
    }

    #[test]
    fn test_instrument_matching_engine_stop_order_trigger() {
        let config = default_instrument_config("BTCUSDT.BINANCE");
        let mut engine = InstrumentMatchingEngine::new(config);
        let clock = create_test_clock();

        // Submit buy stop at 51000
        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Stop, 1.0, 51000.0,
        );
        let stop_order = engine.submit_stop_order(req, Direction::Long, Offset::Open, &clock);
        assert!(stop_order.is_ok());
        assert_eq!(engine.active_stop_orders().len(), 1);

        // Process bar that triggers the stop
        let bar = create_bar("BTCUSDT", Exchange::Binance, 50500.0, 51200.0, 50400.0, 51100.0);
        let trades = engine.process_bar(&bar, Exchange::Binance, &clock);

        assert_eq!(trades.len(), 1);
        assert!(engine.active_stop_orders().is_empty());
        assert!(engine.position().is_long());
    }

    #[test]
    fn test_instrument_matching_engine_cancel_order() {
        let config = default_instrument_config("BTCUSDT.BINANCE");
        let mut engine = InstrumentMatchingEngine::new(config);
        let clock = create_test_clock();

        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let order = engine.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock).unwrap();
        assert_eq!(engine.active_limit_orders().len(), 1);

        let result = engine.cancel_order(&order.orderid);
        assert!(result.is_ok());
        assert!(engine.active_limit_orders().is_empty());

        // Cancel again should fail
        let result2 = engine.cancel_order(&order.orderid);
        assert!(result2.is_err());
    }

    // === SimulatedExchange Tests ===

    #[test]
    fn test_simulated_exchange_new() {
        let exchange = SimulatedExchange::new("TEST_EXCHANGE".to_string());
        assert_eq!(exchange.name(), "TEST_EXCHANGE");
        assert!(exchange.instrument_symbols().is_empty());
        assert_eq!(exchange.default_fee_model().name(), "NoFeeModel");
        assert_eq!(exchange.latency_model().latency_ms(), 0);
    }

    #[test]
    fn test_simulated_exchange_add_instrument() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        let config = default_instrument_config("BTCUSDT.BINANCE");
        exchange.add_instrument(config);

        assert_eq!(exchange.instrument_symbols().len(), 1);
        assert!(exchange.get_instrument("BTCUSDT.BINANCE").is_some());
        assert!(exchange.get_instrument("ETHUSDT.BINANCE").is_none());
    }

    #[test]
    fn test_simulated_exchange_submit_order() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));
        let clock = create_test_clock();

        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let result = exchange.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);
        assert!(result.is_ok());

        let instrument = exchange.get_instrument("BTCUSDT.BINANCE").unwrap();
        assert_eq!(instrument.active_limit_orders().len(), 1);
    }

    #[test]
    fn test_simulated_exchange_submit_order_unknown_instrument() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        let clock = create_test_clock();

        let req = make_order_req(
            "ETHUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 3000.0,
        );
        let result = exchange.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);
        assert!(result.is_err());
    }

    #[test]
    fn test_simulated_exchange_process_bar_fill() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));
        let clock = create_test_clock();

        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let _ = exchange.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);

        let bar = create_bar("BTCUSDT", Exchange::Binance, 49900.0, 50100.0, 49800.0, 50050.0);
        let trades = exchange.process_bar("BTCUSDT.BINANCE", &bar, Exchange::Binance, &clock);

        assert_eq!(trades.len(), 1);
        let instrument = exchange.get_instrument("BTCUSDT.BINANCE").unwrap();
        assert!(instrument.position().is_long());
    }

    #[test]
    fn test_simulated_exchange_cancel_order() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));
        let clock = create_test_clock();

        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let order = exchange.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock).unwrap();

        let result = exchange.cancel_order(&order.orderid);
        assert!(result.is_ok());

        let instrument = exchange.get_instrument("BTCUSDT.BINANCE").unwrap();
        assert!(instrument.active_limit_orders().is_empty());

        // Cancel non-existent order
        let result2 = exchange.cancel_order("NONEXISTENT");
        assert!(result2.is_err());
    }

    #[test]
    fn test_simulated_exchange_risk_check_rejects() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));

        // Enable risk checks with very small max order size
        let risk_config = RiskConfig {
            max_order_size: 0.5,
            check_order_size: true,
            ..RiskConfig::default()
        };
        exchange.set_risk_config(risk_config);

        let clock = create_test_clock();
        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );

        // Order volume 1.0 exceeds max 0.5
        let result = exchange.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);
        assert!(result.is_err());
    }

    #[test]
    fn test_simulated_exchange_fee_calculation() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());

        // Add instrument with specific fee model
        let config = InstrumentConfig::new(
            "BTCUSDT.BINANCE".to_string(),
            0.01,
            1.0,
            Box::new(BestPriceFillModel::new(0.0)),
        ).with_fee_model(Box::new(MakerTakerFeeModel::new(0.001, 0.002)));
        exchange.add_instrument(config);

        // Use instrument-specific fee model
        let fee = exchange.calculate_fee(
            "BTCUSDT.BINANCE", 50000.0, 1.0, Direction::Long, LiquiditySide::Maker,
        );
        assert!((fee - 50.0).abs() < 1e-10); // 50000 * 1 * 0.001

        // Unknown instrument uses default (NoFeeModel)
        let fee2 = exchange.calculate_fee(
            "ETHUSDT.BINANCE", 3000.0, 1.0, Direction::Long, LiquiditySide::Maker,
        );
        assert!((fee2 - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_simulated_exchange_set_models() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());

        exchange.set_default_fee_model(Box::new(MakerTakerFeeModel::binance_default()));
        assert_eq!(exchange.default_fee_model().name(), "MakerTakerFeeModel");

        exchange.set_latency_model(Box::new(FixedLatency::new(200)));
        assert_eq!(exchange.latency_model().latency_ms(), 200);
    }

    #[test]
    fn test_simulated_exchange_process_bar_all() {
        let mut exchange = SimulatedExchange::new("TEST".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));
        exchange.add_instrument(default_instrument_config("ETHUSDT.BINANCE"));
        let clock = create_test_clock();

        // Submit orders for both instruments
        let req1 = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let _ = exchange.submit_order(req1, Exchange::Binance, Direction::Long, Offset::Open, &clock);

        let req2 = make_order_req(
            "ETHUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 3000.0,
        );
        let _ = exchange.submit_order(req2, Exchange::Binance, Direction::Long, Offset::Open, &clock);

        // Use separate bars for each instrument since they have different price levels
        let btc_bar = create_bar("BTCUSDT", Exchange::Binance, 49900.0, 50100.0, 49800.0, 50050.0);
        let btc_trades = exchange.process_bar("BTCUSDT.BINANCE", &btc_bar, Exchange::Binance, &clock);
        assert_eq!(btc_trades.len(), 1);

        let eth_bar = create_bar("ETHUSDT", Exchange::Binance, 2950.0, 3100.0, 2900.0, 3050.0);
        let eth_trades = exchange.process_bar("ETHUSDT.BINANCE", &eth_bar, Exchange::Binance, &clock);
        assert_eq!(eth_trades.len(), 1);

        // Verify both positions are long
        let btc_inst = exchange.get_instrument("BTCUSDT.BINANCE").unwrap();
        assert!(btc_inst.position().is_long());
        let eth_inst = exchange.get_instrument("ETHUSDT.BINANCE").unwrap();
        assert!(eth_inst.position().is_long());
    }

    #[test]
    fn test_simulated_exchange_debug_format() {
        let mut exchange = SimulatedExchange::new("TEST_EXCHANGE".to_string());
        exchange.add_instrument(default_instrument_config("BTCUSDT.BINANCE"));
        let debug_str = format!("{:?}", exchange);
        assert!(debug_str.contains("TEST_EXCHANGE"));
        assert!(debug_str.contains("instruments: 1"));
    }

    #[test]
    fn test_instrument_matching_engine_tick_fill() {
        let config = default_instrument_config("BTCUSDT.BINANCE");
        let mut engine = InstrumentMatchingEngine::new(config);
        let clock = create_test_clock();

        let req = make_order_req(
            "BTCUSDT", Exchange::Binance,
            Direction::Long, OrderType::Limit, 1.0, 50000.0,
        );
        let _ = engine.submit_order(req, Exchange::Binance, Direction::Long, Offset::Open, &clock);

        // Tick with bid >= order price should fill
        let tick = create_tick("BTCUSDT", Exchange::Binance, 50050.0, 50000.0, 50010.0);
        let trades = engine.process_tick(&tick, Exchange::Binance, &clock);

        assert_eq!(trades.len(), 1);
        assert!(engine.position().is_long());
    }

    #[test]
    fn test_maker_taker_fee_model_binance_default() {
        let model = MakerTakerFeeModel::binance_default();
        let fee = model.calculate_fee(10000.0, 1.0, Direction::Long, LiquiditySide::Taker);
        assert!((fee - 10.0).abs() < 1e-10); // 10000 * 1 * 0.001
    }
}
