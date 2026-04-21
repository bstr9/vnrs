//! Action Mapping for RL Environment
//!
//! Provides the `ActionMapper` trait and implementations for mapping
//! RL agent actions (discrete or continuous) to trading orders.
//!
//! # Design
//!
//! - `DiscreteActionMapper`: Maps integer actions to buy/sell/hold
//! - `ContinuousActionMapper`: Maps floating-point actions to position sizes
//!
//! Both mappers produce `Vec<OrderRequest>` that can be fed into the
//! backtesting engine.

use serde::{Deserialize, Serialize};

use crate::trader::{Direction, Exchange, Offset, OrderRequest, OrderType};

/// Describes the action space type and dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionSpace {
    /// Discrete action space with `n` possible actions (0..n)
    Discrete { n: usize },
    /// Continuous action space with `dim` dimensions, each in [low, high]
    Continuous { dim: usize, low: f64, high: f64 },
    /// Multi-discrete action space (e.g., separate action for each asset)
    MultiDiscrete { nvec: Vec<usize> },
}

/// Action value from the RL agent.
///
/// Can be either a discrete integer (for DiscreteActionMapper)
/// or continuous floats (for ContinuousActionMapper).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionValue {
    /// Discrete action index
    Discrete(i64),
    /// Continuous action vector
    Continuous(Vec<f64>),
    /// Multi-discrete action indices
    MultiDiscrete(Vec<i64>),
}

/// Trait for mapping agent actions to order requests.
///
/// Implementations convert the agent's action output into concrete
/// `OrderRequest` objects that can be submitted to the trading engine.
pub trait ActionMapper: Send + Sync {
    /// Return the description of the action space.
    fn action_space(&self) -> ActionSpace;

    /// Map an action value to a list of order requests.
    ///
    /// The returned orders represent the desired trades for this step.
    /// The environment will submit them to the backtesting engine.
    fn map_action(&self, action: &ActionValue) -> Vec<OrderRequest>;
}

// ---------------------------------------------------------------------------
// DiscreteActionMapper
// ---------------------------------------------------------------------------

/// Discrete action definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiscreteAction {
    /// Hold / do nothing
    Hold,
    /// Buy (go long or add to long position)
    Buy,
    /// Sell (go short or add to short position)
    Sell,
    /// Close all positions
    Close,
}

impl std::fmt::Display for DiscreteAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscreteAction::Hold => write!(f, "Hold"),
            DiscreteAction::Buy => write!(f, "Buy"),
            DiscreteAction::Sell => write!(f, "Sell"),
            DiscreteAction::Close => write!(f, "Close"),
        }
    }
}

/// Maps discrete integer actions to buy/sell/hold/close.
///
/// Action mapping:
/// - 0 → Hold (no orders)
/// - 1 → Buy at market
/// - 2 → Sell at market
/// - 3 → Close all positions (if any)
///
/// The `order_volume` parameter controls the size of each order.
pub struct DiscreteActionMapper {
    /// Symbol to trade
    symbol: String,
    /// Exchange
    exchange: Exchange,
    /// Volume per order
    order_volume: f64,
    /// Action definitions (index → DiscreteAction)
    actions: Vec<DiscreteAction>,
}

impl DiscreteActionMapper {
    /// Create a new discrete action mapper with default 4 actions (Hold, Buy, Sell, Close).
    pub fn new(symbol: String, exchange: Exchange, order_volume: f64) -> Self {
        Self {
            symbol,
            exchange,
            order_volume,
            actions: vec![
                DiscreteAction::Hold,
                DiscreteAction::Buy,
                DiscreteAction::Sell,
                DiscreteAction::Close,
            ],
        }
    }

    /// Create with custom action set.
    pub fn with_actions(
        symbol: String,
        exchange: Exchange,
        order_volume: f64,
        actions: Vec<DiscreteAction>,
    ) -> Self {
        Self {
            symbol,
            exchange,
            order_volume,
            actions,
        }
    }

    /// Get the number of discrete actions.
    pub fn n_actions(&self) -> usize {
        self.actions.len()
    }

    /// Resolve a discrete index to a DiscreteAction.
    fn resolve(&self, index: i64) -> Option<DiscreteAction> {
        if index < 0 {
            return None;
        }
        let idx = index as usize;
        if idx < self.actions.len() {
            Some(self.actions[idx])
        } else {
            None
        }
    }

    /// Create a buy order request.
    fn buy_request(&self) -> OrderRequest {
        OrderRequest::new(
            self.symbol.clone(),
            self.exchange,
            Direction::Long,
            OrderType::Market,
            self.order_volume,
        )
    }

    /// Create a sell order request.
    fn sell_request(&self) -> OrderRequest {
        OrderRequest::new(
            self.symbol.clone(),
            self.exchange,
            Direction::Short,
            OrderType::Market,
            self.order_volume,
        )
    }

    /// Create a close-long order request.
    fn close_long_request(&self, volume: f64) -> OrderRequest {
        OrderRequest {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            direction: Direction::Short,
            order_type: OrderType::Market,
            volume,
            price: 0.0,
            offset: Offset::Close,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        }
    }

    /// Create a close-short order request.
    fn close_short_request(&self, volume: f64) -> OrderRequest {
        OrderRequest {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            direction: Direction::Long,
            order_type: OrderType::Market,
            volume,
            price: 0.0,
            offset: Offset::Close,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        }
    }
}

impl ActionMapper for DiscreteActionMapper {
    fn action_space(&self) -> ActionSpace {
        ActionSpace::Discrete { n: self.actions.len() }
    }

    fn map_action(&self, action: &ActionValue) -> Vec<OrderRequest> {
        let index = match action {
            ActionValue::Discrete(i) => *i,
            ActionValue::Continuous(v) if v.len() == 1 => v[0] as i64,
            _ => return Vec::new(),
        };

        match self.resolve(index) {
            Some(DiscreteAction::Hold) | None => Vec::new(),
            Some(DiscreteAction::Buy) => vec![self.buy_request()],
            Some(DiscreteAction::Sell) => vec![self.sell_request()],
            Some(DiscreteAction::Close) => {
                // Close emits both close-long and close-short; the engine
                // will ignore the one that has no position to close.
                vec![
                    self.close_long_request(self.order_volume),
                    self.close_short_request(self.order_volume),
                ]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ContinuousActionMapper
// ---------------------------------------------------------------------------

/// Maps continuous action values to position sizes.
///
/// The action value is interpreted as a target position weight:
/// - `weight > 0` → target long position of `weight * max_volume`
/// - `weight < 0` → target short position of `|weight| * max_volume`
/// - `weight = 0` → flat (close all)
///
/// The mapper computes the delta from the current position and generates
/// orders to reach the target position.
pub struct ContinuousActionMapper {
    /// Symbol to trade
    symbol: String,
    /// Exchange
    exchange: Exchange,
    /// Maximum volume for position sizing
    max_volume: f64,
    /// Action dimension (1 for single-asset, n for multi-asset)
    action_dim: usize,
}

impl ContinuousActionMapper {
    /// Create a new continuous action mapper.
    ///
    /// `max_volume` is the maximum position size that corresponds to a
    /// weight of 1.0 or -1.0.
    pub fn new(symbol: String, exchange: Exchange, max_volume: f64) -> Self {
        Self {
            symbol,
            exchange,
            max_volume,
            action_dim: 1,
        }
    }

    /// Create with custom action dimension.
    pub fn with_dim(symbol: String, exchange: Exchange, max_volume: f64, dim: usize) -> Self {
        Self {
            symbol,
            exchange,
            max_volume,
            action_dim: dim,
        }
    }

    /// Compute order requests to go from `current_qty` to `target_qty`.
    fn compute_orders(&self, current_qty: f64, target_qty: f64) -> Vec<OrderRequest> {
        let delta = target_qty - current_qty;
        if delta.abs() < f64::EPSILON {
            return Vec::new();
        }

        let (direction, volume, offset) = if delta > 0.0 {
            // Need to buy more or close short + go long
            if current_qty < 0.0 {
                // Currently short: close short first, then open long
                let close_volume = current_qty.abs().min(delta);
                if close_volume < current_qty.abs() {
                    // Partial close of short, no new long yet
                    (Direction::Long, close_volume, Offset::Close)
                } else {
                    // Full close of short + open long
                    (Direction::Long, delta, Offset::None)
                }
            } else {
                // Currently long or flat: add to position
                (Direction::Long, delta, Offset::Open)
            }
        } else {
            // Need to sell or close long + go short
            if current_qty > 0.0 {
                // Currently long: close long first
                let close_volume = current_qty.min(delta.abs());
                if close_volume < current_qty {
                    (Direction::Short, close_volume, Offset::Close)
                } else {
                    (Direction::Short, delta.abs(), Offset::None)
                }
            } else {
                // Currently short or flat: add to short position
                (Direction::Short, delta.abs(), Offset::Open)
            }
        };

        if volume < f64::EPSILON {
            return Vec::new();
        }

        vec![OrderRequest {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            direction,
            order_type: OrderType::Market,
            volume,
            price: 0.0,
            offset,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        }]
    }
}

impl ActionMapper for ContinuousActionMapper {
    fn action_space(&self) -> ActionSpace {
        ActionSpace::Continuous {
            dim: self.action_dim,
            low: -1.0,
            high: 1.0,
        }
    }

    fn map_action(&self, action: &ActionValue) -> Vec<OrderRequest> {
        let weight = match action {
            ActionValue::Continuous(v) if !v.is_empty() => v[0],
            ActionValue::Discrete(i) => {
                // Interpret discrete as: 0=hold, 1=buy_max, -1=sell_max
                (*i as f64).clamp(-1.0, 1.0)
            }
            _ => 0.0,
        };

        // Clamp weight to [-1.0, 1.0]
        let weight = weight.clamp(-1.0, 1.0);
        
        // Target position size
        let target_qty = weight * self.max_volume;
        
        // Current position is tracked externally; we compute from 0.0
        // (the env will pass current position as context)
        self.compute_orders(0.0, target_qty)
    }
}

/// Map a continuous action to order requests given a known current position.
///
/// This is the preferred method for environments that track position state.
pub fn map_continuous_action_with_position(
    mapper: &ContinuousActionMapper,
    action: &ActionValue,
    current_qty: f64,
) -> Vec<OrderRequest> {
    let weight = match action {
        ActionValue::Continuous(v) if !v.is_empty() => v[0],
        ActionValue::Discrete(i) => (*i as f64).clamp(-1.0, 1.0),
        _ => 0.0,
    };

    let weight = weight.clamp(-1.0, 1.0);
    let target_qty = weight * mapper.max_volume;
    mapper.compute_orders(current_qty, target_qty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> DiscreteActionMapper {
        DiscreteActionMapper::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            1.0,
        )
    }

    fn make_continuous_mapper() -> ContinuousActionMapper {
        ContinuousActionMapper::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            10.0,
        )
    }

    // --- DiscreteActionMapper ---

    #[test]
    fn test_discrete_action_space() {
        let mapper = make_mapper();
        let space = mapper.action_space();
        match space {
            ActionSpace::Discrete { n } => assert_eq!(n, 4),
            _ => panic!("expected Discrete action space"),
        }
    }

    #[test]
    fn test_discrete_hold() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(0));
        assert!(orders.is_empty());
    }

    #[test]
    fn test_discrete_buy() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(1));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert_eq!(orders[0].volume, 1.0);
        assert_eq!(orders[0].order_type, OrderType::Market);
    }

    #[test]
    fn test_discrete_sell() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(2));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Short);
        assert_eq!(orders[0].volume, 1.0);
    }

    #[test]
    fn test_discrete_close() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(3));
        // Close emits both close-long and close-short
        assert_eq!(orders.len(), 2);
    }

    #[test]
    fn test_discrete_invalid_index() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(99));
        assert!(orders.is_empty());
    }

    #[test]
    fn test_discrete_negative_index() {
        let mapper = make_mapper();
        let orders = mapper.map_action(&ActionValue::Discrete(-1));
        assert!(orders.is_empty());
    }

    #[test]
    fn test_discrete_custom_actions() {
        let mapper = DiscreteActionMapper::with_actions(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            1.0,
            vec![DiscreteAction::Hold, DiscreteAction::Buy],
        );
        assert_eq!(mapper.n_actions(), 2);
        
        let orders = mapper.map_action(&ActionValue::Discrete(1));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
    }

    // --- ContinuousActionMapper ---

    #[test]
    fn test_continuous_action_space() {
        let mapper = make_continuous_mapper();
        let space = mapper.action_space();
        match space {
            ActionSpace::Continuous { dim, low, high } => {
                assert_eq!(dim, 1);
                assert_eq!(low, -1.0);
                assert_eq!(high, 1.0);
            }
            _ => panic!("expected Continuous action space"),
        }
    }

    #[test]
    fn test_continuous_zero_weight() {
        let mapper = make_continuous_mapper();
        let orders = mapper.map_action(&ActionValue::Continuous(vec![0.0]));
        assert!(orders.is_empty());
    }

    #[test]
    fn test_continuous_positive_weight() {
        let mapper = make_continuous_mapper();
        let orders = mapper.map_action(&ActionValue::Continuous(vec![0.5]));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert!((orders[0].volume - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_continuous_negative_weight() {
        let mapper = make_continuous_mapper();
        let orders = mapper.map_action(&ActionValue::Continuous(vec![-0.5]));
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Short);
        assert!((orders[0].volume - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_continuous_max_weight() {
        let mapper = make_continuous_mapper();
        let orders = mapper.map_action(&ActionValue::Continuous(vec![1.0]));
        assert_eq!(orders.len(), 1);
        assert!((orders[0].volume - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_continuous_clamp() {
        let mapper = make_continuous_mapper();
        // Weight > 1.0 should be clamped
        let orders = mapper.map_action(&ActionValue::Continuous(vec![2.0]));
        assert_eq!(orders.len(), 1);
        assert!((orders[0].volume - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_continuous_with_position() {
        let mapper = make_continuous_mapper();
        // Currently long 5 units, target is 10 (weight=1.0)
        let orders = map_continuous_action_with_position(
            &mapper,
            &ActionValue::Continuous(vec![1.0]),
            5.0,
        );
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert!((orders[0].volume - 5.0).abs() < f64::EPSILON);
    }

    // --- ActionValue / ActionSpace ---

    #[test]
    fn test_action_value_serialization() {
        let discrete = ActionValue::Discrete(3);
        let json = serde_json::to_string(&discrete).expect("should serialize");
        let parsed: ActionValue = serde_json::from_str(&json).expect("should deserialize");
        match parsed {
            ActionValue::Discrete(i) => assert_eq!(i, 3),
            _ => panic!("expected Discrete"),
        }

        let continuous = ActionValue::Continuous(vec![0.5, -0.3]);
        let json = serde_json::to_string(&continuous).expect("should serialize");
        let parsed: ActionValue = serde_json::from_str(&json).expect("should deserialize");
        match parsed {
            ActionValue::Continuous(v) => assert_eq!(v, vec![0.5, -0.3]),
            _ => panic!("expected Continuous"),
        }
    }

    #[test]
    fn test_discrete_action_display() {
        assert_eq!(format!("{}", DiscreteAction::Hold), "Hold");
        assert_eq!(format!("{}", DiscreteAction::Buy), "Buy");
        assert_eq!(format!("{}", DiscreteAction::Sell), "Sell");
        assert_eq!(format!("{}", DiscreteAction::Close), "Close");
    }
}
