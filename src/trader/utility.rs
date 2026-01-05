//! General utility functions.

use chrono::Timelike;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde_json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

// Technical Analysis imports from ta crate
use ta::indicators::{
    AverageTrueRange, BollingerBands, CommodityChannelIndex, ExponentialMovingAverage,
    FastStochastic, KeltnerChannel, Maximum, Minimum, MoneyFlowIndex,
    MovingAverageConvergenceDivergence, OnBalanceVolume, RateOfChange, RelativeStrengthIndex,
    SimpleMovingAverage, SlowStochastic, StandardDeviation, TrueRange,
};
use ta::{Close, High, Low, Next, Open, Volume};

use super::constant::{Exchange, Interval};
use super::object::{BarData, TickData};

/// Extract symbol and exchange from vt_symbol
pub fn extract_vt_symbol(vt_symbol: &str) -> Option<(String, Exchange)> {
    let parts: Vec<&str> = vt_symbol.rsplitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let exchange_str = parts[0];
    let symbol = parts[1].to_string();
    
    // Parse exchange from string
    let exchange = match exchange_str {
        "CFFEX" => Exchange::Cffex,
        "SHFE" => Exchange::Shfe,
        "CZCE" => Exchange::Czce,
        "DCE" => Exchange::Dce,
        "INE" => Exchange::Ine,
        "GFEX" => Exchange::Gfex,
        "SSE" => Exchange::Sse,
        "SZSE" => Exchange::Szse,
        "BSE" => Exchange::Bse,
        "BINANCE" => Exchange::Binance,
        "BINANCE_USDM" => Exchange::BinanceUsdm,
        "BINANCE_COINM" => Exchange::BinanceCoinm,
        "LOCAL" => Exchange::Local,
        "GLOBAL" => Exchange::Global,
        _ => return None,
    };
    
    Some((symbol, exchange))
}

/// Generate vt_symbol from symbol and exchange
pub fn generate_vt_symbol(symbol: &str, exchange: Exchange) -> String {
    format!("{}.{}", symbol, exchange.value())
}

/// Get trader directory
fn get_trader_dir(temp_name: &str) -> (PathBuf, PathBuf) {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let temp_path = cwd.join(temp_name);
    
    // If .rstrader folder exists in current working directory, use it
    if temp_path.exists() {
        return (cwd, temp_path);
    }
    
    // Otherwise use home path
    let home_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let temp_path = home_path.join(temp_name);
    
    // Create folder if not exists
    if !temp_path.exists() {
        let _ = fs::create_dir_all(&temp_path);
    }
    
    (home_path, temp_path)
}

/// Trader directory
pub static TRADER_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let (trader_dir, _) = get_trader_dir(".rstrader");
    trader_dir
});

/// Temp directory
pub static TEMP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let (_, temp_dir) = get_trader_dir(".rstrader");
    temp_dir
});

/// Get path for temp file with filename
pub fn get_file_path(filename: &str) -> PathBuf {
    TEMP_DIR.join(filename)
}

/// Get path for temp folder with folder name
pub fn get_folder_path(folder_name: &str) -> PathBuf {
    let folder_path = TEMP_DIR.join(folder_name);
    if !folder_path.exists() {
        let _ = fs::create_dir_all(&folder_path);
    }
    folder_path
}

/// Get path for icon file with ico name
pub fn get_icon_path(filepath: &str, ico_name: &str) -> PathBuf {
    let ui_path = Path::new(filepath).parent().unwrap_or(Path::new("."));
    ui_path.join("ico").join(ico_name)
}

/// Load data from JSON file in temp path
pub fn load_json(filename: &str) -> HashMap<String, serde_json::Value> {
    let filepath = get_file_path(filename);
    
    if filepath.exists() {
        if let Ok(content) = fs::read_to_string(&filepath) {
            if let Ok(data) = serde_json::from_str(&content) {
                return data;
            }
        }
    }
    
    // Save empty JSON and return empty map
    save_json(filename, &HashMap::new());
    HashMap::new()
}

/// Save data into JSON file in temp path
pub fn save_json(filename: &str, data: &HashMap<String, serde_json::Value>) {
    let filepath = get_file_path(filename);
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = fs::write(filepath, json);
    }
}

/// Round price to price tick value
pub fn round_to(value: f64, target: f64) -> f64 {
    let decimal_value = Decimal::from_f64(value).unwrap_or_default();
    let decimal_target = Decimal::from_f64(target).unwrap_or(Decimal::ONE);
    
    if decimal_target.is_zero() {
        return value;
    }
    
    let result = (decimal_value / decimal_target).round() * decimal_target;
    result.to_f64().unwrap_or(value)
}

/// Floor to target float number
pub fn floor_to(value: f64, target: f64) -> f64 {
    let decimal_value = Decimal::from_f64(value).unwrap_or_default();
    let decimal_target = Decimal::from_f64(target).unwrap_or(Decimal::ONE);
    
    if decimal_target.is_zero() {
        return value;
    }
    
    let result = (decimal_value / decimal_target).floor() * decimal_target;
    result.to_f64().unwrap_or(value)
}

/// Ceil to target float number
pub fn ceil_to(value: f64, target: f64) -> f64 {
    let decimal_value = Decimal::from_f64(value).unwrap_or_default();
    let decimal_target = Decimal::from_f64(target).unwrap_or(Decimal::ONE);
    
    if decimal_target.is_zero() {
        return value;
    }
    
    let result = (decimal_value / decimal_target).ceil() * decimal_target;
    result.to_f64().unwrap_or(value)
}

/// Get number of digits after decimal point
pub fn get_digits(value: f64) -> usize {
    let value_str = format!("{}", value);
    
    if value_str.contains("e-") {
        let parts: Vec<&str> = value_str.split("e-").collect();
        if parts.len() == 2 {
            return parts[1].parse().unwrap_or(0);
        }
    } else if value_str.contains('.') {
        let parts: Vec<&str> = value_str.split('.').collect();
        if parts.len() == 2 {
            return parts[1].len();
        }
    }
    
    0
}

/// Bar generator for generating bar data from tick data
#[allow(dead_code)]
pub struct BarGenerator<F, W>
where
    F: FnMut(BarData),
    W: FnMut(BarData),
{
    bar: Option<BarData>,
    on_bar: F,
    
    interval: Interval,
    interval_count: i32,
    
    hour_bar: Option<BarData>,
    daily_bar: Option<BarData>,
    
    window: i32,
    window_bar: Option<BarData>,
    on_window_bar: Option<W>,
    
    last_tick: Option<TickData>,
    
    daily_end: Option<chrono::NaiveTime>,
}

impl<F, W> BarGenerator<F, W>
where
    F: FnMut(BarData),
    W: FnMut(BarData),
{
    /// Create a new BarGenerator
    pub fn new(on_bar: F, window: i32, on_window_bar: Option<W>, interval: Interval) -> Self {
        Self {
            bar: None,
            on_bar,
            interval,
            interval_count: 0,
            hour_bar: None,
            daily_bar: None,
            window,
            window_bar: None,
            on_window_bar,
            last_tick: None,
            daily_end: None,
        }
    }

    /// Update new tick data into generator
    pub fn update_tick(&mut self, tick: TickData) {
        let mut new_minute = false;

        // Filter tick data with 0 last price
        if tick.last_price == 0.0 {
            return;
        }

        if self.bar.is_none() {
            new_minute = true;
        } else if let Some(ref bar) = self.bar {
            let bar_minute = bar.datetime.format("%M").to_string();
            let tick_minute = tick.datetime.format("%M").to_string();
            let bar_hour = bar.datetime.format("%H").to_string();
            let tick_hour = tick.datetime.format("%H").to_string();
            
            if bar_minute != tick_minute || bar_hour != tick_hour {
                // Call on_bar callback
                let mut finished_bar = self.bar.take().unwrap();
                finished_bar.datetime = finished_bar.datetime
                    .with_second(0).unwrap()
                    .with_nanosecond(0).unwrap();
                (self.on_bar)(finished_bar);
                new_minute = true;
            }
        }

        if new_minute {
            self.bar = Some(BarData {
                gateway_name: tick.gateway_name.clone(),
                symbol: tick.symbol.clone(),
                exchange: tick.exchange,
                datetime: tick.datetime,
                interval: Some(Interval::Minute),
                volume: 0.0,
                turnover: 0.0,
                open_interest: tick.open_interest,
                open_price: tick.last_price,
                high_price: tick.last_price,
                low_price: tick.last_price,
                close_price: tick.last_price,
                extra: None,
            });
        } else if let Some(ref mut bar) = self.bar {
            bar.high_price = bar.high_price.max(tick.last_price);
            bar.low_price = bar.low_price.min(tick.last_price);
            bar.close_price = tick.last_price;
            bar.open_interest = tick.open_interest;
            bar.datetime = tick.datetime;
        }

        // Update volume
        if let (Some(ref last_tick), Some(ref mut bar)) = (&self.last_tick, &mut self.bar) {
            let volume_change = tick.volume - last_tick.volume;
            bar.volume += volume_change.max(0.0);

            let turnover_change = tick.turnover - last_tick.turnover;
            bar.turnover += turnover_change.max(0.0);
        }

        self.last_tick = Some(tick);
    }

    /// Generate the bar data and call callback immediately
    pub fn generate(&mut self) -> Option<BarData> {
        if let Some(mut bar) = self.bar.take() {
            bar.datetime = bar.datetime
                .with_second(0).unwrap()
                .with_nanosecond(0).unwrap();
            (self.on_bar)(bar.clone());
            return Some(bar);
        }
        None
    }
}

/// Array manager for time series container of bar data and calculating technical indicators
/// Uses ta-rs library for technical indicator calculations
pub struct ArrayManager {
    count: usize,
    size: usize,
    inited: bool,
    
    pub open_array: Vec<f64>,
    pub high_array: Vec<f64>,
    pub low_array: Vec<f64>,
    pub close_array: Vec<f64>,
    pub volume_array: Vec<f64>,
    pub turnover_array: Vec<f64>,
    pub open_interest_array: Vec<f64>,
}

/// Helper struct to implement OHLCV traits for ta-rs
struct BarDataItem {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl Open for BarDataItem {
    fn open(&self) -> f64 {
        self.open
    }
}

impl High for BarDataItem {
    fn high(&self) -> f64 {
        self.high
    }
}

impl Low for BarDataItem {
    fn low(&self) -> f64 {
        self.low
    }
}

impl Close for BarDataItem {
    fn close(&self) -> f64 {
        self.close
    }
}

impl Volume for BarDataItem {
    fn volume(&self) -> f64 {
        self.volume
    }
}

impl ArrayManager {
    /// Create a new ArrayManager
    pub fn new(size: usize) -> Self {
        Self {
            count: 0,
            size,
            inited: false,
            open_array: vec![0.0; size],
            high_array: vec![0.0; size],
            low_array: vec![0.0; size],
            close_array: vec![0.0; size],
            volume_array: vec![0.0; size],
            turnover_array: vec![0.0; size],
            open_interest_array: vec![0.0; size],
        }
    }

    /// Check if initialized
    pub fn is_inited(&self) -> bool {
        self.inited
    }

    /// Update new bar data into array manager
    pub fn update_bar(&mut self, bar: &BarData) {
        self.count += 1;
        if !self.inited && self.count >= self.size {
            self.inited = true;
        }

        // Shift arrays
        self.open_array.rotate_left(1);
        self.high_array.rotate_left(1);
        self.low_array.rotate_left(1);
        self.close_array.rotate_left(1);
        self.volume_array.rotate_left(1);
        self.turnover_array.rotate_left(1);
        self.open_interest_array.rotate_left(1);

        // Update last values
        let last = self.size - 1;
        self.open_array[last] = bar.open_price;
        self.high_array[last] = bar.high_price;
        self.low_array[last] = bar.low_price;
        self.close_array[last] = bar.close_price;
        self.volume_array[last] = bar.volume;
        self.turnover_array[last] = bar.turnover;
        self.open_interest_array[last] = bar.open_interest;
    }

    /// Get open price array
    pub fn open(&self) -> &[f64] {
        &self.open_array
    }

    /// Get high price array
    pub fn high(&self) -> &[f64] {
        &self.high_array
    }

    /// Get low price array
    pub fn low(&self) -> &[f64] {
        &self.low_array
    }

    /// Get close price array
    pub fn close(&self) -> &[f64] {
        &self.close_array
    }

    /// Get volume array
    pub fn volume(&self) -> &[f64] {
        &self.volume_array
    }

    /// Get turnover array
    pub fn turnover(&self) -> &[f64] {
        &self.turnover_array
    }

    /// Get open interest array
    pub fn open_interest(&self) -> &[f64] {
        &self.open_interest_array
    }

    /// Create DataItem for ta-rs from index
    fn get_data_item(&self, i: usize) -> BarDataItem {
        BarDataItem {
            open: self.open_array[i],
            high: self.high_array[i],
            low: self.low_array[i],
            close: self.close_array[i],
            volume: self.volume_array[i],
        }
    }

    // ==================== Moving Averages ====================

    /// Simple Moving Average (SMA)
    pub fn sma(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = SimpleMovingAverage::new(n).unwrap();
        let mut result = 0.0;
        for i in (self.size - n)..self.size {
            result = indicator.next(self.close_array[i]);
        }
        result
    }

    /// Simple Moving Average - returns array
    pub fn sma_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = SimpleMovingAverage::new(n).unwrap();
        self.close_array.iter().map(|&v| indicator.next(v)).collect()
    }

    /// Exponential Moving Average (EMA)
    pub fn ema(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = ExponentialMovingAverage::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        result
    }

    /// Exponential Moving Average - returns array
    pub fn ema_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = ExponentialMovingAverage::new(n).unwrap();
        self.close_array.iter().map(|&v| indicator.next(v)).collect()
    }

    // ==================== Momentum Indicators ====================

    /// Relative Strength Index (RSI)
    pub fn rsi(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = RelativeStrengthIndex::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        result
    }

    /// RSI - returns array
    pub fn rsi_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = RelativeStrengthIndex::new(n).unwrap();
        self.close_array.iter().map(|&v| indicator.next(v)).collect()
    }

    /// Rate of Change (ROC)
    pub fn roc(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = RateOfChange::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        result
    }

    /// ROC - returns array
    pub fn roc_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = RateOfChange::new(n).unwrap();
        self.close_array.iter().map(|&v| indicator.next(v)).collect()
    }

    /// Momentum (MOM) - simple price change over n periods
    pub fn mom(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let last = self.size - 1;
        self.close_array[last] - self.close_array[last - n]
    }

    // ==================== Volatility Indicators ====================

    /// Standard Deviation (STDDEV)
    pub fn std(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = StandardDeviation::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        result
    }

    /// STDDEV - returns array
    pub fn std_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = StandardDeviation::new(n).unwrap();
        self.close_array.iter().map(|&v| indicator.next(v)).collect()
    }

    /// Average True Range (ATR)
    pub fn atr(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = AverageTrueRange::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// ATR - returns array
    pub fn atr_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = AverageTrueRange::new(n).unwrap();
        (0..self.size)
            .map(|i| indicator.next(&self.get_data_item(i)))
            .collect()
    }

    /// True Range (TRANGE)
    pub fn trange(&self) -> f64 {
        let mut indicator = TrueRange::new();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// TRANGE - returns array
    pub fn trange_array(&self) -> Vec<f64> {
        let mut indicator = TrueRange::new();
        (0..self.size)
            .map(|i| indicator.next(&self.get_data_item(i)))
            .collect()
    }

    /// Normalized ATR (NATR) = ATR / Close * 100
    pub fn natr(&self, n: usize) -> f64 {
        let atr_val = self.atr(n);
        let close = self.close_array[self.size - 1];
        if close == 0.0 {
            return 0.0;
        }
        (atr_val / close) * 100.0
    }

    // ==================== Trend Indicators ====================

    /// Moving Average Convergence Divergence (MACD)
    /// Returns (macd, signal, histogram)
    pub fn macd(&self, fast: usize, slow: usize, signal: usize) -> (f64, f64, f64) {
        if fast > self.size || slow > self.size || signal > self.size {
            return (0.0, 0.0, 0.0);
        }
        let mut indicator = MovingAverageConvergenceDivergence::new(fast, slow, signal).unwrap();
        let mut result = ta::indicators::MovingAverageConvergenceDivergenceOutput {
            macd: 0.0,
            signal: 0.0,
            histogram: 0.0,
        };
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        (result.macd, result.signal, result.histogram)
    }

    /// MACD - returns arrays (macd_array, signal_array, histogram_array)
    pub fn macd_array(&self, fast: usize, slow: usize, signal: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        if fast > self.size || slow > self.size || signal > self.size {
            return (vec![0.0; self.size], vec![0.0; self.size], vec![0.0; self.size]);
        }
        let mut indicator = MovingAverageConvergenceDivergence::new(fast, slow, signal).unwrap();
        let mut macd_arr = Vec::with_capacity(self.size);
        let mut signal_arr = Vec::with_capacity(self.size);
        let mut hist_arr = Vec::with_capacity(self.size);
        
        for i in 0..self.size {
            let result = indicator.next(self.close_array[i]);
            macd_arr.push(result.macd);
            signal_arr.push(result.signal);
            hist_arr.push(result.histogram);
        }
        (macd_arr, signal_arr, hist_arr)
    }

    /// Commodity Channel Index (CCI)
    pub fn cci(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = CommodityChannelIndex::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// CCI - returns array
    pub fn cci_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = CommodityChannelIndex::new(n).unwrap();
        (0..self.size)
            .map(|i| indicator.next(&self.get_data_item(i)))
            .collect()
    }

    // ==================== Channel Indicators ====================

    /// Bollinger Bands
    /// Returns (upper, middle, lower)
    pub fn boll(&self, n: usize, dev: f64) -> (f64, f64, f64) {
        if n > self.size || n == 0 {
            return (0.0, 0.0, 0.0);
        }
        let mut indicator = BollingerBands::new(n, dev).unwrap();
        let mut result = ta::indicators::BollingerBandsOutput {
            average: 0.0,
            upper: 0.0,
            lower: 0.0,
        };
        for i in 0..self.size {
            result = indicator.next(self.close_array[i]);
        }
        (result.upper, result.average, result.lower)
    }

    /// Bollinger Bands - returns arrays (upper, middle, lower)
    pub fn boll_array(&self, n: usize, dev: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        if n > self.size || n == 0 {
            return (vec![0.0; self.size], vec![0.0; self.size], vec![0.0; self.size]);
        }
        let mut indicator = BollingerBands::new(n, dev).unwrap();
        let mut upper = Vec::with_capacity(self.size);
        let mut middle = Vec::with_capacity(self.size);
        let mut lower = Vec::with_capacity(self.size);
        
        for i in 0..self.size {
            let result = indicator.next(self.close_array[i]);
            upper.push(result.upper);
            middle.push(result.average);
            lower.push(result.lower);
        }
        (upper, middle, lower)
    }

    /// Keltner Channel
    /// Returns (upper, middle, lower)
    pub fn keltner(&self, n: usize, multiplier: f64) -> (f64, f64, f64) {
        if n > self.size || n == 0 {
            return (0.0, 0.0, 0.0);
        }
        let mut indicator = KeltnerChannel::new(n, multiplier).unwrap();
        let mut result = ta::indicators::KeltnerChannelOutput {
            average: 0.0,
            upper: 0.0,
            lower: 0.0,
        };
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        (result.upper, result.average, result.lower)
    }

    /// Keltner Channel - returns arrays (upper, middle, lower)
    pub fn keltner_array(&self, n: usize, multiplier: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        if n > self.size || n == 0 {
            return (vec![0.0; self.size], vec![0.0; self.size], vec![0.0; self.size]);
        }
        let mut indicator = KeltnerChannel::new(n, multiplier).unwrap();
        let mut upper = Vec::with_capacity(self.size);
        let mut middle = Vec::with_capacity(self.size);
        let mut lower = Vec::with_capacity(self.size);
        
        for i in 0..self.size {
            let result = indicator.next(&self.get_data_item(i));
            upper.push(result.upper);
            middle.push(result.average);
            lower.push(result.lower);
        }
        (upper, middle, lower)
    }

    /// Donchian Channel
    /// Returns (upper, lower)
    pub fn donchian(&self, n: usize) -> (f64, f64) {
        if n > self.size || n == 0 {
            return (0.0, 0.0);
        }
        let mut max_indicator = Maximum::new(n).unwrap();
        let mut min_indicator = Minimum::new(n).unwrap();
        let mut upper = 0.0;
        let mut lower = 0.0;
        
        for i in 0..self.size {
            upper = max_indicator.next(self.high_array[i]);
            lower = min_indicator.next(self.low_array[i]);
        }
        (upper, lower)
    }

    /// Donchian Channel - returns arrays (upper, lower)
    pub fn donchian_array(&self, n: usize) -> (Vec<f64>, Vec<f64>) {
        if n > self.size || n == 0 {
            return (vec![0.0; self.size], vec![0.0; self.size]);
        }
        let mut max_indicator = Maximum::new(n).unwrap();
        let mut min_indicator = Minimum::new(n).unwrap();
        
        let upper: Vec<f64> = self.high_array.iter().map(|&v| max_indicator.next(v)).collect();
        let lower: Vec<f64> = self.low_array.iter().map(|&v| min_indicator.next(v)).collect();
        (upper, lower)
    }

    // ==================== Oscillators ====================

    /// Fast Stochastic Oscillator (%K)
    /// Returns %K value (Fast Stochastic)
    pub fn stoch_fast(&self, period: usize) -> f64 {
        if period > self.size || period == 0 {
            return 0.0;
        }
        let mut indicator = FastStochastic::new(period).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// Slow Stochastic Oscillator
    /// Returns %K value (smoothed by EMA)
    pub fn stoch_slow(&self, stochastic_period: usize, ema_period: usize) -> f64 {
        if stochastic_period > self.size || ema_period > self.size {
            return 0.0;
        }
        let mut indicator = SlowStochastic::new(stochastic_period, ema_period).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// Full Stochastic Oscillator
    /// Returns (%K, %D) where %D is SMA of %K
    pub fn stoch(&self, k_period: usize, d_period: usize) -> (f64, f64) {
        if k_period > self.size || d_period > self.size || k_period == 0 || d_period == 0 {
            return (0.0, 0.0);
        }
        
        // Calculate %K values using FastStochastic
        let mut fast_stoch = FastStochastic::new(k_period).unwrap();
        let k_values: Vec<f64> = (0..self.size)
            .map(|i| fast_stoch.next(&self.get_data_item(i)))
            .collect();
        
        // Calculate %D as SMA of %K
        let mut d_sma = SimpleMovingAverage::new(d_period).unwrap();
        let mut d_value = 0.0;
        for &k in &k_values {
            d_value = d_sma.next(k);
        }
        
        let k_value = *k_values.last().unwrap_or(&0.0);
        (k_value, d_value)
    }

    /// Williams %R
    pub fn willr(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let last = self.size - 1;
        let start = last - n + 1;
        
        let highest_high = self.high_array[start..=last]
            .iter()
            .cloned()
            .fold(f64::MIN, f64::max);
        let lowest_low = self.low_array[start..=last]
            .iter()
            .cloned()
            .fold(f64::MAX, f64::min);
        
        if highest_high == lowest_low {
            return 0.0;
        }
        
        -100.0 * (highest_high - self.close_array[last]) / (highest_high - lowest_low)
    }

    // ==================== Volume Indicators ====================

    /// On Balance Volume (OBV)
    pub fn obv(&self) -> f64 {
        let mut indicator = OnBalanceVolume::new();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// OBV - returns array
    pub fn obv_array(&self) -> Vec<f64> {
        let mut indicator = OnBalanceVolume::new();
        (0..self.size)
            .map(|i| indicator.next(&self.get_data_item(i)))
            .collect()
    }

    /// Money Flow Index (MFI)
    pub fn mfi(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = MoneyFlowIndex::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(&self.get_data_item(i));
        }
        result
    }

    /// MFI - returns array
    pub fn mfi_array(&self, n: usize) -> Vec<f64> {
        if n > self.size || n == 0 {
            return vec![0.0; self.size];
        }
        let mut indicator = MoneyFlowIndex::new(n).unwrap();
        (0..self.size)
            .map(|i| indicator.next(&self.get_data_item(i)))
            .collect()
    }

    // ==================== Price Extremes ====================

    /// Highest value over n periods
    pub fn highest(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = Maximum::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.high_array[i]);
        }
        result
    }

    /// Lowest value over n periods
    pub fn lowest(&self, n: usize) -> f64 {
        if n > self.size || n == 0 {
            return 0.0;
        }
        let mut indicator = Minimum::new(n).unwrap();
        let mut result = 0.0;
        for i in 0..self.size {
            result = indicator.next(self.low_array[i]);
        }
        result
    }

    // ==================== Directional Movement (手动实现) ====================

    /// Average Directional Index (ADX) - manual implementation
    pub fn adx(&self, n: usize) -> f64 {
        if n > self.size || n < 2 {
            return 0.0;
        }

        let mut plus_dm = Vec::with_capacity(self.size);
        let mut minus_dm = Vec::with_capacity(self.size);
        let mut tr = Vec::with_capacity(self.size);

        // Calculate +DM, -DM, TR
        for i in 1..self.size {
            let high_diff = self.high_array[i] - self.high_array[i - 1];
            let low_diff = self.low_array[i - 1] - self.low_array[i];
            
            let pdm = if high_diff > low_diff && high_diff > 0.0 { high_diff } else { 0.0 };
            let mdm = if low_diff > high_diff && low_diff > 0.0 { low_diff } else { 0.0 };
            
            plus_dm.push(pdm);
            minus_dm.push(mdm);
            
            let tr_val = (self.high_array[i] - self.low_array[i])
                .max((self.high_array[i] - self.close_array[i - 1]).abs())
                .max((self.low_array[i] - self.close_array[i - 1]).abs());
            tr.push(tr_val);
        }

        if plus_dm.len() < n {
            return 0.0;
        }

        // Smoothed averages
        let smoothed_plus_dm = Self::wilder_smooth(&plus_dm, n);
        let smoothed_minus_dm = Self::wilder_smooth(&minus_dm, n);
        let smoothed_tr = Self::wilder_smooth(&tr, n);

        // Calculate +DI, -DI
        let mut dx_values = Vec::new();
        for i in 0..smoothed_tr.len() {
            if smoothed_tr[i] != 0.0 {
                let plus_di = 100.0 * smoothed_plus_dm[i] / smoothed_tr[i];
                let minus_di = 100.0 * smoothed_minus_dm[i] / smoothed_tr[i];
                let di_sum = plus_di + minus_di;
                if di_sum != 0.0 {
                    let dx = 100.0 * (plus_di - minus_di).abs() / di_sum;
                    dx_values.push(dx);
                }
            }
        }

        if dx_values.is_empty() {
            return 0.0;
        }

        // ADX is smoothed DX
        let adx = Self::wilder_smooth(&dx_values, n);
        *adx.last().unwrap_or(&0.0)
    }

    /// Plus Directional Indicator (+DI)
    pub fn plus_di(&self, n: usize) -> f64 {
        if n > self.size || n < 2 {
            return 0.0;
        }

        let mut plus_dm = Vec::with_capacity(self.size);
        let mut tr = Vec::with_capacity(self.size);

        for i in 1..self.size {
            let high_diff = self.high_array[i] - self.high_array[i - 1];
            let low_diff = self.low_array[i - 1] - self.low_array[i];
            
            let pdm = if high_diff > low_diff && high_diff > 0.0 { high_diff } else { 0.0 };
            plus_dm.push(pdm);
            
            let tr_val = (self.high_array[i] - self.low_array[i])
                .max((self.high_array[i] - self.close_array[i - 1]).abs())
                .max((self.low_array[i] - self.close_array[i - 1]).abs());
            tr.push(tr_val);
        }

        let smoothed_plus_dm = Self::wilder_smooth(&plus_dm, n);
        let smoothed_tr = Self::wilder_smooth(&tr, n);

        if let (Some(&dm), Some(&tr_val)) = (smoothed_plus_dm.last(), smoothed_tr.last()) {
            if tr_val != 0.0 {
                return 100.0 * dm / tr_val;
            }
        }
        0.0
    }

    /// Minus Directional Indicator (-DI)
    pub fn minus_di(&self, n: usize) -> f64 {
        if n > self.size || n < 2 {
            return 0.0;
        }

        let mut minus_dm = Vec::with_capacity(self.size);
        let mut tr = Vec::with_capacity(self.size);

        for i in 1..self.size {
            let high_diff = self.high_array[i] - self.high_array[i - 1];
            let low_diff = self.low_array[i - 1] - self.low_array[i];
            
            let mdm = if low_diff > high_diff && low_diff > 0.0 { low_diff } else { 0.0 };
            minus_dm.push(mdm);
            
            let tr_val = (self.high_array[i] - self.low_array[i])
                .max((self.high_array[i] - self.close_array[i - 1]).abs())
                .max((self.low_array[i] - self.close_array[i - 1]).abs());
            tr.push(tr_val);
        }

        let smoothed_minus_dm = Self::wilder_smooth(&minus_dm, n);
        let smoothed_tr = Self::wilder_smooth(&tr, n);

        if let (Some(&dm), Some(&tr_val)) = (smoothed_minus_dm.last(), smoothed_tr.last()) {
            if tr_val != 0.0 {
                return 100.0 * dm / tr_val;
            }
        }
        0.0
    }

    /// Wilder's smoothing method (used for ADX/DI calculations)
    fn wilder_smooth(data: &[f64], n: usize) -> Vec<f64> {
        if data.len() < n {
            return vec![];
        }
        
        let mut result = Vec::with_capacity(data.len() - n + 1);
        
        // First value is SMA
        let first: f64 = data[..n].iter().sum::<f64>() / n as f64;
        result.push(first);
        
        // Subsequent values use Wilder's smoothing
        for i in n..data.len() {
            let prev = result.last().unwrap();
            let smoothed = prev - (prev / n as f64) + data[i];
            result.push(smoothed);
        }
        
        result
    }

    // ==================== SAR (Parabolic SAR) ====================

    /// Parabolic SAR
    pub fn sar(&self, acceleration: f64, maximum: f64) -> f64 {
        if self.size < 2 {
            return 0.0;
        }

        let mut is_long = true;
        let mut sar = self.low_array[0];
        let mut ep = self.high_array[0];
        let mut af = acceleration;

        for i in 1..self.size {
            let high = self.high_array[i];
            let low = self.low_array[i];

            // Update SAR
            sar = sar + af * (ep - sar);

            if is_long {
                // Limit SAR to prior two lows
                if i >= 2 {
                    sar = sar.min(self.low_array[i - 1]).min(self.low_array[i - 2]);
                } else if i >= 1 {
                    sar = sar.min(self.low_array[i - 1]);
                }

                // Check for reversal
                if low < sar {
                    is_long = false;
                    sar = ep;
                    ep = low;
                    af = acceleration;
                } else {
                    if high > ep {
                        ep = high;
                        af = (af + acceleration).min(maximum);
                    }
                }
            } else {
                // Limit SAR to prior two highs
                if i >= 2 {
                    sar = sar.max(self.high_array[i - 1]).max(self.high_array[i - 2]);
                } else if i >= 1 {
                    sar = sar.max(self.high_array[i - 1]);
                }

                // Check for reversal
                if high > sar {
                    is_long = true;
                    sar = ep;
                    ep = high;
                    af = acceleration;
                } else {
                    if low < ep {
                        ep = low;
                        af = (af + acceleration).min(maximum);
                    }
                }
            }
        }

        sar
    }

    // ==================== Aroon Indicator ====================

    /// Aroon Indicator
    /// Returns (aroon_up, aroon_down)
    pub fn aroon(&self, n: usize) -> (f64, f64) {
        if n > self.size || n == 0 {
            return (0.0, 0.0);
        }

        let last = self.size - 1;
        let start = last - n + 1;

        // Find periods since highest high and lowest low
        let mut highest_idx = start;
        let mut lowest_idx = start;
        let mut highest = self.high_array[start];
        let mut lowest = self.low_array[start];

        for i in start..=last {
            if self.high_array[i] >= highest {
                highest = self.high_array[i];
                highest_idx = i;
            }
            if self.low_array[i] <= lowest {
                lowest = self.low_array[i];
                lowest_idx = i;
            }
        }

        let periods_since_high = last - highest_idx;
        let periods_since_low = last - lowest_idx;

        let aroon_up = 100.0 * (n - periods_since_high) as f64 / n as f64;
        let aroon_down = 100.0 * (n - periods_since_low) as f64 / n as f64;

        (aroon_up, aroon_down)
    }

    /// Aroon Oscillator
    pub fn aroonosc(&self, n: usize) -> f64 {
        let (up, down) = self.aroon(n);
        up - down
    }

    // ==================== Ultimate Oscillator ====================

    /// Ultimate Oscillator
    pub fn ultosc(&self, period1: usize, period2: usize, period3: usize) -> f64 {
        if period3 > self.size || period3 < 2 {
            return 0.0;
        }

        let mut bp_sum1 = 0.0;
        let mut tr_sum1 = 0.0;
        let mut bp_sum2 = 0.0;
        let mut tr_sum2 = 0.0;
        let mut bp_sum3 = 0.0;
        let mut tr_sum3 = 0.0;

        for i in (self.size - period3)..self.size {
            if i == 0 {
                continue;
            }
            let low = self.low_array[i];
            let close = self.close_array[i];
            let prev_close = self.close_array[i - 1];
            let high = self.high_array[i];

            let true_low = low.min(prev_close);
            let bp = close - true_low;
            let tr = high.max(prev_close) - true_low;

            if i >= self.size - period1 {
                bp_sum1 += bp;
                tr_sum1 += tr;
            }
            if i >= self.size - period2 {
                bp_sum2 += bp;
                tr_sum2 += tr;
            }
            bp_sum3 += bp;
            tr_sum3 += tr;
        }

        let avg1 = if tr_sum1 != 0.0 { bp_sum1 / tr_sum1 } else { 0.0 };
        let avg2 = if tr_sum2 != 0.0 { bp_sum2 / tr_sum2 } else { 0.0 };
        let avg3 = if tr_sum3 != 0.0 { bp_sum3 / tr_sum3 } else { 0.0 };

        100.0 * (4.0 * avg1 + 2.0 * avg2 + avg3) / 7.0
    }

    // ==================== Balance of Power ====================

    /// Balance of Power (BOP)
    pub fn bop(&self) -> f64 {
        let last = self.size - 1;
        let high = self.high_array[last];
        let low = self.low_array[last];
        let open = self.open_array[last];
        let close = self.close_array[last];

        if high == low {
            return 0.0;
        }

        (close - open) / (high - low)
    }
}

impl Default for ArrayManager {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Mark a function as "virtual", which means it can be overridden
/// In Rust, this is typically handled through traits
#[macro_export]
macro_rules! virtual_fn {
    ($fn:item) => {
        $fn
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_vt_symbol() {
        let result = extract_vt_symbol("BTCUSDT.BINANCE");
        assert!(result.is_some());
        let (symbol, exchange) = result.unwrap();
        assert_eq!(symbol, "BTCUSDT");
        assert_eq!(exchange, Exchange::Binance);
    }

    #[test]
    fn test_generate_vt_symbol() {
        let vt_symbol = generate_vt_symbol("BTCUSDT", Exchange::Binance);
        assert_eq!(vt_symbol, "BTCUSDT.BINANCE");
    }

    #[test]
    fn test_round_to() {
        assert_eq!(round_to(1.234, 0.01), 1.23);
        assert_eq!(round_to(1.235, 0.01), 1.24);
    }

    #[test]
    fn test_floor_to() {
        assert_eq!(floor_to(1.239, 0.01), 1.23);
    }

    #[test]
    fn test_ceil_to() {
        assert_eq!(ceil_to(1.231, 0.01), 1.24);
    }

    #[test]
    fn test_get_digits() {
        assert_eq!(get_digits(1.234), 3);
        assert_eq!(get_digits(0.01), 2);
        assert_eq!(get_digits(0.001), 3);
        assert_eq!(get_digits(1e-8), 8);
    }

    #[test]
    fn test_array_manager() {
        let mut am = ArrayManager::new(10);
        assert!(!am.is_inited());
        
        // Update with bars
        for i in 0..10 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0 + i as f64,
                high_price: 101.0 + i as f64,
                low_price: 99.0 + i as f64,
                close_price: 100.5 + i as f64,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        assert!(am.is_inited());
    }

    #[test]
    fn test_array_manager_sma() {
        let mut am = ArrayManager::new(20);
        
        // Generate test data
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 105.0,
                low_price: 95.0,
                close_price: 100.0 + (i % 5) as f64, // 100, 101, 102, 103, 104, 100, ...
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let sma5 = am.sma(5);
        assert!(sma5 > 0.0);
    }

    #[test]
    fn test_array_manager_macd() {
        let mut am = ArrayManager::new(50);
        
        // Generate test data with trend
        for i in 0..50 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0 + i as f64 * 0.5,
                high_price: 105.0 + i as f64 * 0.5,
                low_price: 95.0 + i as f64 * 0.5,
                close_price: 100.0 + i as f64 * 0.5,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let (macd, signal, hist) = am.macd(12, 26, 9);
        // In an uptrend, MACD should be positive
        assert!(macd != 0.0 || signal != 0.0 || hist != 0.0);
    }

    #[test]
    fn test_array_manager_bollinger() {
        let mut am = ArrayManager::new(30);
        
        for i in 0..30 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 105.0,
                low_price: 95.0,
                close_price: 100.0 + (i % 10) as f64 - 5.0, // oscillating around 100
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let (upper, middle, lower) = am.boll(20, 2.0);
        assert!(upper > middle);
        assert!(middle > lower);
    }

    #[test]
    fn test_array_manager_rsi() {
        let mut am = ArrayManager::new(20);
        
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0 + i as f64,
                high_price: 105.0 + i as f64,
                low_price: 95.0 + i as f64,
                close_price: 100.0 + i as f64, // uptrend
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let rsi = am.rsi(14);
        // In uptrend, RSI should be high (but might not be exactly 100 due to smoothing)
        assert!(rsi > 0.0 && rsi <= 100.0);
    }

    #[test]
    fn test_array_manager_atr() {
        let mut am = ArrayManager::new(20);
        
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 110.0,
                low_price: 90.0,
                close_price: 100.0,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let atr = am.atr(14);
        assert!(atr > 0.0);
    }

    #[test]
    fn test_array_manager_donchian() {
        let mut am = ArrayManager::new(20);
        
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 100.0 + i as f64,
                low_price: 100.0 - i as f64,
                close_price: 100.0,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let (upper, lower) = am.donchian(10);
        assert!(upper > lower);
    }

    #[test]
    fn test_array_manager_stochastic() {
        let mut am = ArrayManager::new(20);
        
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 110.0,
                low_price: 90.0,
                close_price: 100.0 + (i % 10) as f64,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let k = am.stoch_fast(14);
        assert!(k >= 0.0 && k <= 100.0);
        
        let (k2, d) = am.stoch(14, 3);
        assert!(k2 >= 0.0 && k2 <= 100.0);
        assert!(d >= 0.0 && d <= 100.0);
    }

    #[test]
    fn test_array_manager_obv() {
        let mut am = ArrayManager::new(20);
        
        for i in 0..20 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 1000.0 + (i * 100) as f64,
                turnover: 100000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 105.0,
                low_price: 95.0,
                close_price: 100.0 + i as f64,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let obv = am.obv();
        assert!(obv != 0.0);
    }

    #[test]
    fn test_array_manager_aroon() {
        let mut am = ArrayManager::new(30);
        
        for i in 0..30 {
            let bar = BarData {
                gateway_name: "test".to_string(),
                symbol: "TEST".to_string(),
                exchange: Exchange::Local,
                datetime: chrono::Utc::now(),
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 1000.0,
                open_interest: 0.0,
                open_price: 100.0,
                high_price: 100.0 + i as f64,
                low_price: 100.0 - (i / 2) as f64,
                close_price: 100.0,
                extra: None,
            };
            am.update_bar(&bar);
        }
        
        let (aroon_up, aroon_down) = am.aroon(14);
        assert!(aroon_up >= 0.0 && aroon_up <= 100.0);
        assert!(aroon_down >= 0.0 && aroon_down <= 100.0);
    }
}
