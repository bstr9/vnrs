//! Test fixtures - Factory functions for creating test data instances

use chrono::{DateTime, Utc};
use trade_engine::trader::{
    BarData, Direction, Exchange, Interval, OrderData, OrderRequest, OrderType, TickData,
};
use trade_engine::trader::{Offset, Status};

/// Create a test TickData with sensible defaults
///
/// # Arguments
/// * `symbol` - Trading symbol (e.g., "BTCUSDT")
/// * `exchange` - Exchange enum
/// * `last_price` - Last traded price
#[allow(clippy::unwrap_used)]
pub fn make_test_tick(symbol: &str, exchange: Exchange, last_price: f64) -> TickData {
    make_test_tick_at_time(symbol, exchange, last_price, Utc::now())
}

/// Create a test TickData with a specific datetime
#[allow(clippy::unwrap_used)]
pub fn make_test_tick_at_time(
    symbol: &str,
    exchange: Exchange,
    last_price: f64,
    dt: DateTime<Utc>,
) -> TickData {
    let mut tick = TickData::new("TEST_GATEWAY".to_string(), symbol.to_string(), exchange, dt);
    tick.last_price = last_price;
    tick.volume = 1000.0;
    tick.open_interest = 500.0;

    // Initialize bid/ask prices with a spread around last_price
    let spread = last_price * 0.001; // 0.1% spread
    tick.bid_price_1 = last_price - spread;
    tick.bid_price_2 = last_price - spread * 2.0;
    tick.bid_price_3 = last_price - spread * 3.0;
    tick.bid_price_4 = last_price - spread * 4.0;
    tick.bid_price_5 = last_price - spread * 5.0;

    tick.ask_price_1 = last_price + spread;
    tick.ask_price_2 = last_price + spread * 2.0;
    tick.ask_price_3 = last_price + spread * 3.0;
    tick.ask_price_4 = last_price + spread * 4.0;
    tick.ask_price_5 = last_price + spread * 5.0;

    // Initialize bid/ask volumes
    tick.bid_volume_1 = 10.0;
    tick.bid_volume_2 = 15.0;
    tick.bid_volume_3 = 20.0;
    tick.bid_volume_4 = 25.0;
    tick.bid_volume_5 = 30.0;

    tick.ask_volume_1 = 10.0;
    tick.ask_volume_2 = 15.0;
    tick.ask_volume_3 = 20.0;
    tick.ask_volume_4 = 25.0;
    tick.ask_volume_5 = 30.0;

    tick
}

/// Create a test BarData with OHLCV values
#[allow(clippy::unwrap_used)]
pub fn make_test_bar(
    symbol: &str,
    exchange: Exchange,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> BarData {
    make_test_bar_at_time(symbol, exchange, open, high, low, close, volume, Utc::now())
}

/// Create a test BarData with a specific datetime
#[allow(clippy::unwrap_used)]
pub fn make_test_bar_at_time(
    symbol: &str,
    exchange: Exchange,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    dt: DateTime<Utc>,
) -> BarData {
    let mut bar = BarData::new("TEST_GATEWAY".to_string(), symbol.to_string(), exchange, dt);
    bar.interval = Some(Interval::Minute);
    bar.open_price = open;
    bar.high_price = high;
    bar.low_price = low;
    bar.close_price = close;
    bar.volume = volume;
    bar
}

/// Create a test OrderData
#[allow(clippy::unwrap_used)]
pub fn make_test_order(
    symbol: &str,
    exchange: Exchange,
    direction: Direction,
    price: f64,
    volume: f64,
) -> OrderData {
    let orderid = format!("ORDER_{}", Utc::now().timestamp_millis());
    let mut order = OrderData::new(
        "TEST_GATEWAY".to_string(),
        symbol.to_string(),
        exchange,
        orderid,
    );
    order.direction = Some(direction);
    order.order_type = OrderType::Limit;
    order.price = price;
    order.volume = volume;
    order.status = Status::NotTraded;
    order.offset = Offset::Open;
    order.datetime = Some(Utc::now());
    order
}

/// Create a test OrderRequest
pub fn make_test_order_request(
    symbol: &str,
    exchange: Exchange,
    direction: Direction,
    price: f64,
    volume: f64,
) -> OrderRequest {
    let mut req = OrderRequest::new(
        symbol.to_string(),
        exchange,
        direction,
        OrderType::Limit,
        volume,
    );
    req.price = price;
    req.offset = Offset::Open;
    req
}

/// Generate a series of ascending bars for backtesting
///
/// Creates `count` bars starting from `start_price`, with each bar's close price
/// being `start_price + i * step` where step is calculated to end at roughly
/// `start_price * 1.1` for the final bar.
pub fn make_ascending_bars(
    symbol: &str,
    exchange: Exchange,
    start_price: f64,
    count: usize,
    interval_seconds: i64,
) -> Vec<BarData> {
    let mut bars = Vec::with_capacity(count);
    let step = if count > 1 {
        start_price * 0.1 / (count as f64 - 1.0)
    } else {
        0.0
    };

    let base_time = Utc::now() - chrono::Duration::seconds(count as i64 * interval_seconds);

    for i in 0..count {
        let close = start_price + step * i as f64;
        let open = close - step * 0.3;
        let high = close.max(open) + step * 0.2;
        let low = open.min(close) - step * 0.1;

        let dt = base_time + chrono::Duration::seconds((i as i64 + 1) * interval_seconds);
        let bar = make_test_bar_at_time(symbol, exchange, open, high, low, close, 100.0, dt);
        bars.push(bar);
    }

    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_test_tick() {
        let tick = make_test_tick("BTCUSDT", Exchange::Binance, 50000.0);
        assert_eq!(tick.symbol, "BTCUSDT");
        assert_eq!(tick.exchange, Exchange::Binance);
        assert_eq!(tick.last_price, 50000.0);
        assert!(tick.bid_price_1 > 0.0);
        assert!(tick.ask_price_1 > 0.0);
        assert!(tick.bid_volume_1 > 0.0);
        assert!(tick.ask_volume_1 > 0.0);
    }

    #[test]
    fn test_make_test_bar() {
        let bar = make_test_bar("BTCUSDT", Exchange::Binance, 50000.0, 50500.0, 49800.0, 50200.0, 100.0);
        assert_eq!(bar.symbol, "BTCUSDT");
        assert_eq!(bar.open_price, 50000.0);
        assert_eq!(bar.high_price, 50500.0);
        assert_eq!(bar.low_price, 49800.0);
        assert_eq!(bar.close_price, 50200.0);
        assert_eq!(bar.volume, 100.0);
    }

    #[test]
    fn test_make_test_order() {
        let order = make_test_order("BTCUSDT", Exchange::Binance, Direction::Long, 50000.0, 1.0);
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.direction, Some(Direction::Long));
        assert_eq!(order.price, 50000.0);
        assert_eq!(order.volume, 1.0);
        assert_eq!(order.status, Status::NotTraded);
    }

    #[test]
    fn test_make_ascending_bars() {
        let bars = make_ascending_bars("BTCUSDT", Exchange::Binance, 50000.0, 10, 60);
        assert_eq!(bars.len(), 10);
        
        // Verify prices are ascending
        for i in 1..bars.len() {
            assert!(bars[i].close_price > bars[i - 1].close_price);
        }
    }
}
