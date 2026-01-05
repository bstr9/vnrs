//! Template for alpha strategies
//! Provides the base structure for implementing alpha trading strategies

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
#[cfg(feature = "alpha")]
use polars::prelude::*;
use crate::alpha::strategy::backtesting::BacktestingEngine;

pub struct AlphaStrategy {
    pub strategy_name: String,
    pub vt_symbols: Vec<String>,
    
    // Position data
    pub pos_data: Arc<Mutex<HashMap<String, f64>>>,
    pub target_data: Arc<Mutex<HashMap<String, f64>>>,
    
    // Order management
    pub active_orderids: Arc<Mutex<Vec<String>>>,
}

impl AlphaStrategy {
    pub fn new(
        strategy_name: String,
        vt_symbols: Vec<String>,
        _setting: HashMap<String, serde_json::Value>,
    ) -> Self {
        AlphaStrategy {
            strategy_name,
            vt_symbols,
            pos_data: Arc::new(Mutex::new(HashMap::new())),
            target_data: Arc::new(Mutex::new(HashMap::new())),
            active_orderids: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Initialization callback
    pub fn on_init(&mut self) {
        println!("Strategy {} initialized", self.strategy_name);
    }

    /// Bar slice callback
    pub fn on_bars(&mut self, _bars: &HashMap<String, crate::trader::TickData>) {
        // Process bar data
        println!("Processing bars for strategy: {}", self.strategy_name);
    }

    /// Trade callback
    pub fn on_trade(&mut self, _trade: &crate::trader::TradeData) {
        println!("Processing trade for strategy: {}", self.strategy_name);
    }

    /// Update trade data
    pub fn update_trade(&mut self, trade: &crate::trader::TradeData) {
        {
            let mut pos_data = self.pos_data.lock().unwrap();
            if let Some(direction) = trade.direction {
                if direction == crate::trader::Direction::Long {
                    *pos_data.entry(trade.vt_symbol().clone()).or_insert(0.0) += trade.volume;
                } else {
                    *pos_data.entry(trade.vt_symbol().clone()).or_insert(0.0) -= trade.volume;
                }
            }
        }
        
        self.on_trade(trade);
    }

    /// Get current signal
    #[cfg(feature = "alpha")]
    pub fn get_signal(&self, engine: &BacktestingEngine) -> Result<DataFrame, Box<dyn std::error::Error>> {
        // This would get the signal from the backtesting engine
        Ok(engine.get_signal())
    }

    #[cfg(not(feature = "alpha"))]
    pub fn get_signal(&self, _engine: &BacktestingEngine) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Buy to open position
    pub fn buy(&mut self, vt_symbol: &str, price: f64, volume: f64, engine: &mut BacktestingEngine) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.send_order(vt_symbol, crate::trader::Direction::Long, crate::trader::Offset::Open, price, volume, engine)
    }

    /// Sell to close position
    pub fn sell(&mut self, vt_symbol: &str, price: f64, volume: f64, engine: &mut BacktestingEngine) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.send_order(vt_symbol, crate::trader::Direction::Short, crate::trader::Offset::Close, price, volume, engine)
    }

    /// Sell to open position (short)
    pub fn short(&mut self, vt_symbol: &str, price: f64, volume: f64, engine: &mut BacktestingEngine) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.send_order(vt_symbol, crate::trader::Direction::Short, crate::trader::Offset::Open, price, volume, engine)
    }

    /// Buy to close position (cover)
    pub fn cover(&mut self, vt_symbol: &str, price: f64, volume: f64, engine: &mut BacktestingEngine) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.send_order(vt_symbol, crate::trader::Direction::Long, crate::trader::Offset::Close, price, volume, engine)
    }

    /// Send order
    pub fn send_order(
        &mut self,
        vt_symbol: &str,
        direction: crate::trader::Direction,
        offset: crate::trader::Offset,
        price: f64,
        volume: f64,
        engine: &mut BacktestingEngine,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let order_ids = engine.send_order(vt_symbol, direction, offset, price, volume);
        let mut active_orders = self.active_orderids.lock().unwrap();
        active_orders.extend(order_ids.clone());
        Ok(order_ids)
    }

    /// Cancel order
    pub fn cancel_order(&mut self, vt_orderid: &str, engine: &mut BacktestingEngine) {
        engine.cancel_order(vt_orderid);
    }

    /// Cancel all active orders
    pub fn cancel_all(&mut self, engine: &mut BacktestingEngine) {
        let mut active_orders = self.active_orderids.lock().unwrap();
        for order_id in active_orders.iter() {
            engine.cancel_order(order_id);
        }
        active_orders.clear();
        println!("All orders canceled");
    }

    /// Query current position
    pub fn get_pos(&self, vt_symbol: &str) -> f64 {
        let pos_data = self.pos_data.lock().unwrap();
        *pos_data.get(vt_symbol).unwrap_or(&0.0)
    }

    /// Query target position
    pub fn get_target(&self, vt_symbol: &str) -> f64 {
        let target_data = self.target_data.lock().unwrap();
        *target_data.get(vt_symbol).unwrap_or(&0.0)
    }

    /// Set target position
    pub fn set_target(&mut self, vt_symbol: &str, target: f64) {
        let mut target_data = self.target_data.lock().unwrap();
        target_data.insert(vt_symbol.to_string(), target);
    }

    /// Execute position adjustment based on targets
    pub fn execute_trading(&mut self, bars: &HashMap<String, crate::trader::TickData>, price_add: f64, engine: &mut BacktestingEngine) -> Result<(), Box<dyn std::error::Error>> {
        // Cancel all orders first
        self.cancel_all(engine);
        
        // Then execute trades based on target positions
        for (vt_symbol, bar) in bars {
            let target = self.get_target(vt_symbol);
            let pos = self.get_pos(vt_symbol);
            let diff = target - pos;

            if diff > 0.0 {
                // Long position
                let order_price = bar.last_price * (1.0 + price_add);
                let cover_volume = (0.0_f64).min(diff.abs() - pos.min(0.0).abs());
                let buy_volume = (diff - cover_volume).max(0.0);

                if cover_volume > 0.0 {
                    self.cover(vt_symbol, order_price, cover_volume, engine)?;
                }
                if buy_volume > 0.0 {
                    self.buy(vt_symbol, order_price, buy_volume, engine)?;
                }
            } else if diff < 0.0 {
                // Short position
                let order_price = bar.last_price * (1.0 - price_add);
                let sell_volume = (0.0_f64).min(diff.abs() - pos.max(0.0).abs());
                let short_volume = (diff.abs() - sell_volume).max(0.0);

                if sell_volume > 0.0 {
                    self.sell(vt_symbol, order_price, sell_volume, engine)?;
                }
                if short_volume > 0.0 {
                    self.short(vt_symbol, order_price, short_volume, engine)?;
                }
            }
        }
        Ok(())
    }

    /// Write log message
    pub fn write_log(&self, msg: &str, engine: &BacktestingEngine) {
        engine.write_log(msg, self);
    }

    /// Get available cash
    pub fn get_cash_available(&self, engine: &BacktestingEngine) -> f64 {
        engine.get_cash_available()
    }

    /// Get holding market value
    pub fn get_holding_value(&self, engine: &BacktestingEngine) -> f64 {
        engine.get_holding_value()
    }

    /// Get total portfolio value
    pub fn get_portfolio_value(&self, engine: &BacktestingEngine) -> f64 {
        self.get_cash_available(engine) + self.get_holding_value(engine)
    }
}