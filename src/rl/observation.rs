//! Observation Space and Portfolio Snapshot
//!
//! Provides the `Observation` struct that represents the agent's view of the
//! environment at each step, along with `PortfolioSnapshot` for capturing
//! the current portfolio state, and `ObservationBuilder` for constructing
//! observations incrementally.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::trader::{BarData, TickData, Exchange, Direction};

/// Snapshot of the portfolio state at a point in time.
///
/// Captures all relevant portfolio information needed for:
/// - Observation construction
/// - Reward computation
/// - Risk metrics calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    /// Total portfolio value (cash + positions at market value)
    pub equity: f64,
    /// Cash available for trading
    pub cash: f64,
    /// Market value of positions
    pub position_value: f64,
    /// Unrealized PnL from open positions
    pub unrealized_pnl: f64,
    /// Realized PnL during episode
    pub realized_pnl: f64,
    /// Current position quantity (signed: positive = long, negative = short)
    pub position_qty: f64,
    /// Average entry price for current position
    pub avg_entry_price: f64,
    /// Current position direction (None if flat)
    pub position_direction: Option<Direction>,
    /// Peak equity during episode (for drawdown calculation)
    pub peak_equity: f64,
    /// Maximum drawdown from peak
    pub max_drawdown: f64,
    /// Number of trades executed
    pub trade_count: usize,
    /// Timestamp of the snapshot
    pub timestamp: DateTime<Utc>,
}

impl PortfolioSnapshot {
    /// Create a new portfolio snapshot with initial capital.
    pub fn new(initial_capital: f64) -> Self {
        Self {
            equity: initial_capital,
            cash: initial_capital,
            position_value: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            position_qty: 0.0,
            avg_entry_price: 0.0,
            position_direction: None,
            peak_equity: initial_capital,
            max_drawdown: 0.0,
            trade_count: 0,
            timestamp: Utc::now(),
        }
    }

    /// Check if currently holding a long position.
    pub fn is_long(&self) -> bool {
        self.position_qty > 0.0 || self.position_direction == Some(Direction::Long)
    }

    /// Check if currently holding a short position.
    pub fn is_short(&self) -> bool {
        self.position_qty < 0.0 || self.position_direction == Some(Direction::Short)
    }

    /// Check if currently flat (no position).
    pub fn is_flat(&self) -> bool {
        self.position_qty.abs() < f64::EPSILON || self.position_direction.is_none()
    }

    /// Calculate current drawdown from peak.
    pub fn current_drawdown(&self) -> f64 {
        if self.peak_equity > 0.0 {
            (self.peak_equity - self.equity) / self.peak_equity
        } else {
            0.0
        }
    }

    /// Update equity and recalculate drawdown metrics.
    pub fn update_equity(&mut self, new_equity: f64) {
        self.equity = new_equity;
        if new_equity > self.peak_equity {
            self.peak_equity = new_equity;
        }
        let dd = self.current_drawdown();
        if dd > self.max_drawdown {
            self.max_drawdown = dd;
        }
    }
}

impl Default for PortfolioSnapshot {
    fn default() -> Self {
        Self::new(1_000_000.0)
    }
}

/// Market data snapshot for observation.
///
/// Captures the current bar/tick data in a normalized form
/// suitable for RL agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSnapshot {
    /// Symbol being traded
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Current timestamp
    pub datetime: DateTime<Utc>,
    /// Current open price
    pub open_price: f64,
    /// Current high price
    pub high_price: f64,
    /// Current low price
    pub low_price: f64,
    /// Current close price
    pub close_price: f64,
    /// Volume for the bar
    pub volume: f64,
    /// Turnover (volume * price or trade count)
    pub turnover: f64,
}

impl MarketSnapshot {
    /// Create from BarData.
    pub fn from_bar(bar: &BarData) -> Self {
        Self {
            symbol: bar.symbol.clone(),
            exchange: bar.exchange,
            datetime: bar.datetime,
            open_price: bar.open_price,
            high_price: bar.high_price,
            low_price: bar.low_price,
            close_price: bar.close_price,
            volume: bar.volume,
            turnover: bar.turnover,
        }
    }

    /// Create from TickData (synthesizes a "bar" from tick).
    pub fn from_tick(tick: &TickData) -> Self {
        Self {
            symbol: tick.symbol.clone(),
            exchange: tick.exchange,
            datetime: tick.datetime,
            open_price: tick.last_price,
            high_price: tick.last_price,
            low_price: tick.last_price,
            close_price: tick.last_price,
            volume: tick.volume,
            turnover: tick.turnover,
        }
    }
}

/// Definition of the observation space.
///
/// Describes the structure and bounds of the observation vector
/// for documentation and validation purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSpace {
    /// Feature dimension size
    pub dim: usize,
    /// Feature names for interpretability
    pub feature_names: Vec<String>,
    /// Lower bounds for each feature (optional, for bounded spaces)
    pub low: Option<Vec<f64>>,
    /// Upper bounds for each feature (optional, for bounded spaces)
    pub high: Option<Vec<f64>>,
}

impl ObservationSpace {
    /// Create a new observation space with the given dimension.
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            feature_names: Vec::new(),
            low: None,
            high: None,
        }
    }

    /// Create with feature names.
    pub fn with_names(names: Vec<String>) -> Self {
        let dim = names.len();
        Self {
            dim,
            feature_names: names,
            low: None,
            high: None,
        }
    }

    /// Create a bounded observation space.
    pub fn bounded(dim: usize, low: f64, high: f64) -> Self {
        Self {
            dim,
            feature_names: Vec::new(),
            low: Some(vec![low; dim]),
            high: Some(vec![high; dim]),
        }
    }
}

/// Observation returned by `reset()` and `step()`.
///
/// Contains:
/// - Normalized feature vector for the RL agent
/// - Current portfolio state
/// - Current market data snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Normalized feature vector for agent input
    pub features: Vec<f64>,
    /// Current portfolio state
    pub portfolio: PortfolioSnapshot,
    /// Current bar/tick data
    pub market_data: MarketSnapshot,
}

impl Observation {
    /// Create a new observation with the given components.
    pub fn new(
        features: Vec<f64>,
        portfolio: PortfolioSnapshot,
        market_data: MarketSnapshot,
    ) -> Self {
        Self {
            features,
            portfolio,
            market_data,
        }
    }

    /// Get the feature dimension.
    pub fn dim(&self) -> usize {
        self.features.len()
    }
}

/// Builder for constructing observations incrementally.
///
/// Allows adding features one-by-one and then building the
/// final observation. Useful when features are computed
/// from multiple sources.
pub struct ObservationBuilder {
    features: Vec<f64>,
    feature_names: Vec<String>,
    portfolio: PortfolioSnapshot,
    market_data: Option<MarketSnapshot>,
}

impl ObservationBuilder {
    /// Create a new builder with estimated capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            features: Vec::with_capacity(capacity),
            feature_names: Vec::with_capacity(capacity),
            portfolio: PortfolioSnapshot::default(),
            market_data: None,
        }
    }

    /// Set the portfolio snapshot.
    pub fn portfolio(mut self, portfolio: PortfolioSnapshot) -> Self {
        self.portfolio = portfolio;
        self
    }

    /// Set the market data snapshot.
    pub fn market_data(mut self, market_data: MarketSnapshot) -> Self {
        self.market_data = Some(market_data);
        self
    }

    /// Add a single feature value.
    pub fn add_feature(mut self, value: f64) -> Self {
        self.features.push(value);
        self
    }

    /// Add a named feature value.
    pub fn add_named_feature(mut self, name: &str, value: f64) -> Self {
        self.features.push(value);
        self.feature_names.push(name.to_string());
        self
    }

    /// Add multiple features at once.
    pub fn add_features(mut self, values: &[f64]) -> Self {
        self.features.extend_from_slice(values);
        self
    }

    /// Add features from a HashMap.
    pub fn add_features_from_map(mut self, map: &HashMap<String, f64>, keys: &[&str]) -> Self {
        for key in keys {
            if let Some(value) = map.get(*key) {
                self.features.push(*value);
                self.feature_names.push(key.to_string());
            }
        }
        self
    }

    /// Build the final observation.
    ///
    /// Returns an error if market_data was not set.
    pub fn build(self) -> Result<Observation, String> {
        let market_data = self.market_data.ok_or_else(|| {
            "market_data is required to build Observation".to_string()
        })?;
        
        Ok(Observation {
            features: self.features,
            portfolio: self.portfolio,
            market_data,
        })
    }

    /// Build the observation space definition from the current feature names.
    pub fn build_space(&self) -> ObservationSpace {
        ObservationSpace::with_names(self.feature_names.clone())
    }

    /// Get the current number of features.
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Check if no features have been added.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Exchange, Interval};

    fn make_bar(close: f64, volume: f64) -> BarData {
        BarData {
            gateway_name: "test".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
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

    #[test]
    fn test_portfolio_snapshot_new() {
        let snap = PortfolioSnapshot::new(100_000.0);
        assert_eq!(snap.equity, 100_000.0);
        assert_eq!(snap.cash, 100_000.0);
        assert!(snap.is_flat());
        assert!(!snap.is_long());
        assert!(!snap.is_short());
    }

    #[test]
    fn test_portfolio_snapshot_position() {
        let mut snap = PortfolioSnapshot::new(100_000.0);
        snap.position_qty = 10.0;
        snap.position_direction = Some(Direction::Long);
        assert!(snap.is_long());
        assert!(!snap.is_short());
        assert!(!snap.is_flat());
    }

    #[test]
    fn test_portfolio_snapshot_drawdown() {
        let mut snap = PortfolioSnapshot::new(100_000.0);
        assert_eq!(snap.current_drawdown(), 0.0);
        
        snap.update_equity(90_000.0);
        assert!((snap.current_drawdown() - 0.1).abs() < 1e-10);
        assert!((snap.max_drawdown - 0.1).abs() < 1e-10);
        
        // New peak resets drawdown
        snap.update_equity(110_000.0);
        assert_eq!(snap.current_drawdown(), 0.0);
        assert_eq!(snap.peak_equity, 110_000.0);
        // Max drawdown is still 10%
        assert!((snap.max_drawdown - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_market_snapshot_from_bar() {
        let bar = make_bar(42000.0, 100.0);
        let snap = MarketSnapshot::from_bar(&bar);
        assert_eq!(snap.symbol, "BTCUSDT");
        assert_eq!(snap.exchange, Exchange::Binance);
        assert_eq!(snap.close_price, 42000.0);
        assert_eq!(snap.volume, 100.0);
    }

    #[test]
    fn test_observation_space_new() {
        let space = ObservationSpace::new(10);
        assert_eq!(space.dim, 10);
        assert!(space.feature_names.is_empty());
        assert!(space.low.is_none());
        assert!(space.high.is_none());
    }

    #[test]
    fn test_observation_space_bounded() {
        let space = ObservationSpace::bounded(5, -1.0, 1.0);
        assert_eq!(space.dim, 5);
        assert!(space.low.is_some());
        assert!(space.high.is_some());
        assert_eq!(space.low.as_ref().map(|v| v.len()), Some(5));
    }

    #[test]
    fn test_observation_builder() {
        let bar = make_bar(42000.0, 100.0);
        let market = MarketSnapshot::from_bar(&bar);
        
        let obs = ObservationBuilder::new(5)
            .market_data(market)
            .add_named_feature("close", 42000.0)
            .add_named_feature("volume", 100.0)
            .add_feature(0.5)
            .build()
            .expect("should build observation");
        
        assert_eq!(obs.features.len(), 3);
        assert_eq!(obs.features[0], 42000.0);
        assert_eq!(obs.features[1], 100.0);
        assert_eq!(obs.features[2], 0.5);
    }

    #[test]
    fn test_observation_builder_missing_market_data() {
        let result = ObservationBuilder::new(5)
            .add_feature(1.0)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_observation_serialization() {
        let bar = make_bar(42000.0, 100.0);
        let market = MarketSnapshot::from_bar(&bar);
        let portfolio = PortfolioSnapshot::new(100_000.0);
        
        let obs = Observation::new(
            vec![1.0, 2.0, 3.0],
            portfolio,
            market,
        );
        
        let json = serde_json::to_string(&obs).expect("serialization should succeed");
        let parsed: Observation = serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(parsed.features, vec![1.0, 2.0, 3.0]);
        assert_eq!(parsed.portfolio.equity, 100_000.0);
    }
}
