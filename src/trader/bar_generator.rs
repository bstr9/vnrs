//! Bar generator for converting tick data to bar data.
//!
//! This module provides functionality to aggregate tick data into bars
//! of different intervals (1 second, 1 minute, 15 minutes, etc.)

use crate::trader::constant::{Exchange, Interval};
use crate::trader::object::{BarData, TickData};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use std::collections::HashMap;

/// Bar generator that aggregates ticks into bars
pub struct BarGenerator {
    /// Current interval for bar generation
    interval: Interval,
    /// Current bar being built for each symbol
    current_bars: HashMap<String, BarBuilder>,
    /// Callback for completed bars
    on_bar: Option<Box<dyn Fn(BarData) + Send + Sync>>,
}

/// Builder for accumulating tick data into a bar
struct BarBuilder {
    symbol: String,
    exchange: Exchange,
    interval: Interval,
    gateway_name: String,

    /// Bar start time
    start_time: DateTime<Utc>,
    /// Open price (first tick in period)
    open_price: f64,
    /// High price (max in period)
    high_price: f64,
    /// Low price (min in period)
    low_price: f64,
    /// Close price (last tick in period)
    close_price: f64,
    /// Accumulated volume
    volume: f64,
    /// Accumulated turnover
    turnover: f64,
    /// Last open interest
    open_interest: f64,

    /// Number of ticks in this bar
    tick_count: usize,
}

impl BarBuilder {
    /// Create a new bar builder from the first tick
    fn new(tick: &TickData, interval: Interval, start_time: DateTime<Utc>) -> Self {
        Self {
            symbol: tick.symbol.clone(),
            exchange: tick.exchange,
            interval,
            gateway_name: tick.gateway_name.clone(),
            start_time,
            open_price: tick.last_price,
            high_price: tick.last_price,
            low_price: tick.last_price,
            close_price: tick.last_price,
            volume: tick.volume,
            turnover: tick.turnover,
            open_interest: tick.open_interest,
            tick_count: 1,
        }
    }

    /// Update the bar with a new tick
    fn update(&mut self, tick: &TickData) {
        self.high_price = self.high_price.max(tick.last_price);
        self.low_price = self.low_price.min(tick.last_price);
        self.close_price = tick.last_price;
        self.volume = tick.volume;
        self.turnover = tick.turnover;
        self.open_interest = tick.open_interest;
        self.tick_count += 1;
    }

    /// Build the final bar data
    fn build(self) -> BarData {
        BarData {
            symbol: self.symbol,
            exchange: self.exchange,
            datetime: self.start_time,
            interval: Some(self.interval),
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            close_price: self.close_price,
            volume: self.volume,
            turnover: self.turnover,
            open_interest: self.open_interest,
            gateway_name: self.gateway_name,
            extra: None,
        }
    }
}

impl BarGenerator {
    /// Create a new bar generator
    pub fn new(interval: Interval) -> Self {
        Self {
            interval,
            current_bars: HashMap::new(),
            on_bar: None,
        }
    }

    /// Set callback for when a bar is completed
    pub fn set_callback<F>(&mut self, callback: F)
    where
        F: Fn(BarData) + Send + Sync + 'static,
    {
        self.on_bar = Some(Box::new(callback));
    }

    /// Update with a new tick
    pub fn update_tick(&mut self, tick: &TickData) -> Option<BarData> {
        let vt_symbol = tick.vt_symbol();
        let bar_start_time = self.get_bar_start_time(&tick.datetime);

        let mut completed_bar = None;

        // Check if we have an existing bar for this symbol
        if let Some(builder) = self.current_bars.get(&vt_symbol) {
            // Check if the tick belongs to the current bar period
            let next_time = self.get_next_bar_time(&builder.start_time);
            let start_time = builder.start_time;

            if tick.datetime >= start_time && tick.datetime < next_time {
                // Same period - update the bar
                self.current_bars.get_mut(&vt_symbol).map(|b| b.update(tick));
            } else if tick.datetime < start_time {
                // Out-of-order tick, skip it
                return None;
            } else {
                // New period - complete the old bar and start a new one
                let old_builder = self
                    .current_bars
                    .remove(&vt_symbol)
                    .expect("builder must exist for the current vt_symbol");
                let finished_bar = old_builder.build();

                // Notify callback if set
                if let Some(ref callback) = self.on_bar {
                    callback(finished_bar.clone());
                }

                completed_bar = Some(finished_bar);

                // Start new bar
                let new_builder = BarBuilder::new(tick, self.interval, bar_start_time);
                self.current_bars.insert(vt_symbol, new_builder);
            }
        } else {
            // First tick for this symbol - create new bar
            let builder = BarBuilder::new(tick, self.interval, bar_start_time);
            self.current_bars.insert(vt_symbol, builder);
        }

        completed_bar
    }

    /// Get the start time for the bar period containing the given datetime
    fn get_bar_start_time(&self, dt: &DateTime<Utc>) -> DateTime<Utc> {
        match self.interval {
            Interval::Second => {
                // Round down to the nearest second
                dt.with_nanosecond(0).unwrap_or(*dt)
            }
            Interval::Minute => {
                // Round down to the nearest minute
                dt.with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Minute5 => {
                // Round down to the nearest 5-minute mark
                let minute = dt.minute();
                let rounded_minute = (minute / 5) * 5;
                dt.with_minute(rounded_minute)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Minute15 => {
                // Round down to the nearest 15-minute mark
                let minute = dt.minute();
                let rounded_minute = (minute / 15) * 15;
                dt.with_minute(rounded_minute)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Minute30 => {
                // Round down to the nearest 30-minute mark
                let minute = dt.minute();
                let rounded_minute = (minute / 30) * 30;
                dt.with_minute(rounded_minute)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Hour => {
                // Round down to the nearest hour
                dt.with_minute(0)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Hour4 => {
                // Round down to the nearest 4-hour mark
                let hour = dt.hour();
                let rounded_hour = (hour / 4) * 4;
                dt.with_hour(rounded_hour)
                    .unwrap_or(*dt)
                    .with_minute(0)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Daily => {
                // Round down to the start of the day
                dt.with_hour(0)
                    .unwrap_or(*dt)
                    .with_minute(0)
                    .unwrap_or(*dt)
                    .with_second(0)
                    .unwrap_or(*dt)
                    .with_nanosecond(0)
                    .unwrap_or(*dt)
            }
            Interval::Weekly => {
                // Round down to the start of the week (Monday)
                let days_from_monday = dt.weekday().num_days_from_monday();
                let week_start = *dt - Duration::days(days_from_monday as i64);
                week_start
                    .with_hour(0)
                    .unwrap_or(week_start)
                    .with_minute(0)
                    .unwrap_or(week_start)
                    .with_second(0)
                    .unwrap_or(week_start)
                    .with_nanosecond(0)
                    .unwrap_or(week_start)
            }
            Interval::Tick => *dt, // No aggregation for tick
        }
    }

    /// Get the start time of the next bar period
    fn get_next_bar_time(&self, start_time: &DateTime<Utc>) -> DateTime<Utc> {
        match self.interval {
            Interval::Second => *start_time + Duration::seconds(1),
            Interval::Minute => *start_time + Duration::minutes(1),
            Interval::Minute5 => *start_time + Duration::minutes(5),
            Interval::Minute15 => *start_time + Duration::minutes(15),
            Interval::Minute30 => *start_time + Duration::minutes(30),
            Interval::Hour => *start_time + Duration::hours(1),
            Interval::Hour4 => *start_time + Duration::hours(4),
            Interval::Daily => *start_time + Duration::days(1),
            Interval::Weekly => *start_time + Duration::weeks(1),
            Interval::Tick => *start_time + Duration::nanoseconds(1),
        }
    }

    /// Force complete all current bars (useful when stopping)
    pub fn flush_all(&mut self) -> Vec<BarData> {
        let mut bars = Vec::new();

        for (_, builder) in self.current_bars.drain() {
            let bar = builder.build();

            if let Some(ref callback) = self.on_bar {
                callback(bar.clone());
            }

            bars.push(bar);
        }

        bars
    }

    /// Get the current bar for a symbol (without completing it)
    pub fn get_current_bar(&self, vt_symbol: &str) -> Option<BarData> {
        self.current_bars.get(vt_symbol).map(|builder| BarData {
            symbol: builder.symbol.clone(),
            exchange: builder.exchange,
            datetime: builder.start_time,
            interval: Some(builder.interval),
            open_price: builder.open_price,
            high_price: builder.high_price,
            low_price: builder.low_price,
            close_price: builder.close_price,
            volume: builder.volume,
            turnover: builder.turnover,
            open_interest: builder.open_interest,
            gateway_name: builder.gateway_name.clone(),
            extra: None,
        })
    }

    /// Change the interval (will flush all current bars)
    pub fn set_interval(&mut self, interval: Interval) -> Vec<BarData> {
        let bars = self.flush_all();
        self.interval = interval;
        bars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tick(symbol: &str, price: f64, datetime: DateTime<Utc>) -> TickData {
        TickData {
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            datetime,
            name: symbol.to_string(),
            volume: 100.0,
            turnover: price * 100.0,
            open_interest: 0.0,
            last_price: price,
            last_volume: 10.0,
            limit_up: 0.0,
            limit_down: 0.0,
            open_price: price,
            high_price: price,
            low_price: price,
            pre_close: price - 1.0,
            bid_price_1: price - 0.01,
            bid_volume_1: 50.0,
            ask_price_1: price + 0.01,
            ask_volume_1: 50.0,
            gateway_name: "test".to_string(),
            extra: None,
            bid_price_2: None,
            bid_price_3: None,
            bid_price_4: None,
            bid_price_5: None,
            ask_price_2: None,
            ask_price_3: None,
            ask_price_4: None,
            ask_price_5: None,
            bid_volume_2: None,
            bid_volume_3: None,
            bid_volume_4: None,
            bid_volume_5: None,
            ask_volume_2: None,
            ask_volume_3: None,
            ask_volume_4: None,
            ask_volume_5: None,
        }
    }

    #[test]
    fn test_bar_generator_minute() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .expect("second 0 is always valid")
            .with_nanosecond(0)
            .expect("nanosecond 0 is always valid");

        // First tick - creates new bar
        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        assert!(gen.update_tick(&tick1).is_none());

        // Second tick - same minute, updates bar
        let tick2 = create_test_tick("BTCUSDT", 50100.0, base_time + Duration::seconds(30));
        assert!(gen.update_tick(&tick2).is_none());

        // Third tick - next minute, completes previous bar
        let tick3 = create_test_tick("BTCUSDT", 50200.0, base_time + Duration::minutes(1));
        let completed = gen.update_tick(&tick3);
        assert!(completed.is_some());

        let bar = completed.expect("completed bar should exist after period transition");
        assert_eq!(bar.open_price, 50000.0);
        assert_eq!(bar.close_price, 50100.0);
        assert_eq!(bar.high_price, 50100.0);
        assert_eq!(bar.low_price, 50000.0);
    }

    #[test]
    fn test_bar_generator_daily() {
        let mut gen = BarGenerator::new(Interval::Daily);

        let day1 = Utc::now()
            .with_hour(10)
            .unwrap_or(Utc::now())
            .with_minute(0)
            .unwrap_or(Utc::now())
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("ETHUSDT", 3000.0, day1);
        assert!(gen.update_tick(&tick1).is_none());

        let tick2 = create_test_tick("ETHUSDT", 3050.0, day1 + Duration::hours(6));
        assert!(gen.update_tick(&tick2).is_none());

        let day2 = day1 + Duration::days(1);
        let tick3 = create_test_tick("ETHUSDT", 3100.0, day2);
        let completed = gen.update_tick(&tick3);
        assert!(completed.is_some());

        let bar = completed.unwrap();
        assert_eq!(bar.open_price, 3000.0);
        assert_eq!(bar.close_price, 3050.0);
        assert_eq!(bar.high_price, 3050.0);
        assert_eq!(bar.low_price, 3000.0);
    }

    #[test]
    fn test_bar_generator_out_of_order_tick() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        assert!(gen.update_tick(&tick1).is_none());

        let later_tick = create_test_tick("BTCUSDT", 50200.0, base_time + Duration::minutes(1));
        assert!(gen.update_tick(&later_tick).is_some());

        let out_of_order_tick =
            create_test_tick("BTCUSDT", 50100.0, base_time + Duration::seconds(30));
        let result = gen.update_tick(&out_of_order_tick);
        assert!(result.is_none());
    }

    #[test]
    fn test_bar_generator_flush_all() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        gen.update_tick(&tick1);

        let bars = gen.flush_all();
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].open_price, 50000.0);
        assert_eq!(bars[0].close_price, 50000.0);
    }

    #[test]
    fn test_bar_generator_get_current_bar() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        assert!(gen.get_current_bar("BTCUSDT.BINANCE").is_none());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        gen.update_tick(&tick1);

        let current = gen.get_current_bar("BTCUSDT.BINANCE");
        assert!(current.is_some());
        let bar = current.unwrap();
        assert_eq!(bar.open_price, 50000.0);
        assert_eq!(bar.close_price, 50000.0);
    }

    #[test]
    fn test_bar_generator_multiple_symbols() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick_btc = create_test_tick("BTCUSDT", 50000.0, base_time);
        let tick_eth = create_test_tick("ETHUSDT", 3000.0, base_time);

        assert!(gen.update_tick(&tick_btc).is_none());
        assert!(gen.update_tick(&tick_eth).is_none());

        assert!(gen.get_current_bar("BTCUSDT.BINANCE").is_some());
        assert!(gen.get_current_bar("ETHUSDT.BINANCE").is_some());
    }

    #[test]
    fn test_bar_generator_set_interval() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        gen.update_tick(&tick1);

        let bars = gen.set_interval(Interval::Hour);
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn test_bar_generator_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let mut gen = BarGenerator::new(Interval::Minute);
        gen.set_callback(move |_bar| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        gen.update_tick(&tick1);

        let tick2 = create_test_tick("BTCUSDT", 50100.0, base_time + Duration::minutes(1));
        gen.update_tick(&tick2);

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_bar_generator_high_low_tracking() {
        let mut gen = BarGenerator::new(Interval::Minute);

        let base_time = Utc::now()
            .with_second(0)
            .unwrap_or(Utc::now())
            .with_nanosecond(0)
            .unwrap_or(Utc::now());

        let tick1 = create_test_tick("BTCUSDT", 50000.0, base_time);
        gen.update_tick(&tick1);

        let tick2 = create_test_tick("BTCUSDT", 50500.0, base_time + Duration::seconds(10));
        gen.update_tick(&tick2);

        let tick3 = create_test_tick("BTCUSDT", 49800.0, base_time + Duration::seconds(20));
        gen.update_tick(&tick3);

        let tick4 = create_test_tick("BTCUSDT", 50200.0, base_time + Duration::minutes(1));
        let completed = gen.update_tick(&tick4);
        assert!(completed.is_some());

        let bar = completed.unwrap();
        assert_eq!(bar.high_price, 50500.0);
        assert_eq!(bar.low_price, 49800.0);
        assert_eq!(bar.open_price, 50000.0);
        assert_eq!(bar.close_price, 50200.0);
    }

    #[test]
    fn test_bar_generator_flush_all_empty() {
        let gen = BarGenerator::new(Interval::Minute);
        let bars = gen.flush_all();
        assert!(bars.is_empty());
    }

    #[test]
    fn test_bar_generator_no_current_bar_for_unknown_symbol() {
        let gen = BarGenerator::new(Interval::Minute);
        assert!(gen.get_current_bar("UNKNOWN.BINANCE").is_none());
    }
}
