//! Futures Strategy Template
//!
//! Strategy template for futures trading with proper offset handling.
//! Handles SHFE/INE CloseToday/CloseYesterday splitting for Chinese futures exchanges.

use std::collections::HashMap;

use super::base::{StrategySetting, StrategyState, StrategyType, StopOrderRequest, CancelRequestType};
use super::template::{BaseStrategy, StrategyContext, StrategyTemplate};
use crate::trader::{
    BarData, Direction, Exchange, Offset, OrderData, OrderRequest, TickData, TradeData,
};

/// Offset mode for futures position closing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OffsetMode {
    /// Open new positions first (default for non-SHFE/INE)
    #[default]
    OpenFirst,
    /// Close yesterday positions first (SHFE/INE default)
    CloseYesterdayFirst,
    /// Lock mode (close opposite yesterday then open new)
    LockMode,
}

/// Futures strategy template with offset handling for Chinese exchanges
pub struct FuturesStrategy {
    /// Base strategy implementation
    base: BaseStrategy,
    
    /// Offset mode for position management
    pub offset_mode: OffsetMode,
    
    /// Today's long position
    pub long_td: f64,
    /// Yesterday's long position
    pub long_yd: f64,
    /// Today's short position
    pub short_td: f64,
    /// Yesterday's short position
    pub short_yd: f64,
    
    /// Frozen long today (pending close orders)
    long_td_frozen: f64,
    /// Frozen long yesterday
    long_yd_frozen: f64,
    /// Frozen short today
    short_td_frozen: f64,
    /// Frozen short yesterday
    short_yd_frozen: f64,
}

impl FuturesStrategy {
    /// Create a new futures strategy
    pub fn new(strategy_name: String, vt_symbols: Vec<String>, setting: StrategySetting) -> Self {
        Self {
            base: BaseStrategy::new(strategy_name, vt_symbols, StrategyType::Futures, setting),
            offset_mode: OffsetMode::CloseYesterdayFirst,
            long_td: 0.0,
            long_yd: 0.0,
            short_td: 0.0,
            short_yd: 0.0,
            long_td_frozen: 0.0,
            long_yd_frozen: 0.0,
            short_td_frozen: 0.0,
            short_yd_frozen: 0.0,
        }
    }
    
    /// Get total long position
    pub fn get_long_pos(&self) -> f64 {
        self.long_td + self.long_yd
    }
    
    /// Get total short position
    pub fn get_short_pos(&self) -> f64 {
        self.short_td + self.short_yd
    }
    
    /// Get net position (positive = long, negative = short)
    pub fn get_net_pos(&self) -> f64 {
        self.get_long_pos() - self.get_short_pos()
    }
    
    /// Check if exchange requires CloseToday/CloseYesterday splitting
    fn requires_offset_split(&self, exchange: Exchange) -> bool {
        matches!(exchange, Exchange::Shfe | Exchange::Ine)
    }
    
    /// Buy to open long position
    pub fn buy_open(&self, vt_symbol: &str, price: f64, volume: f64) -> String {
        self.base.buy(vt_symbol, price, volume, false)
    }
    
    /// Buy to close short position
    /// For SHFE/INE: splits into CloseToday/CloseYesterday based on available position
    pub fn buy_close(&mut self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        let exchange = self.get_exchange_from_symbol(vt_symbol);
        
        if self.requires_offset_split(exchange) {
            self.close_short_split(vt_symbol, price, volume)
        } else {
            // For non-SHFE/INE, simple Close offset
            let req = OrderRequest {
                symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                exchange,
                direction: Direction::Long,
                order_type: crate::trader::constant::OrderType::Limit,
                volume,
                price,
                offset: Offset::Close,
                reference: self.base.strategy_name.clone(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
            };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            let vt_orderid = format!("BUYCLOSE_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis());
            vec![vt_orderid]
        }
    }
    
    /// Sell to close long position
    /// For SHFE/INE: splits into CloseToday/CloseYesterday based on available position
    pub fn sell_close(&mut self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        let exchange = self.get_exchange_from_symbol(vt_symbol);
        
        if self.requires_offset_split(exchange) {
            self.close_long_split(vt_symbol, price, volume)
        } else {
            // For non-SHFE/INE, simple Close offset
            let req = OrderRequest {
                symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                exchange,
                direction: Direction::Short,
                order_type: crate::trader::constant::OrderType::Limit,
                volume,
                price,
                offset: Offset::Close,
                reference: self.base.strategy_name.clone(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
            };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            let vt_orderid = format!("SELLCLOSE_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis());
            vec![vt_orderid]
        }
    }
    
    /// Short to open short position
    pub fn short_open(&self, vt_symbol: &str, price: f64, volume: f64) -> String {
        self.base.short(vt_symbol, price, volume, false)
    }
    
    /// Cover to close short position (alias for buy_close)
    pub fn cover(&mut self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        self.buy_close(vt_symbol, price, volume)
    }
    
    /// Split close long position for SHFE/INE
    fn close_long_split(&mut self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        let exchange = self.get_exchange_from_symbol(vt_symbol);
        let mut vt_orderids = Vec::new();
        let mut remaining = volume;
        
        let td_available = (self.long_td - self.long_td_frozen).max(0.0);
        let yd_available = (self.long_yd - self.long_yd_frozen).max(0.0);
        
        // CloseToday first (today's position)
        if td_available > 0.0 && remaining > 0.0 {
            let close_vol = td_available.min(remaining);
            let req = OrderRequest {
                            symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                            exchange,
                            direction: Direction::Short,
                            order_type: crate::trader::constant::OrderType::Limit,
                            volume: close_vol,
                            price,
                            offset: Offset::CloseToday,
                            reference: self.base.strategy_name.clone(),
                            post_only: false,
                            reduce_only: false,
                            expire_time: None,
                        };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            vt_orderids.push(format!("CLOSETD_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis()));
            self.long_td_frozen += close_vol;
            remaining -= close_vol;
        }
        
        // CloseYesterday (yesterday's position)
        if yd_available > 0.0 && remaining > 0.0 {
            let close_vol = yd_available.min(remaining);
            let req = OrderRequest {
                            symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                            exchange,
                            direction: Direction::Short,
                            order_type: crate::trader::constant::OrderType::Limit,
                            volume: close_vol,
                            price,
                            offset: Offset::CloseYesterday,
                            reference: self.base.strategy_name.clone(),
                            post_only: false,
                            reduce_only: false,
                            expire_time: None,
                        };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            vt_orderids.push(format!("CLOSEYD_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis()));
            self.long_yd_frozen += close_vol;
        }
        
        vt_orderids
    }
    
    /// Split close short position for SHFE/INE
    fn close_short_split(&mut self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        let exchange = self.get_exchange_from_symbol(vt_symbol);
        let mut vt_orderids = Vec::new();
        let mut remaining = volume;
        
        let td_available = (self.short_td - self.short_td_frozen).max(0.0);
        let yd_available = (self.short_yd - self.short_yd_frozen).max(0.0);
        
        // CloseToday first (today's position)
        if td_available > 0.0 && remaining > 0.0 {
            let close_vol = td_available.min(remaining);
            let req = OrderRequest {
                            symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                            exchange,
                            direction: Direction::Long,
                            order_type: crate::trader::constant::OrderType::Limit,
                            volume: close_vol,
                            price,
                            offset: Offset::CloseToday,
                            reference: self.base.strategy_name.clone(),
                            post_only: false,
                            reduce_only: false,
                            expire_time: None,
                        };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            vt_orderids.push(format!("CLOSETD_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis()));
            self.short_td_frozen += close_vol;
            remaining -= close_vol;
        }
        
        // CloseYesterday (yesterday's position)
        if yd_available > 0.0 && remaining > 0.0 {
            let close_vol = yd_available.min(remaining);
            let req = OrderRequest {
                            symbol: vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string(),
                            exchange,
                            direction: Direction::Long,
                            order_type: crate::trader::constant::OrderType::Limit,
                            volume: close_vol,
                            price,
                            offset: Offset::CloseYesterday,
                            reference: self.base.strategy_name.clone(),
                            post_only: false,
                            reduce_only: false,
                            expire_time: None,
                        };
            self.base.pending_orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            vt_orderids.push(format!("CLOSEYD_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis()));
            self.short_yd_frozen += close_vol;
        }
        
        vt_orderids
    }
    
    /// Get exchange from vt_symbol
    fn get_exchange_from_symbol(&self, vt_symbol: &str) -> Exchange {
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        if parts.len() == 2 {
            match parts[1].to_uppercase().as_str() {
                "BINANCE" => Exchange::Binance,
                "OKX" => Exchange::Okx,
                "BYBIT" => Exchange::Bybit,
                "SHFE" => Exchange::Shfe,
                "INE" => Exchange::Ine,
                "DCE" => Exchange::Dce,
                "CZCE" => Exchange::Czce,
                "CFFEX" => Exchange::Cffex,
                "GFEX" => Exchange::Gfex,
                "SSE" => Exchange::Sse,
                "SZSE" => Exchange::Szse,
                _ => Exchange::Local,
            }
        } else {
            Exchange::Local
        }
    }
    
    /// Update position tracking from trade
    pub fn update_position_from_trade(&mut self, trade: &TradeData) {
        let direction = match trade.direction {
            Some(d) => d,
            None => return,
        };
        
        match direction {
            Direction::Long => {
                match trade.offset {
                    Offset::Open => {
                        self.long_td += trade.volume;
                    }
                    Offset::CloseToday => {
                        self.short_td = (self.short_td - trade.volume).max(0.0);
                        self.short_td_frozen = (self.short_td_frozen - trade.volume).max(0.0);
                    }
                    Offset::CloseYesterday => {
                        self.short_yd = (self.short_yd - trade.volume).max(0.0);
                        self.short_yd_frozen = (self.short_yd_frozen - trade.volume).max(0.0);
                    }
                    Offset::Close => {
                        // Non-SHFE/INE: reduce short_td first, then short_yd
                        let mut vol = trade.volume;
                        if self.short_td >= vol {
                            self.short_td -= vol;
                        } else {
                            vol -= self.short_td;
                            self.short_td = 0.0;
                            self.short_yd = (self.short_yd - vol).max(0.0);
                        }
                    }
                    _ => {}
                }
            }
            Direction::Short => {
                match trade.offset {
                    Offset::Open => {
                        self.short_td += trade.volume;
                    }
                    Offset::CloseToday => {
                        self.long_td = (self.long_td - trade.volume).max(0.0);
                        self.long_td_frozen = (self.long_td_frozen - trade.volume).max(0.0);
                    }
                    Offset::CloseYesterday => {
                        self.long_yd = (self.long_yd - trade.volume).max(0.0);
                        self.long_yd_frozen = (self.long_yd_frozen - trade.volume).max(0.0);
                    }
                    Offset::Close => {
                        // Non-SHFE/INE: reduce long_td first, then long_yd
                        let mut vol = trade.volume;
                        if self.long_td >= vol {
                            self.long_td -= vol;
                        } else {
                            vol -= self.long_td;
                            self.long_td = 0.0;
                            self.long_yd = (self.long_yd - vol).max(0.0);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl StrategyTemplate for FuturesStrategy {
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
        vars.insert("long_td".to_string(), format!("{:.4}", self.long_td));
        vars.insert("long_yd".to_string(), format!("{:.4}", self.long_yd));
        vars.insert("short_td".to_string(), format!("{:.4}", self.short_td));
        vars.insert("short_yd".to_string(), format!("{:.4}", self.short_yd));
        vars
    }
    
    fn on_init(&mut self, _context: &StrategyContext) {
        self.base.write_log("FuturesStrategy初始化");
        self.base.state = StrategyState::Inited;
    }
    
    fn on_start(&mut self) {
        self.base.write_log("FuturesStrategy启动");
        self.base.state = StrategyState::Trading;
    }
    
    fn on_stop(&mut self) {
        self.base.write_log("FuturesStrategy停止");
        self.base.state = StrategyState::Stopped;
    }
    
    fn on_tick(&mut self, _tick: &TickData, _context: &StrategyContext) {}
    
    fn on_bar(&mut self, _bar: &BarData, _context: &StrategyContext) {}
    
    fn on_order(&mut self, _order: &OrderData) {}
    
    fn on_trade(&mut self, trade: &TradeData) {
        self.update_position_from_trade(trade);
        self.base.write_log(&format!(
            "成交: {:?} {}@{:.2} offset={:?}",
            trade.direction, trade.volume, trade.price, trade.offset
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
    fn test_futures_strategy_new() {
        let setting = StrategySetting::new();
        let strategy = FuturesStrategy::new(
            "TestFutures".to_string(),
            vec!["IF2312.CFFEX".to_string()],
            setting,
        );
        assert_eq!(strategy.strategy_name(), "TestFutures");
        assert_eq!(strategy.strategy_type(), StrategyType::Futures);
        assert_eq!(strategy.offset_mode, OffsetMode::CloseYesterdayFirst);
    }
    
    #[test]
    fn test_position_tracking() {
        let setting = StrategySetting::new();
        let mut strategy = FuturesStrategy::new(
            "TestFutures".to_string(),
            vec!["IF2312.CFFEX".to_string()],
            setting,
        );
        
        // Open long position
        let mut trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "IF2312".to_string(),
            exchange: Exchange::Cffex,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 4000.0,
            volume: 1.0,
            datetime: None,
            extra: None,
        };
        strategy.update_position_from_trade(&trade);
        assert!((strategy.long_td - 1.0).abs() < 1e-10);
        
        // Close today
        trade.direction = Some(Direction::Short);
        trade.offset = Offset::CloseToday;
        strategy.update_position_from_trade(&trade);
        assert!((strategy.long_td - 0.0).abs() < 1e-10);
    }
    
    #[test]
    fn test_offset_split_detection() {
        let setting = StrategySetting::new();
        let strategy = FuturesStrategy::new(
            "TestFutures".to_string(),
            vec!["au2406.SHFE".to_string()],
            setting,
        );
        
        assert!(strategy.requires_offset_split(Exchange::Shfe));
        assert!(strategy.requires_offset_split(Exchange::Ine));
        assert!(!strategy.requires_offset_split(Exchange::Dce));
        assert!(!strategy.requires_offset_split(Exchange::Cffex));
    }
}
