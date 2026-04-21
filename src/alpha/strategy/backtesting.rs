//! Backtesting engine for alpha strategies
//! Provides backtesting functionality for alpha trading strategies

use chrono::{DateTime, NaiveDate, Utc};
#[cfg(feature = "alpha")]
use polars::prelude::*;
use std::collections::HashMap;

use crate::alpha::model::AlphaModel;
use crate::alpha::strategy::template::AlphaStrategy;
use crate::alpha::types::AlphaBarData;
use crate::trader::{Direction, Offset, OrderData, OrderType, Status, TradeData};

pub struct BacktestingEngine {
    pub capital: f64,
    pub risk_free: f64,
    pub annual_days: u32,

    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,

    pub strategy: Option<AlphaStrategy>,
    pub model: Option<Box<dyn AlphaModel>>,

    pub daily_results: HashMap<NaiveDate, PortfolioDailyResult>,
    #[cfg(feature = "alpha")]
    pub daily_df: Option<DataFrame>,
    #[cfg(not(feature = "alpha"))]
    pub daily_df: Option<()>,

    pub cash: f64,

    pub bars: HashMap<String, crate::trader::TickData>,
    pub datetime: Option<DateTime<Utc>>,

    pub data: HashMap<String, Vec<AlphaBarData>>,
    pub positions: HashMap<String, f64>,
    pub position_prices: HashMap<String, f64>,
    pub rate: f64,
    pub slippage: f64,
    pub size: f64,
    pub pricetick: f64,
    pub trade_count: u32,
    pub trades: HashMap<String, TradeData>,
    pub limit_orders: HashMap<String, OrderData>,
    pub active_limit_orders: HashMap<String, OrderData>,
}

impl BacktestingEngine {
    pub fn new() -> Self {
        BacktestingEngine {
            capital: 1_000_000.0,
            risk_free: 0.0,
            annual_days: 240,
            start: Utc::now(),
            end: Utc::now(),
            strategy: None,
            model: None,
            daily_results: HashMap::new(),
            daily_df: None,
            cash: 1_000_000.0,
            bars: HashMap::new(),
            datetime: None,
            data: HashMap::new(),
            positions: HashMap::new(),
            position_prices: HashMap::new(),
            rate: 0.0,
            slippage: 0.0,
            size: 1.0,
            pricetick: 0.0,
            trade_count: 0,
            trades: HashMap::new(),
            limit_orders: HashMap::new(),
            active_limit_orders: HashMap::new(),
        }
    }

    pub fn set_parameters(
        &mut self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        capital: f64,
        risk_free: f64,
        annual_days: u32,
    ) {
        self.start = start;
        self.end = end;
        self.capital = capital;
        self.risk_free = risk_free;
        self.annual_days = annual_days;
        self.cash = capital;
    }

    /// Set trading cost parameters
    pub fn set_cost(&mut self, rate: f64, slippage: f64, size: f64, pricetick: f64) {
        self.rate = rate;
        self.slippage = slippage;
        self.size = size;
        self.pricetick = pricetick;
    }

    /// Add historical bar data for a symbol
    pub fn add_data(&mut self, vt_symbol: &str, bars: Vec<AlphaBarData>) {
        self.data.insert(vt_symbol.to_string(), bars);
    }

    pub fn add_strategy(&mut self, strategy: AlphaStrategy) {
        self.strategy = Some(strategy);
    }

    pub fn add_model(&mut self, model: Box<dyn AlphaModel>) {
        self.model = Some(model);
    }

    pub fn load_data(&mut self, _vt_symbols: Vec<String>) {
        println!("Loading historical data...");
    }

    pub fn run_backtesting(&mut self) {
        println!("Running backtesting...");

        if let Some(ref mut strategy) = self.strategy {
            strategy.on_init();
            println!("Strategy initialized");
        }

        let mut all_bars: Vec<AlphaBarData> = Vec::new();
        for bars in self.data.values() {
            for bar in bars {
                all_bars.push(bar.clone());
            }
        }
        all_bars.sort_by_key(|b| b.datetime);

        if all_bars.is_empty() {
            println!("No historical data available for backtesting");
            if let Some(ref mut strategy) = self.strategy {
                strategy.on_stop();
            }
            return;
        }

        for bar in &all_bars {
            self.datetime = Some(bar.datetime);

            let tick = self.bar_to_tick(bar);
            self.bars.insert(bar.vt_symbol(), tick);

            if let Some(ref mut strategy) = self.strategy {
                let bars_map: HashMap<String, crate::trader::TickData> = self.bars.clone();
                strategy.on_bars(&bars_map);
            }

            self.process_orders(bar);
        }

        if let Some(ref mut strategy) = self.strategy {
            strategy.on_stop();
        }

        self.calculate_result();

        println!("Backtesting completed");
    }

    /// Convert an AlphaBarData to TickData for strategy consumption
    fn bar_to_tick(&self, bar: &AlphaBarData) -> crate::trader::TickData {
        crate::trader::TickData {
            gateway_name: "BACKTEST".to_string(),
            symbol: bar.symbol.clone(),
            exchange: bar.exchange,
            datetime: bar.datetime,
            name: String::new(),
            volume: bar.volume,
            turnover: bar.turnover,
            open_interest: bar.open_interest,
            last_price: bar.close,
            last_volume: bar.volume,
            limit_up: 0.0,
            limit_down: 0.0,
            open_price: bar.open,
            high_price: bar.high,
            low_price: bar.low,
            pre_close: bar.close,
            bid_price_1: bar.close,
            bid_price_2: 0.0,
            bid_price_3: 0.0,
            bid_price_4: 0.0,
            bid_price_5: 0.0,
            ask_price_1: bar.close,
            ask_price_2: 0.0,
            ask_price_3: 0.0,
            ask_price_4: 0.0,
            ask_price_5: 0.0,
            bid_volume_1: 0.0,
            bid_volume_2: 0.0,
            bid_volume_3: 0.0,
            bid_volume_4: 0.0,
            bid_volume_5: 0.0,
            ask_volume_1: 0.0,
            ask_volume_2: 0.0,
            ask_volume_3: 0.0,
            ask_volume_4: 0.0,
            ask_volume_5: 0.0,
            localtime: None,
            extra: None,
        }
    }

    fn process_orders(&mut self, bar: &AlphaBarData) {
        let vt_symbol = bar.vt_symbol();
        let close_price = bar.close;

        let order_ids: Vec<String> = self.active_limit_orders.keys().cloned().collect();
        let mut filled_orders = Vec::new();

        for orderid in order_ids {
            if let Some(order) = self.active_limit_orders.get(&orderid) {
                if order.vt_symbol() != vt_symbol {
                    continue;
                }

                let can_fill = match order.direction {
                    Some(Direction::Long) => {
                        close_price <= order.price || order.order_type == OrderType::Market
                    }
                    Some(Direction::Short) => {
                        close_price >= order.price || order.order_type == OrderType::Market
                    }
                    None | Some(Direction::Net) => false,
                };

                if can_fill {
                    filled_orders.push(orderid);
                }
            }
        }

        for orderid in filled_orders {
            if let Some(order) = self.active_limit_orders.remove(&orderid) {
                self.cross_order(order, close_price);
            }
        }
    }

    fn cross_order(&mut self, order: OrderData, fill_price: f64) {
        let trade_price = match order.direction {
            Some(Direction::Long) => fill_price + self.slippage,
            Some(Direction::Short) => fill_price - self.slippage,
            None | Some(Direction::Net) => fill_price,
        };

        let trade_value = trade_price * order.volume * self.size;
        let commission = trade_value * self.rate;

        self.trade_count += 1;
        let tradeid = format!("{}", self.trade_count);
        let trade = TradeData {
            gateway_name: "BACKTEST".to_string(),
            symbol: order.symbol.clone(),
            exchange: order.exchange,
            orderid: order.orderid.clone(),
            tradeid,
            direction: order.direction,
            offset: order.offset,
            price: trade_price,
            volume: order.volume,
            datetime: self.datetime,
            extra: None,
        };

        let vt_tradeid = trade.vt_tradeid();

        let vt_symbol = order.vt_symbol();
        let pos = self.positions.entry(vt_symbol.clone()).or_insert(0.0);
        let prev_pos = *pos;

        let delta = match (order.direction, order.offset) {
            (Some(Direction::Long), Offset::Open) => order.volume,
            (Some(Direction::Long), Offset::Close) => order.volume,
            (Some(Direction::Short), Offset::Open) => -order.volume,
            (Some(Direction::Short), Offset::Close) => -order.volume,
            _ => 0.0,
        };

        *pos += delta;

        if order.offset == Offset::Open {
            let prev_price = *self.position_prices.entry(vt_symbol.clone()).or_insert(0.0);
            if prev_pos.abs() < 1e-10 {
                if let Some(price_entry) = self.position_prices.get_mut(&vt_symbol) {
                    *price_entry = trade_price;
                }
            } else {
                let new_avg = (prev_pos * prev_price + delta * trade_price) / pos.abs();
                if let Some(price_entry) = self.position_prices.get_mut(&vt_symbol) {
                    *price_entry = new_avg;
                }
            }
        }

        let cash_delta = match order.direction {
            Some(Direction::Long) => -(trade_value + commission),
            Some(Direction::Short) => trade_value - commission,
            None | Some(Direction::Net) => 0.0,
        };
        self.cash += cash_delta;

        if let Some(ref mut strategy) = self.strategy {
            strategy.update_trade(&trade);
        }

        self.trades.insert(vt_tradeid, trade);
    }

    #[cfg(feature = "alpha")]
    pub fn calculate_result(&mut self) -> Option<DataFrame> {
        println!("Calculating backtesting results...");

        let mut daily_pnl: HashMap<NaiveDate, f64> = HashMap::new();
        for trade in self.trades.values() {
            if let Some(dt) = trade.datetime {
                let date = dt.date_naive();
                let trade_value = trade.price * trade.volume * self.size;
                let commission = trade_value * self.rate;
                let pnl = match (trade.direction, trade.offset) {
                    (Some(Direction::Long), Offset::Close) => trade_value - commission,
                    (Some(Direction::Short), Offset::Close) => -(trade_value + commission),
                    _ => -commission,
                };
                *daily_pnl.entry(date).or_insert(0.0) += pnl;
            }
        }

        for (vt_symbol, pos_qty) in &self.positions {
            if pos_qty.abs() > 1e-10 {
                if let Some(tick) = self.bars.get(vt_symbol) {
                    let entry_price = self.position_prices.get(vt_symbol).copied().unwrap_or(0.0);
                    let holding_pnl = pos_qty * (tick.last_price - entry_price) * self.size;
                    if let Some(dt) = self.datetime {
                        let date = dt.date_naive();
                        *daily_pnl.entry(date).or_insert(0.0) += holding_pnl;
                    }
                }
            }
        }

        if daily_pnl.is_empty() {
            let df = DataFrame::new(vec![
                Column::new("date".into(), Vec::<String>::new()),
                Column::new("return".into(), Vec::<f64>::new()),
            ])
            .ok();
            self.daily_df = df.clone();
            return df;
        }

        let mut dates: Vec<String> = daily_pnl.keys().map(|d| d.to_string()).collect();
        dates.sort();

        let returns: Vec<f64> = dates
            .iter()
            .map(|d| {
                let date = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                date.and_then(|nd| daily_pnl.get(&nd).copied())
                    .unwrap_or(0.0)
                    / self.capital
            })
            .collect();

        let df = DataFrame::new(vec![
            Column::new("date".into(), dates),
            Column::new("return".into(), returns),
        ])
        .ok();

        self.daily_df = df.clone();
        df
    }

    #[cfg(not(feature = "alpha"))]
    pub fn calculate_result(&mut self) -> Option<()> {
        println!("Calculating backtesting results...");
        None
    }

    pub fn calculate_statistics(&self) -> HashMap<String, f64> {
        println!("Calculating statistics...");

        let mut stats = HashMap::new();

        let mut total_pnl = 0.0;
        let mut trade_pnls: Vec<f64> = Vec::new();
        for trade in self.trades.values() {
            let trade_value = trade.price * trade.volume * self.size;
            let commission = trade_value * self.rate;
            let pnl = match (trade.direction, trade.offset) {
                (Some(Direction::Long), Offset::Open) => -trade_value - commission,
                (Some(Direction::Long), Offset::Close) => trade_value - commission,
                (Some(Direction::Short), Offset::Open) => trade_value - commission,
                (Some(Direction::Short), Offset::Close) => -trade_value - commission,
                _ => -commission,
            };
            total_pnl += pnl;
            trade_pnls.push(pnl);
        }

        for (vt_symbol, pos_qty) in &self.positions {
            if pos_qty.abs() > 1e-10 {
                if let Some(tick) = self.bars.get(vt_symbol) {
                    let entry_price = self.position_prices.get(vt_symbol).copied().unwrap_or(0.0);
                    total_pnl += pos_qty * (tick.last_price - entry_price) * self.size;
                }
            }
        }

        stats.insert("total_return".to_string(), total_pnl / self.capital);

        if trade_pnls.len() > 1 {
            let mean_pnl = trade_pnls.iter().sum::<f64>() / trade_pnls.len() as f64;
            let variance = trade_pnls
                .iter()
                .map(|p| (p - mean_pnl).powi(2))
                .sum::<f64>()
                / (trade_pnls.len() - 1) as f64;
            let std_pnl = variance.sqrt();
            let sharpe = if std_pnl > 1e-10 {
                (mean_pnl / std_pnl) * (self.annual_days as f64).sqrt()
            } else {
                0.0
            };
            stats.insert("sharpe_ratio".to_string(), sharpe);
        } else {
            stats.insert("sharpe_ratio".to_string(), 0.0);
        }

        let mut equity = self.capital;
        let mut peak = equity;
        let mut max_dd = 0.0;
        for pnl in &trade_pnls {
            equity += pnl;
            if equity > peak {
                peak = equity;
            }
            let dd = (equity - peak) / peak;
            if dd < max_dd {
                max_dd = dd;
            }
        }
        stats.insert("max_drawdown".to_string(), max_dd);

        stats.insert("trade_count".to_string(), self.trades.len() as f64);

        if !trade_pnls.is_empty() {
            let wins = trade_pnls.iter().filter(|p| **p > 0.0).count();
            stats.insert(
                "win_rate".to_string(),
                wins as f64 / trade_pnls.len() as f64,
            );
        } else {
            stats.insert("win_rate".to_string(), 0.0);
        }

        stats
    }

    #[cfg(feature = "alpha")]
    pub fn get_signal(&self) -> DataFrame {
        DataFrame::default()
    }

    #[cfg(not(feature = "alpha"))]
    pub fn get_signal(&self) -> () {}

    pub fn send_order(
        &mut self,
        vt_symbol: &str,
        direction: Direction,
        offset: Offset,
        price: f64,
        volume: f64,
    ) -> Vec<String> {
        let orderid = format!("ORDER_{}", uuid::Uuid::new_v4());
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        let symbol = parts.first().unwrap_or(&"").to_string();
        let exchange = if parts.len() > 1 {
            match parts[1] {
                "BINANCE" => crate::trader::Exchange::Binance,
                "BINANCE_USDM" => crate::trader::Exchange::BinanceUsdm,
                "BINANCE_COINM" => crate::trader::Exchange::BinanceCoinm,
                other => {
                    tracing::warn!("Unknown exchange '{}', defaulting to Binance", other);
                    crate::trader::Exchange::Binance
                }
            }
        } else {
            crate::trader::Exchange::Binance
        };

        let order = OrderData {
            gateway_name: "BACKTEST".to_string(),
            symbol,
            exchange,
            orderid: orderid.clone(),
            order_type: OrderType::Limit,
            direction: Some(direction),
            offset,
            price,
            volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: self.datetime,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        self.active_limit_orders.insert(orderid.clone(), order);
        vec![orderid]
    }

    pub fn cancel_order(&mut self, vt_orderid: &str) {
        self.active_limit_orders.remove(vt_orderid);
    }

    pub fn write_log(&self, msg: &str, _strategy: &AlphaStrategy) {
        println!("[BACKTEST] {}", msg);
    }

    pub fn get_cash_available(&self) -> f64 {
        self.cash
    }

    pub fn get_holding_value(&self) -> f64 {
        let mut value = 0.0;
        for (vt_symbol, pos_qty) in &self.positions {
            if pos_qty.abs() > 1e-10 {
                if let Some(tick) = self.bars.get(vt_symbol) {
                    value += pos_qty * tick.last_price * self.size;
                }
            }
        }
        value
    }
}

impl Default for BacktestingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct ContractDailyResult {
    pub date: NaiveDate,
    pub close_price: f64,
    pub pre_close: f64,
    pub trades: Vec<crate::trader::TradeData>,
    pub trade_count: u32,
    pub start_pos: f64,
    pub end_pos: f64,
    pub turnover: f64,
    pub commission: f64,
    pub trading_pnl: f64,
    pub holding_pnl: f64,
    pub total_pnl: f64,
    pub net_pnl: f64,
}

impl ContractDailyResult {
    pub fn new(date: NaiveDate, close_price: f64) -> Self {
        ContractDailyResult {
            date,
            close_price,
            pre_close: 0.0,
            trades: Vec::new(),
            trade_count: 0,
            start_pos: 0.0,
            end_pos: 0.0,
            turnover: 0.0,
            commission: 0.0,
            trading_pnl: 0.0,
            holding_pnl: 0.0,
            total_pnl: 0.0,
            net_pnl: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct PortfolioDailyResult {
    pub date: NaiveDate,
    pub close_prices: HashMap<String, f64>,
    pub pre_closes: HashMap<String, f64>,
    pub start_poses: HashMap<String, f64>,
    pub end_poses: HashMap<String, f64>,
    pub contract_results: HashMap<String, ContractDailyResult>,
    pub trade_count: u32,
    pub turnover: f64,
    pub commission: f64,
    pub trading_pnl: f64,
    pub holding_pnl: f64,
    pub total_pnl: f64,
    pub net_pnl: f64,
}

impl PortfolioDailyResult {
    pub fn new(date: NaiveDate, close_prices: HashMap<String, f64>) -> Self {
        let mut contract_results = HashMap::new();
        for (vt_symbol, close_price) in &close_prices {
            contract_results.insert(
                vt_symbol.clone(),
                ContractDailyResult::new(date, *close_price),
            );
        }

        PortfolioDailyResult {
            date,
            close_prices,
            pre_closes: HashMap::new(),
            start_poses: HashMap::new(),
            end_poses: HashMap::new(),
            contract_results,
            trade_count: 0,
            turnover: 0.0,
            commission: 0.0,
            trading_pnl: 0.0,
            holding_pnl: 0.0,
            total_pnl: 0.0,
            net_pnl: 0.0,
        }
    }

    pub fn add_trade(&mut self, trade: &crate::trader::TradeData) {
        if let Some(contract_result) = self.contract_results.get_mut(&trade.vt_symbol()) {
            contract_result.trades.push(trade.clone());
            contract_result.trade_count += 1;
        }
    }
}
