//! Trading Environment for Reinforcement Learning
//!
//! `TradingEnv` wraps the `BacktestingEngine` to provide a Gym-compatible
//! interface for RL agents. Each episode runs through historical data one
//! bar at a time, with the agent's actions mapped to orders via `ActionMapper`.
//!
//! # Interface
//!
//! - `reset()` → `Observation`
//! - `step(action)` → `(Observation, f64, bool, StepInfo)`
//!
//! The environment follows the Gymnasium convention:
//! - `terminated = true` when the episode ends naturally (data exhausted)
//! - `truncated = true` when a termination condition triggers (margin call, max steps)
//!
//! # Example
//!
//! ```rust,ignore
//! use trade_engine::rl::TradingEnv;
//! use trade_engine::rl::action::{DiscreteActionMapper, ActionValue};
//! use trade_engine::rl::reward::PnlReward;
//!
//! let env = TradingEnv::builder()
//!     .initial_capital(100_000.0)
//!     .action_mapper(Box::new(DiscreteActionMapper::new(...)))
//!     .reward_fn(Box::new(PnlReward::new()))
//!     .bars(historical_bars)
//!     .build();
//!
//! let obs = env.reset();
//! let (obs, reward, done, info) = env.step(&ActionValue::Discrete(1));
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::trader::{BarData, OrderRequest, Direction, Exchange, OrderType};

use super::action::{ActionMapper, ActionValue};
use super::info::{StepInfo, EpisodeInfo, TerminationReason};
use super::observation::{
    Observation, ObservationBuilder, ObservationSpace, PortfolioSnapshot, MarketSnapshot,
};
use super::reward::RewardFunction;

/// Configuration for the trading environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingEnvConfig {
    /// Initial capital
    pub initial_capital: f64,
    /// Symbol to trade
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Contract size multiplier
    pub size: f64,
    /// Price tick (minimum price movement)
    pub pricetick: f64,
    /// Commission rate
    pub rate: f64,
    /// Slippage per unit
    pub slippage: f64,
    /// Maximum steps per episode (0 = unlimited)
    pub max_steps: usize,
    /// Minimum equity before margin call (0 = no margin call)
    pub min_equity: f64,
    /// Number of lookback bars for feature construction
    pub lookback: usize,
}

impl Default for TradingEnvConfig {
    fn default() -> Self {
        Self {
            initial_capital: 1_000_000.0,
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            size: 1.0,
            pricetick: 0.01,
            rate: 0.001,
            slippage: 0.0,
            max_steps: 0,
            min_equity: 0.0,
            lookback: 20,
        }
    }
}

/// Result of a single step in the environment.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// New observation after the step
    pub observation: Observation,
    /// Reward for this step
    pub reward: f64,
    /// Whether the episode is terminated (natural end)
    pub terminated: bool,
    /// Whether the episode is truncated (forced end)
    pub truncated: bool,
    /// Diagnostic info
    pub info: StepInfo,
}

/// The trading environment for reinforcement learning.
///
/// Wraps a `BacktestingEngine` and provides a step-based interface
/// where each step processes one bar of data.
pub struct TradingEnv {
    /// Configuration
    config: TradingEnvConfig,
    /// Historical bar data for the episode
    bars: Vec<BarData>,
    /// Current bar index
    current_step: usize,
    /// Action mapper
    action_mapper: Box<dyn ActionMapper>,
    /// Reward function
    reward_fn: Box<dyn RewardFunction>,
    /// Previous portfolio snapshot (for reward computation)
    prev_portfolio: PortfolioSnapshot,
    /// Current portfolio snapshot
    curr_portfolio: PortfolioSnapshot,
    /// Cumulative reward for the episode
    total_reward: f64,
    /// Per-step rewards
    step_rewards: Vec<f64>,
    /// Episode info (populated at end)
    episode_info: Option<EpisodeInfo>,
    /// Whether the episode is active
    episode_active: bool,
    /// Observation space (cached)
    observation_space: ObservationSpace,
    /// Total commission accumulated
    total_commission: f64,
    /// Total slippage accumulated
    total_slippage: f64,
    /// Total trades
    total_trades: usize,
}

impl TradingEnv {
    /// Create a new TradingEnv with the given configuration.
    pub fn new(
        config: TradingEnvConfig,
        bars: Vec<BarData>,
        action_mapper: Box<dyn ActionMapper>,
        reward_fn: Box<dyn RewardFunction>,
    ) -> Self {
        let initial_capital = config.initial_capital;
        let portfolio = PortfolioSnapshot::new(initial_capital);
        let lookback = config.lookback;
        
        // Build observation space with feature names
        let feature_names = Self::build_feature_names(lookback);
        let observation_space = ObservationSpace::with_names(feature_names);

        Self {
            config,
            bars,
            current_step: 0,
            action_mapper,
            reward_fn,
            prev_portfolio: portfolio.clone(),
            curr_portfolio: portfolio,
            total_reward: 0.0,
            step_rewards: Vec::new(),
            episode_info: None,
            episode_active: false,
            observation_space,
            total_commission: 0.0,
            total_slippage: 0.0,
            total_trades: 0,
        }
    }

    /// Create a builder for constructing a TradingEnv.
    pub fn builder() -> TradingEnvBuilder {
        TradingEnvBuilder::default()
    }

    /// Reset the environment to the start of a new episode.
    ///
    /// Returns the initial observation.
    pub fn reset(&mut self) -> Observation {
        self.current_step = 0;
        let initial_capital = self.config.initial_capital;
        let portfolio = PortfolioSnapshot::new(initial_capital);
        self.prev_portfolio = portfolio.clone();
        self.curr_portfolio = portfolio;
        self.total_reward = 0.0;
        self.step_rewards.clear();
        self.episode_info = None;
        self.episode_active = true;
        self.total_commission = 0.0;
        self.total_slippage = 0.0;
        self.total_trades = 0;

        // Reset reward function internal state
        self.reward_fn.reset();

        // Build initial observation
        if self.bars.is_empty() {
            // No data — return a zero observation
            return self.build_observation(0, &PortfolioSnapshot::new(initial_capital));
        }

        self.build_observation(0, &self.curr_portfolio)
    }

    /// Take a step in the environment.
    ///
    /// # Arguments
    /// * `action` - The action to take
    ///
    /// # Returns
    /// A `StepResult` containing the new observation, reward, termination flags, and info.
    pub fn step(&mut self, action: &ActionValue) -> Result<StepResult, String> {
        if !self.episode_active {
            return Err("Episode is not active. Call reset() first.".to_string());
        }

        if self.bars.is_empty() {
            return Err("No bar data available. Set bars before stepping.".to_string());
        }

        // Save previous portfolio state for reward computation
        self.prev_portfolio = self.curr_portfolio.clone();

        // Map action to orders
        let orders = self.action_mapper.map_action(action);
        let orders_generated = orders.len();

        // Simulate the step: advance one bar and compute portfolio changes
        let current_bar_idx = self.current_step;
        let next_bar_idx = current_bar_idx + 1;

        // Clone the bar to avoid borrow issues
        let bar = self.bars[current_bar_idx].clone();

        // Process orders against the current bar
        let orders_filled = self.simulate_orders(&orders, &bar);
        let num_filled = orders_filled.len();

        // Update portfolio based on price changes
        self.update_portfolio(&bar, &orders_filled);

        // Advance step counter
        self.current_step = next_bar_idx;

        // Compute reward
        let reward = self.reward_fn.compute(
            &self.prev_portfolio,
            &self.curr_portfolio,
            action,
        );
        self.total_reward += reward;
        self.step_rewards.push(reward);

        // Check termination conditions
        let (terminated, truncated, termination_reason) = self.check_termination();

        // Build step info
        let step_pnl = if self.prev_portfolio.equity.abs() > f64::EPSILON {
            self.curr_portfolio.equity - self.prev_portfolio.equity
        } else {
            0.0
        };
        let net_pnl = step_pnl - self.total_commission - self.total_slippage;

        let info = StepInfo {
            orders_generated,
            orders_filled: num_filled,
            commission: self.config.rate * step_pnl.abs(),
            slippage: 0.0,
            step_pnl,
            net_pnl,
            drawdown: self.curr_portfolio.current_drawdown(),
            position_changed: self.curr_portfolio.position_qty != self.prev_portfolio.position_qty,
            warnings: Vec::new(),
        };

        // Build observation for next step (or repeat last if terminated)
        let obs_bar_idx = if next_bar_idx < self.bars.len() {
            next_bar_idx
        } else {
            current_bar_idx
        };
        let observation = self.build_observation(obs_bar_idx, &self.curr_portfolio);

        // If episode is done, populate episode info
        if terminated || truncated {
            self.episode_active = false;
            self.episode_info = Some(EpisodeInfo {
                total_steps: self.current_step,
                total_reward: self.total_reward,
                final_portfolio: self.curr_portfolio.clone(),
                max_drawdown: self.curr_portfolio.max_drawdown,
                total_commission: self.total_commission,
                total_slippage: self.total_slippage,
                total_trades: self.total_trades,
                termination_reason,
                rewards: self.step_rewards.clone(),
            });
        }

        Ok(StepResult {
            observation,
            reward,
            terminated,
            truncated,
            info,
        })
    }

    /// Check if the episode is done.
    pub fn is_done(&self) -> bool {
        !self.episode_active
    }

    /// Get the episode info (only available after episode ends).
    pub fn episode_info(&self) -> Option<&EpisodeInfo> {
        self.episode_info.as_ref()
    }

    /// Get the current step number.
    pub fn current_step(&self) -> usize {
        self.current_step
    }

    /// Get the observation space.
    pub fn observation_space(&self) -> &ObservationSpace {
        &self.observation_space
    }

    /// Get the action space.
    pub fn action_space(&self) -> super::action::ActionSpace {
        self.action_mapper.action_space()
    }

    /// Set historical bar data (can be called between episodes).
    pub fn set_bars(&mut self, bars: Vec<BarData>) {
        self.bars = bars;
    }

    /// Get the current portfolio snapshot.
    pub fn portfolio(&self) -> &PortfolioSnapshot {
        &self.curr_portfolio
    }

    // ==================== Private Methods ====================

    /// Simulate order fills against a bar.
    ///
    /// Returns a list of (direction, price, volume) for filled orders.
    fn simulate_orders(&mut self, orders: &[OrderRequest], bar: &BarData) -> Vec<(Direction, f64, f64)> {
        let mut fills = Vec::new();

        for order in orders {
            // Simple fill model: market orders fill at close price
            // Limit orders would need more sophisticated logic
            match order.order_type {
                OrderType::Market => {
                    // Market order: fill at close price with slippage
                    let fill_price = match order.direction {
                        Direction::Long => bar.close_price * (1.0 + self.config.slippage),
                        Direction::Short => bar.close_price * (1.0 - self.config.slippage),
                        Direction::Net => bar.close_price,
                    };

                    // Compute commission
                    let commission = fill_price * order.volume * self.config.size * self.config.rate;
                    self.total_commission += commission;

                    fills.push((order.direction, fill_price, order.volume));
                    self.total_trades += 1;
                }
                _ => {
                    // For other order types, skip in this simplified model
                    // A full implementation would check limit/stop conditions
                }
            }
        }

        fills
    }

    /// Update portfolio state based on fills and price changes.
    fn update_portfolio(&mut self, bar: &BarData, fills: &[(Direction, f64, f64)]) {
        // Apply fills to position
        for (direction, price, volume) in fills {
            match direction {
                Direction::Long => {
                    // Opening or adding to long position
                    let cost = price * volume * self.config.size;
                    if self.curr_portfolio.position_qty < 0.0 {
                        // Closing short position first
                        let close_qty = volume.min(self.curr_portfolio.position_qty.abs());
                        let realized = close_qty * (self.curr_portfolio.avg_entry_price - price) * self.config.size;
                        self.curr_portfolio.realized_pnl += realized;
                        self.curr_portfolio.cash += close_qty * self.curr_portfolio.avg_entry_price * self.config.size;
                        
                        let remaining = volume - close_qty;
                        if remaining > f64::EPSILON {
                            // Open new long with remaining
                            self.curr_portfolio.position_qty = remaining;
                            self.curr_portfolio.avg_entry_price = *price;
                            self.curr_portfolio.position_direction = Some(Direction::Long);
                            self.curr_portfolio.cash -= remaining * price * self.config.size;
                        } else {
                            let new_qty = self.curr_portfolio.position_qty + volume;
                            if new_qty.abs() < f64::EPSILON {
                                self.curr_portfolio.position_qty = 0.0;
                                self.curr_portfolio.position_direction = None;
                                self.curr_portfolio.avg_entry_price = 0.0;
                            } else {
                                self.curr_portfolio.position_qty = new_qty;
                            }
                        }
                    } else {
                        // Adding to long or opening new long
                        let old_qty = self.curr_portfolio.position_qty;
                        let new_qty = old_qty + volume;
                        if new_qty > f64::EPSILON {
                            self.curr_portfolio.avg_entry_price = if old_qty > f64::EPSILON {
                                (self.curr_portfolio.avg_entry_price * old_qty + price * volume) / new_qty
                            } else {
                                *price
                            };
                            self.curr_portfolio.position_qty = new_qty;
                            self.curr_portfolio.position_direction = Some(Direction::Long);
                        }
                        self.curr_portfolio.cash -= cost;
                    }
                }
                Direction::Short => {
                    // Opening or adding to short position
                    let proceeds = price * volume * self.config.size;
                    if self.curr_portfolio.position_qty > 0.0 {
                        // Closing long position first
                        let close_qty = volume.min(self.curr_portfolio.position_qty);
                        let realized = close_qty * (price - self.curr_portfolio.avg_entry_price) * self.config.size;
                        self.curr_portfolio.realized_pnl += realized;
                        self.curr_portfolio.cash += close_qty * price * self.config.size;
                        
                        let remaining = volume - close_qty;
                        if remaining > f64::EPSILON {
                            // Open new short with remaining
                            self.curr_portfolio.position_qty = -remaining;
                            self.curr_portfolio.avg_entry_price = *price;
                            self.curr_portfolio.position_direction = Some(Direction::Short);
                            self.curr_portfolio.cash += remaining * price * self.config.size;
                        } else {
                            let new_qty = self.curr_portfolio.position_qty - volume;
                            if new_qty.abs() < f64::EPSILON {
                                self.curr_portfolio.position_qty = 0.0;
                                self.curr_portfolio.position_direction = None;
                                self.curr_portfolio.avg_entry_price = 0.0;
                            } else {
                                self.curr_portfolio.position_qty = new_qty;
                            }
                        }
                    } else {
                        // Adding to short or opening new short
                        let old_qty = self.curr_portfolio.position_qty.abs();
                        let new_qty = old_qty + volume;
                        if new_qty > f64::EPSILON {
                            self.curr_portfolio.avg_entry_price = if old_qty > f64::EPSILON {
                                (self.curr_portfolio.avg_entry_price * old_qty + price * volume) / new_qty
                            } else {
                                *price
                            };
                            self.curr_portfolio.position_qty = -new_qty;
                            self.curr_portfolio.position_direction = Some(Direction::Short);
                        }
                        self.curr_portfolio.cash += proceeds;
                    }
                }
                Direction::Net => {
                    // Net direction — not typically used in this context
                }
            }
        }

        // Mark-to-market: update position value and unrealized PnL
        let position_value = self.curr_portfolio.position_qty * bar.close_price * self.config.size;
        let unrealized_pnl = if self.curr_portfolio.position_qty.abs() > f64::EPSILON {
            (bar.close_price - self.curr_portfolio.avg_entry_price) * self.curr_portfolio.position_qty * self.config.size
        } else {
            0.0
        };

        self.curr_portfolio.position_value = position_value;
        self.curr_portfolio.unrealized_pnl = unrealized_pnl;
        self.curr_portfolio.equity = self.curr_portfolio.cash + position_value + unrealized_pnl;
        self.curr_portfolio.update_equity(self.curr_portfolio.equity);
        self.curr_portfolio.timestamp = bar.datetime;
    }

    /// Check if the episode should terminate.
    fn check_termination(&self) -> (bool, bool, TerminationReason) {
        // Data exhausted
        if self.current_step >= self.bars.len() {
            return (true, false, TerminationReason::DataExhausted);
        }

        // Max steps reached
        if self.config.max_steps > 0 && self.current_step >= self.config.max_steps {
            return (false, true, TerminationReason::MaxStepsReached);
        }

        // Margin call
        if self.config.min_equity > 0.0 && self.curr_portfolio.equity < self.config.min_equity {
            return (true, false, TerminationReason::MarginCall);
        }

        (false, false, TerminationReason::DataExhausted)
    }

    /// Build an observation for the current state.
    fn build_observation(&self, bar_idx: usize, portfolio: &PortfolioSnapshot) -> Observation {
        let bar = if bar_idx < self.bars.len() {
            &self.bars[bar_idx]
        } else if !self.bars.is_empty() {
            &self.bars[self.bars.len() - 1]
        } else {
            // No data at all — return a default bar
            return Observation::new(
                vec![0.0; self.observation_space.dim.max(1)],
                portfolio.clone(),
                MarketSnapshot {
                    symbol: self.config.symbol.clone(),
                    exchange: self.config.exchange,
                    datetime: Utc::now(),
                    open_price: 0.0,
                    high_price: 0.0,
                    low_price: 0.0,
                    close_price: 0.0,
                    volume: 0.0,
                    turnover: 0.0,
                },
            );
        };

        let market = MarketSnapshot::from_bar(bar);

        // Build feature vector
        let lookback = self.config.lookback;
        let mut builder = ObservationBuilder::new(50)
            .market_data(market.clone())
            .portfolio(portfolio.clone());

        // Current bar features (normalized)
        builder = builder
            .add_named_feature("close_pct", bar.close_price / 10000.0)
            .add_named_feature("volume_norm", bar.volume / 1000.0)
            .add_named_feature("returns", if bar_idx > 0 {
                let prev_close = self.bars[bar_idx - 1].close_price;
                if prev_close > 0.0 { bar.close_price / prev_close - 1.0 } else { 0.0 }
            } else { 0.0 })
            .add_named_feature("volatility", if bar.close_price > 0.0 {
                (bar.high_price - bar.low_price) / bar.close_price
            } else { 0.0 });

        // Portfolio features
        builder = builder
            .add_named_feature("position_qty_norm", portfolio.position_qty / 100.0)
            .add_named_feature("equity_norm", portfolio.equity / self.config.initial_capital)
            .add_named_feature("unrealized_pnl_norm", portfolio.unrealized_pnl / self.config.initial_capital)
            .add_named_feature("drawdown", portfolio.current_drawdown());

        // Lookback features: returns from previous bars (padded to fixed size)
        let start_idx = if bar_idx >= lookback { bar_idx - lookback } else { 0 };
        for i in start_idx..bar_idx {
            let prev_bar = &self.bars[i];
            let prev_close = if i > 0 { self.bars[i - 1].close_price } else { prev_bar.close_price };
            let ret = if prev_close > 0.0 {
                prev_bar.close_price / prev_close - 1.0
            } else {
                0.0
            };
            builder = builder.add_feature(ret);
        }

        // Pad lookback features to fixed size (fill with 0.0 for missing bars)
        let lookback_added = bar_idx.saturating_sub(start_idx);
        if lookback_added < lookback {
            for _ in lookback_added..lookback {
                builder = builder.add_feature(0.0);
            }
        }

        builder.build().unwrap_or_else(|_| {
            Observation::new(
                vec![0.0; 9],
                portfolio.clone(),
                market,
            )
        })
    }

    /// Build feature names for the observation space.
    fn build_feature_names(lookback: usize) -> Vec<String> {
        let mut names = Vec::with_capacity(50);
        names.extend_from_slice(&[
            "close_pct".to_string(),
            "volume_norm".to_string(),
            "returns".to_string(),
            "volatility".to_string(),
            "position_qty_norm".to_string(),
            "equity_norm".to_string(),
            "unrealized_pnl_norm".to_string(),
            "drawdown".to_string(),
        ]);
        for i in 0..lookback {
            names.push(format!("lookback_return_{}", i));
        }
        names
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for constructing a `TradingEnv`.
pub struct TradingEnvBuilder {
    config: TradingEnvConfig,
    bars: Vec<BarData>,
    action_mapper: Option<Box<dyn ActionMapper>>,
    reward_fn: Option<Box<dyn RewardFunction>>,
}

impl Default for TradingEnvBuilder {
    fn default() -> Self {
        Self {
            config: TradingEnvConfig::default(),
            bars: Vec::new(),
            action_mapper: None,
            reward_fn: None,
        }
    }
}

impl TradingEnvBuilder {
    /// Set the initial capital.
    pub fn initial_capital(mut self, capital: f64) -> Self {
        self.config.initial_capital = capital;
        self
    }

    /// Set the trading symbol.
    pub fn symbol(mut self, symbol: String) -> Self {
        self.config.symbol = symbol;
        self
    }

    /// Set the exchange.
    pub fn exchange(mut self, exchange: Exchange) -> Self {
        self.config.exchange = exchange;
        self
    }

    /// Set contract size multiplier.
    pub fn size(mut self, size: f64) -> Self {
        self.config.size = size;
        self
    }

    /// Set commission rate.
    pub fn rate(mut self, rate: f64) -> Self {
        self.config.rate = rate;
        self
    }

    /// Set slippage.
    pub fn slippage(mut self, slippage: f64) -> Self {
        self.config.slippage = slippage;
        self
    }

    /// Set maximum steps per episode.
    pub fn max_steps(mut self, max_steps: usize) -> Self {
        self.config.max_steps = max_steps;
        self
    }

    /// Set minimum equity before margin call.
    pub fn min_equity(mut self, min_equity: f64) -> Self {
        self.config.min_equity = min_equity;
        self
    }

    /// Set lookback window size.
    pub fn lookback(mut self, lookback: usize) -> Self {
        self.config.lookback = lookback;
        self
    }

    /// Set the action mapper.
    pub fn action_mapper(mut self, mapper: Box<dyn ActionMapper>) -> Self {
        self.action_mapper = Some(mapper);
        self
    }

    /// Set the reward function.
    pub fn reward_fn(mut self, reward_fn: Box<dyn RewardFunction>) -> Self {
        self.reward_fn = Some(reward_fn);
        self
    }

    /// Set the historical bar data.
    pub fn bars(mut self, bars: Vec<BarData>) -> Self {
        self.bars = bars;
        self
    }

    /// Build the TradingEnv.
    ///
    /// Returns an error if required components are missing.
    pub fn build(self) -> Result<TradingEnv, String> {
        let action_mapper = self.action_mapper.ok_or_else(|| {
            "action_mapper is required".to_string()
        })?;
        let reward_fn = self.reward_fn.ok_or_else(|| {
            "reward_fn is required".to_string()
        })?;

        Ok(TradingEnv::new(
            self.config,
            self.bars,
            action_mapper,
            reward_fn,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::action::DiscreteActionMapper;
    use crate::rl::reward::PnlReward;
    use crate::trader::Interval;
    use chrono::{Duration, Utc};

    fn make_bar(symbol: &str, close: f64, volume: f64, offset_hours: i64) -> BarData {
        BarData {
            gateway_name: "test".to_string(),
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now() + Duration::hours(offset_hours),
            interval: Some(Interval::Minute),
            open_price: close - 10.0,
            high_price: close + 20.0,
            low_price: close - 20.0,
            close_price: close,
            volume,
            turnover: close * volume,
            open_interest: 0.0,
            extra: None,
        }
    }

    fn make_test_bars(n: usize) -> Vec<BarData> {
        let mut bars = Vec::with_capacity(n);
        for i in 0..n {
            let price = 42000.0 + (i as f64) * 10.0;
            bars.push(make_bar("BTCUSDT", price, 100.0, i as i64));
        }
        bars
    }

    fn make_env(bars: Vec<BarData>) -> TradingEnv {
        TradingEnv::builder()
            .initial_capital(100_000.0)
            .symbol("BTCUSDT".to_string())
            .exchange(Exchange::Binance)
            .action_mapper(Box::new(DiscreteActionMapper::new(
                "BTCUSDT".to_string(),
                Exchange::Binance,
                1.0,
            )))
            .reward_fn(Box::new(PnlReward::new()))
            .bars(bars)
            .build()
            .expect("should build env")
    }

    #[test]
    fn test_env_reset() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        let obs = env.reset();
        assert!(!obs.features.is_empty());
        assert_eq!(env.current_step(), 0);
        assert!(!env.is_done());
    }

    #[test]
    fn test_env_step_hold() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        env.reset();

        let result = env.step(&ActionValue::Discrete(0)).expect("step should succeed");
        assert!(!result.terminated);
        assert_eq!(env.current_step(), 1);
    }

    #[test]
    fn test_env_step_buy() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        env.reset();

        let result = env.step(&ActionValue::Discrete(1)).expect("step should succeed");
        assert_eq!(result.info.orders_generated, 1);
        assert!(!result.terminated);
    }

    #[test]
    fn test_env_step_sell() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        env.reset();

        let result = env.step(&ActionValue::Discrete(2)).expect("step should succeed");
        assert_eq!(result.info.orders_generated, 1);
        assert_eq!(result.observation.features.len(), env.observation_space().dim);
    }

    #[test]
    fn test_env_episode_completes() {
        let bars = make_test_bars(5);
        let mut env = make_env(bars);
        env.reset();

        let mut steps = 0;
        loop {
            let result = env.step(&ActionValue::Discrete(0)).expect("step should succeed");
            steps += 1;
            if result.terminated || result.truncated {
                break;
            }
            if steps > 100 {
                break;
            }
        }

        assert!(env.is_done());
        assert!(env.episode_info().is_some());
        let info = env.episode_info().expect("should have episode info");
        assert_eq!(info.termination_reason, TerminationReason::DataExhausted);
    }

    #[test]
    fn test_env_max_steps() {
        let bars = make_test_bars(100);
        let mut env = TradingEnv::builder()
            .initial_capital(100_000.0)
            .max_steps(5)
            .action_mapper(Box::new(DiscreteActionMapper::new(
                "BTCUSDT".to_string(),
                Exchange::Binance,
                1.0,
            )))
            .reward_fn(Box::new(PnlReward::new()))
            .bars(bars)
            .build()
            .expect("should build env");

        env.reset();

        for _ in 0..4 {
            let result = env.step(&ActionValue::Discrete(0)).expect("step should succeed");
            assert!(!result.terminated);
            assert!(!result.truncated);
        }

        let result = env.step(&ActionValue::Discrete(0)).expect("step should succeed");
        assert!(result.truncated);
        assert_eq!(
            env.episode_info().map(|i| i.termination_reason),
            Some(TerminationReason::MaxStepsReached)
        );
    }

    #[test]
    fn test_env_step_after_done_fails() {
        let bars = make_test_bars(3);
        let mut env = make_env(bars);
        env.reset();

        // Run until done
        while !env.is_done() {
            let _ = env.step(&ActionValue::Discrete(0));
        }

        let result = env.step(&ActionValue::Discrete(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_env_reset_after_episode() {
        let bars = make_test_bars(5);
        let mut env = make_env(bars);
        
        let _obs1 = env.reset();
        assert_eq!(env.current_step(), 0);
        
        // Run until done
        while !env.is_done() {
            let _ = env.step(&ActionValue::Discrete(0));
        }
        
        // Reset for new episode
        let _obs2 = env.reset();
        assert!(!env.is_done());
        assert_eq!(env.current_step(), 0);
    }

    #[test]
    fn test_env_builder_missing_mapper() {
        let result = TradingEnv::builder()
            .reward_fn(Box::new(PnlReward::new()))
            .bars(make_test_bars(10))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_env_builder_missing_reward() {
        let result = TradingEnv::builder()
            .action_mapper(Box::new(DiscreteActionMapper::new(
                "BTCUSDT".to_string(),
                Exchange::Binance,
                1.0,
            )))
            .bars(make_test_bars(10))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_env_observation_space() {
        let bars = make_test_bars(10);
        let env = make_env(bars);
        let space = env.observation_space();
        assert!(space.dim > 0);
    }

    #[test]
    fn test_env_action_space() {
        let bars = make_test_bars(10);
        let env = make_env(bars);
        let space = env.action_space();
        match space {
            super::super::action::ActionSpace::Discrete { n } => assert_eq!(n, 4),
            _ => panic!("expected Discrete"),
        }
    }

    #[test]
    fn test_env_portfolio_tracking() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        env.reset();

        let portfolio = env.portfolio();
        assert_eq!(portfolio.equity, 100_000.0);
        assert!(portfolio.is_flat());
    }

    #[test]
    fn test_env_set_bars() {
        let mut env = make_env(Vec::new());
        let new_bars = make_test_bars(10);
        env.set_bars(new_bars);
        // Verify env can now reset and step
        let obs = env.reset();
        assert!(!obs.features.is_empty());
    }

    #[test]
    fn test_step_result_fields() {
        let bars = make_test_bars(10);
        let mut env = make_env(bars);
        env.reset();

        let result = env.step(&ActionValue::Discrete(0)).expect("step should succeed");
        // Check that StepResult has all expected fields
        assert!(result.observation.features.len() > 0);
        // reward should be a finite number
        assert!(result.reward.is_finite());
    }
}
