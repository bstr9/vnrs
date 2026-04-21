//! Grid Strategy Template
//!
//! Grid trading strategy that places buy/sell orders at fixed price intervals.
//! When a buy fills at level N, a sell is placed at level N+1, and vice versa.

use std::collections::HashMap;

use super::base::{StrategySetting, StrategyState, StrategyType, StopOrderRequest, CancelRequestType};
use super::template::{BaseStrategy, StrategyContext, StrategyTemplate};
use crate::trader::{
    BarData, Direction, Offset, OrderData, OrderRequest, TickData, TradeData,
};

/// Status of a grid level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridStatus {
    /// Order not yet placed
    Pending,
    /// Order placed, waiting to fill
    Active,
    /// Order filled
    Filled,
    /// Order cancelled
    Cancelled,
}

/// A single grid level
#[derive(Debug, Clone)]
pub struct GridLevel {
    /// Level index (0 = center)
    pub index: i32,
    /// Price for this grid level
    pub price: f64,
    /// Direction: Long = buy level, Short = sell level
    pub direction: Direction,
    /// Volume for this level
    pub volume: f64,
    /// Current status
    pub status: GridStatus,
    /// VT order ID if active
    pub vt_orderid: Option<String>,
}

impl GridLevel {
    pub fn new(index: i32, price: f64, direction: Direction, volume: f64) -> Self {
        Self {
            index,
            price,
            direction,
            volume,
            status: GridStatus::Pending,
            vt_orderid: None,
        }
    }
}

/// Grid trading strategy
pub struct GridStrategy {
    /// Base strategy implementation
    base: BaseStrategy,
    
    // Grid parameters
    /// Center price of the grid
    pub center_price: f64,
    /// Price step between grid levels
    pub grid_step: f64,
    /// Number of grid levels above and below center
    pub grid_count: usize,
    /// Volume per grid level
    pub grid_volume: f64,
    
    /// All grid levels indexed by level index
    pub grid_levels: HashMap<i32, GridLevel>,
    
    /// Realized PnL from completed grid pairs
    pub realized_pnl: f64,
    
    /// Traded symbol
    vt_symbol: String,
}

impl GridStrategy {
    /// Create a new grid strategy
    pub fn new(
        strategy_name: String,
        vt_symbol: String,
        setting: StrategySetting,
    ) -> Self {
        let center_price = setting
            .get("center_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(50000.0);
        let grid_step = setting
            .get("grid_step")
            .and_then(|v| v.as_f64())
            .unwrap_or(500.0);
        let grid_count = setting
            .get("grid_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let grid_volume = setting
            .get("grid_volume")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.01);
        
        Self {
            base: BaseStrategy::new(strategy_name, vec![vt_symbol.clone()], StrategyType::Grid, setting),
            center_price,
            grid_step,
            grid_count,
            grid_volume,
            grid_levels: HashMap::new(),
            realized_pnl: 0.0,
            vt_symbol,
        }
    }
    
    /// Calculate all grid levels and place initial orders
    pub fn init_grid(&mut self) {
        // Create grid levels: sell levels above center, buy levels below
        for i in 1..=self.grid_count as i32 {
            // Buy level below center
            let buy_price = self.center_price - self.grid_step * i as f64;
            let buy_level = GridLevel::new(-i, buy_price, Direction::Long, self.grid_volume);
            self.grid_levels.insert(-i, buy_level);
            
            // Sell level above center
            let sell_price = self.center_price + self.grid_step * i as f64;
            let sell_level = GridLevel::new(i, sell_price, Direction::Short, self.grid_volume);
            self.grid_levels.insert(i, sell_level);
        }
        
        // Place initial orders
        self.place_pending_orders();
        
        self.base.write_log(&format!(
            "网格初始化: 中心价={:.2}, 步长={:.2}, 层数={}, 总级别={}",
            self.center_price, self.grid_step, self.grid_count, self.grid_levels.len()
        ));
    }
    
    /// Place all pending grid level orders
    fn place_pending_orders(&mut self) {
        let levels: Vec<(i32, GridLevel)> = self.grid_levels.iter()
            .filter(|(_, l)| l.status == GridStatus::Pending)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        
        for (idx, level) in levels {
            let req = OrderRequest {
                symbol: self.vt_symbol.split('.').next().unwrap_or(&self.vt_symbol).to_string(),
                exchange: self.get_exchange(),
                direction: level.direction,
                order_type: crate::trader::constant::OrderType::Limit,
                volume: level.volume,
                price: level.price,
                offset: Offset::None,
                reference: self.base.strategy_name.clone(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
            };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            
            if let Some(l) = self.grid_levels.get_mut(&idx) {
                l.status = GridStatus::Active;
                l.vt_orderid = Some(format!("GRID_{}_{}", idx, chrono::Utc::now().timestamp_millis()));
            }
        }
    }
    
    /// Handle trade fill — place opposite order at next grid level
    fn handle_fill(&mut self, filled_direction: Direction, filled_price: f64) {
        // Find which level was filled
        let filled_idx = self.grid_levels.iter()
            .filter(|(_, l)| l.status == GridStatus::Active)
            .find(|(_, l)| {
                (l.price - filled_price).abs() < self.grid_step * 0.1
                    && l.direction == filled_direction
            })
            .map(|(k, _)| *k);
        
        let Some(idx) = filled_idx else {
            return;
        };
        
        // Mark filled
        if let Some(l) = self.grid_levels.get_mut(&idx) {
            l.status = GridStatus::Filled;
        }
        
        // Place opposite order at adjacent level
        let (next_idx, next_direction) = match filled_direction {
            Direction::Long => {
                // Buy filled at level -N → place sell at level -N+1
                self.realized_pnl += 0.0; // Will calculate on sell fill
                (idx + 1, Direction::Short)
            }
            Direction::Short => {
                // Sell filled at level N → place buy at level N-1
                let pnl = self.grid_step * self.grid_volume;
                self.realized_pnl += pnl;
                (idx - 1, Direction::Long)
            }
            _ => return,
        };
        
        // Create or re-activate the next level
        if let Some(next_level) = self.grid_levels.get_mut(&next_idx) {
            if next_level.status == GridStatus::Filled || next_level.status == GridStatus::Pending {
                next_level.status = GridStatus::Pending;
                next_level.direction = next_direction;
            }
        } else {
            // Level doesn't exist yet (outside initial grid), create it
            let price = self.center_price + self.grid_step * next_idx as f64;
            let new_level = GridLevel::new(next_idx, price, next_direction, self.grid_volume);
            self.grid_levels.insert(next_idx, new_level);
        }
        
        // Place the new pending orders
        self.place_pending_orders();
    }
    
    /// Get total grid PnL
    pub fn get_grid_pnl(&self) -> f64 {
        self.realized_pnl
    }
    
    /// Get count of filled levels
    pub fn get_filled_count(&self) -> usize {
        self.grid_levels.values().filter(|l| l.status == GridStatus::Filled).count()
    }
    
    /// Get count of active levels
    pub fn get_active_count(&self) -> usize {
        self.grid_levels.values().filter(|l| l.status == GridStatus::Active).count()
    }
    
    fn get_exchange(&self) -> crate::trader::constant::Exchange {
        let parts: Vec<&str> = self.vt_symbol.split('.').collect();
        if parts.len() == 2 {
            match parts[1].to_uppercase().as_str() {
                "BINANCE" => crate::trader::constant::Exchange::Binance,
                _ => crate::trader::constant::Exchange::Local,
            }
        } else {
            crate::trader::constant::Exchange::Local
        }
    }
}

impl StrategyTemplate for GridStrategy {
    fn strategy_name(&self) -> &str {
        &self.base.strategy_name
    }
    
    fn vt_symbols(&self) -> &[String] {
        &self.base.vt_symbols
    }
    
    fn strategy_type(&self) -> StrategyType {
        self.base.strategy_type
    }
    
    fn state(&self) -> StrategyState {
        self.base.state
    }
    
    fn parameters(&self) -> HashMap<String, String> {
        self.base.parameters.clone()
    }
    
    fn variables(&self) -> HashMap<String, String> {
        let mut vars = self.base.variables.clone();
        vars.insert("realized_pnl".to_string(), format!("{:.2}", self.realized_pnl));
        vars.insert("filled_count".to_string(), self.get_filled_count().to_string());
        vars.insert("active_count".to_string(), self.get_active_count().to_string());
        vars
    }
    
    fn on_init(&mut self, _context: &StrategyContext) {
        self.base.write_log("GridStrategy初始化");
        self.init_grid();
        self.base.state = StrategyState::Inited;
    }
    
    fn on_start(&mut self) {
        self.base.write_log("GridStrategy启动");
        self.base.state = StrategyState::Trading;
    }
    
    fn on_stop(&mut self) {
        self.base.write_log("GridStrategy停止");
        self.base.state = StrategyState::Stopped;
    }
    
    fn on_tick(&mut self, _tick: &TickData, _context: &StrategyContext) {}
    
    fn on_bar(&mut self, _bar: &BarData, _context: &StrategyContext) {
        // Grid strategy is order-driven, not bar-driven
    }
    
    fn on_order(&mut self, _order: &OrderData) {}
    
    fn on_trade(&mut self, trade: &TradeData) {
        let direction = trade.direction.unwrap_or(Direction::Long);
        self.handle_fill(direction, trade.price);
        self.base.write_log(&format!(
            "网格成交: {:?} {:.4}@{:.2}", direction, trade.volume, trade.price
        ));
    }
    
    fn on_stop_order(&mut self, _stop_orderid: &str) {}
    
    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        self.base.drain_pending_orders()
    }
    
    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        self.base.drain_pending_stop_orders()
    }
    
    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        self.base.drain_pending_cancellations()
    }
    
    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        self.base.sync_position(vt_symbol, position);
    }
    
    fn get_position(&self, vt_symbol: &str) -> f64 {
        self.base
            .positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_grid_strategy_new() {
        let setting = StrategySetting::new();
        let strategy = GridStrategy::new(
            "TestGrid".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        assert_eq!(strategy.strategy_name(), "TestGrid");
        assert_eq!(strategy.strategy_type(), StrategyType::Grid);
        assert_eq!(strategy.grid_count, 10);
    }
    
    #[test]
    fn test_grid_strategy_custom_params() {
        let mut setting = StrategySetting::new();
        setting.insert("center_price".to_string(), serde_json::json!(60000.0));
        setting.insert("grid_step".to_string(), serde_json::json!(1000.0));
        setting.insert("grid_count".to_string(), serde_json::json!(5));
        setting.insert("grid_volume".to_string(), serde_json::json!(0.1));
        
        let strategy = GridStrategy::new(
            "TestGrid".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        assert!((strategy.center_price - 60000.0).abs() < 1e-10);
        assert!((strategy.grid_step - 1000.0).abs() < 1e-10);
        assert_eq!(strategy.grid_count, 5);
    }
    
    #[test]
    fn test_grid_init_creates_levels() {
        let mut setting = StrategySetting::new();
        setting.insert("grid_count".to_string(), serde_json::json!(3));
        
        let mut strategy = GridStrategy::new(
            "TestGrid".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        strategy.init_grid();
        
        // 3 levels above + 3 levels below = 6 total
        assert_eq!(strategy.grid_levels.len(), 6);
        
        // Check buy levels below center
        let buy_level = strategy.grid_levels.get(&-1).expect("level -1 should exist");
        assert_eq!(buy_level.direction, Direction::Long);
        assert!((buy_level.price - 49500.0).abs() < 1e-10);
        
        // Check sell levels above center
        let sell_level = strategy.grid_levels.get(&1).expect("level 1 should exist");
        assert_eq!(sell_level.direction, Direction::Short);
        assert!((sell_level.price - 50500.0).abs() < 1e-10);
    }
}
