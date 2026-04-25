//! Redis database implementation for distributed data persistence.
//!
//! Provides a Redis-backed database using Sorted Sets for time-series data
//! (bars, ticks, depths) and Hashes for key-value data (orders, trades, positions).
//! All data is serialized with serde_json for cross-backend compatibility.
//!
//! ## Key Design
//! - Bars: Sorted Set `vnrs:bars:{exchange}:{symbol}:{interval}` (score = timestamp)
//! - Ticks: Sorted Set `vnrs:ticks:{exchange}:{symbol}` (score = timestamp)
//! - Depths: Sorted Set `vnrs:depths:{exchange}:{symbol}` (score = timestamp)
//! - Orders: Hash `vnrs:orders` (field = vt_orderid)
//! - Trades: Hash `vnrs:trades` (field = vt_tradeid)
//! - Positions: Hash `vnrs:positions` (field = vt_positionid)
//! - Events: Sorted Set `vnrs:events` (score = event_id)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use std::collections::HashMap;

use crate::error::DatabaseError;
use super::constant::{Exchange, Interval};
use super::database::{BaseDatabase, BarOverview, EventRecord, TickOverview};
use super::object::{BarData, DepthData, OrderData, PositionData, TickData, TradeData};

/// Redis database implementation.
///
/// Uses `redis::aio::MultiplexedConnection` for async, thread-safe access.
/// The connection can be cloned for concurrent use across tasks.
pub struct RedisDatabase {
    conn: redis::aio::MultiplexedConnection,
    url: String,
}

impl RedisDatabase {
    /// Create a new RedisDatabase by connecting to the given URL.
    ///
    /// The URL should be in the format `redis://host:port/db`.
    pub fn new(url: &str) -> Result<Self, DatabaseError> {
        let client = redis::Client::open(url)
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create Redis client: {}", e)))?;
        Self::new_with_client(client)
    }

    /// Create a new RedisDatabase using an existing `redis::Client`.
    pub fn new_with_client(client: redis::Client) -> Result<Self, DatabaseError> {
        let url = client.get_connection_info().addr.to_string();
        let runtime = tokio::runtime::Handle::current();
        let conn = runtime.block_on(client.get_multiplexed_async_connection())
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to connect to Redis at {}: {}", url, e)))?;
        Ok(Self { conn, url })
    }

    /// Create a new RedisDatabase asynchronously.
    pub async fn connect(url: &str) -> Result<Self, DatabaseError> {
        let client = redis::Client::open(url)
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create Redis client: {}", e)))?;
        let conn = client.get_multiplexed_async_connection().await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to connect to Redis: {}", e)))?;
        Ok(Self { conn, url: url.to_string() })
    }

    /// Get the Redis URL this database is connected to.
    pub fn url(&self) -> &str {
        &self.url
    }

    // ---- Key formatting helpers ----

    /// Format bar sorted set key: `vnrs:bars:{exchange}:{symbol}:{interval}`
    fn bar_key(exchange: &Exchange, symbol: &str, interval: &Interval) -> String {
        format!("vnrs:bars:{}:{}:{}", exchange.value(), symbol, interval.value())
    }

    /// Format tick sorted set key: `vnrs:ticks:{exchange}:{symbol}`
    fn tick_key(exchange: &Exchange, symbol: &str) -> String {
        format!("vnrs:ticks:{}:{}", exchange.value(), symbol)
    }

    /// Format depth sorted set key: `vnrs:depths:{exchange}:{symbol}`
    fn depth_key(exchange: &Exchange, symbol: &str) -> String {
        format!("vnrs:depths:{}:{}", exchange.value(), symbol)
    }

    /// Orders hash key
    const ORDERS_KEY: &'static str = "vnrs:orders";

    /// Trades hash key
    const TRADES_KEY: &'static str = "vnrs:trades";

    /// Positions hash key
    const POSITIONS_KEY: &'static str = "vnrs:positions";

    /// Events sorted set key
    const EVENTS_KEY: &'static str = "vnrs:events";
}

/// Parse an Exchange from its value string (e.g., "BINANCE" -> Exchange::Binance).
fn exchange_from_value(value: &str) -> Option<Exchange> {
    use Exchange::*;
    Some(match value {
        "CFFEX" => Cffex,
        "SHFE" => Shfe,
        "CZCE" => Czce,
        "DCE" => Dce,
        "INE" => Ine,
        "GFEX" => Gfex,
        "SSE" => Sse,
        "SZSE" => Szse,
        "BSE" => Bse,
        "SHHK" => Shhk,
        "SZHK" => Szhk,
        "SGE" => Sge,
        "WXE" => Wxe,
        "CFETS" => Cfets,
        "XBOND" => Xbond,
        "SMART" => Smart,
        "NYSE" => Nyse,
        "NASDAQ" => Nasdaq,
        "ARCA" => Arca,
        "EDGEA" => Edgea,
        "ISLAND" => Island,
        "BATS" => Bats,
        "IEX" => Iex,
        "AMEX" => Amex,
        "TSE" => Tse,
        "NYMEX" => Nymex,
        "COMEX" => Comex,
        "GLOBEX" => Globex,
        "IDEALPRO" => Idealpro,
        "CME" => Cme,
        "ICE" => Ice,
        "SEHK" => Sehk,
        "HKFE" => Hkfe,
        "SGX" => Sgx,
        "CBOT" => Cbot,
        "CBOE" => Cboe,
        "CFE" => Cfe,
        "DME" => Dme,
        "EUX" => Eurex,
        "APEX" => Apex,
        "LME" => Lme,
        "BMD" => Bmd,
        "TOCOM" => Tocom,
        "EUNX" => Eunx,
        "KRX" => Krx,
        "OTC" => Otc,
        "IBKRATS" => Ibkrats,
        "BINANCE" => Binance,
        "BINANCE_USDM" => BinanceUsdm,
        "BINANCE_COINM" => BinanceCoinm,
        "OKX" => Okx,
        "BYBIT" => Bybit,
        "LOCAL" => Local,
        "GLOBAL" => Global,
        _ => return None,
    })
}

/// Parse an Interval from its value string (e.g., "1m" -> Interval::Minute).
fn interval_from_value(value: &str) -> Option<Interval> {
    use Interval::*;
    Some(match value {
        "1s" => Second,
        "1m" => Minute,
        "5m" => Minute5,
        "15m" => Minute15,
        "30m" => Minute30,
        "1h" => Hour,
        "4h" => Hour4,
        "d" => Daily,
        "w" => Weekly,
        "tick" => Tick,
        _ => return None,
    })
}

#[async_trait]
impl BaseDatabase for RedisDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, DatabaseError> {
        if bars.is_empty() {
            return Ok(true);
        }

        let mut pipe = redis::pipe();
        for bar in &bars {
            let interval = bar.interval.unwrap_or(Interval::Minute);
            let key = Self::bar_key(&bar.exchange, &bar.symbol, &interval);
            let score = bar.datetime.timestamp() as f64;
            let member = serde_json::to_string(bar)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize BarData: {}", e)))?;
            pipe.zadd(key, member, score).ignore();
        }

        pipe.query_async::<()>(&mut self.conn.clone()).await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "bars".to_string(),
                reason: format!("Failed to save bar data: {}", e),
            })?;

        Ok(true)
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, DatabaseError> {
        if ticks.is_empty() {
            return Ok(true);
        }

        let mut pipe = redis::pipe();
        for tick in &ticks {
            let key = Self::tick_key(&tick.exchange, &tick.symbol);
            let score = tick.datetime.timestamp() as f64;
            let member = serde_json::to_string(tick)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize TickData: {}", e)))?;
            pipe.zadd(key, member, score).ignore();
        }

        pipe.query_async::<()>(&mut self.conn.clone()).await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "ticks".to_string(),
                reason: format!("Failed to save tick data: {}", e),
            })?;

        Ok(true)
    }

    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, DatabaseError> {
        let key = Self::bar_key(&exchange, symbol, &interval);
        let min_score = start.timestamp() as f64;
        let max_score = end.timestamp() as f64;

        let members: Vec<String> = self.conn.clone()
            .zrangebyscore(key, min_score, max_score)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load bar data: {}", e)))?;

        let mut result = Vec::with_capacity(members.len());
        for member in members {
            let bar: BarData = serde_json::from_str(&member)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize BarData: {}", e)))?;
            result.push(bar);
        }

        Ok(result)
    }

    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, DatabaseError> {
        let key = Self::tick_key(&exchange, symbol);
        let min_score = start.timestamp() as f64;
        let max_score = end.timestamp() as f64;

        let members: Vec<String> = self.conn.clone()
            .zrangebyscore(key, min_score, max_score)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load tick data: {}", e)))?;

        let mut result = Vec::with_capacity(members.len());
        for member in members {
            let tick: TickData = serde_json::from_str(&member)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize TickData: {}", e)))?;
            result.push(tick);
        }

        Ok(result)
    }

    async fn load_depth_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<DepthData>, DatabaseError> {
        let key = Self::depth_key(&exchange, symbol);
        let min_score = start.timestamp() as f64;
        let max_score = end.timestamp() as f64;

        let members: Vec<String> = self.conn.clone()
            .zrangebyscore(key, min_score, max_score)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load depth data: {}", e)))?;

        let mut result = Vec::with_capacity(members.len());
        for member in members {
            let depth: DepthData = serde_json::from_str(&member)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize DepthData: {}", e)))?;
            result.push(depth);
        }

        Ok(result)
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, DatabaseError> {
        let key = Self::bar_key(&exchange, symbol, &interval);
        let mut conn = self.conn.clone();

        let count: i64 = conn.zcard(&key).await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to count bar data: {}", e)))?;

        if count > 0 {
            conn.del::<_, ()>(&key).await
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete bar data: {}", e)))?;
        }

        Ok(count)
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, DatabaseError> {
        let key = Self::tick_key(&exchange, symbol);
        let mut conn = self.conn.clone();

        let count: i64 = conn.zcard(&key).await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to count tick data: {}", e)))?;

        if count > 0 {
            conn.del::<_, ()>(&key).await
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete tick data: {}", e)))?;
        }

        Ok(count)
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, DatabaseError> {
        let mut conn = self.conn.clone();
        let mut overviews = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .cursor_arg(cursor)
                .arg("MATCH").arg("vnrs:bars:*")
                .arg("COUNT").arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to scan bar keys: {}", e)))?;

            for key in &keys {
                let count: i64 = conn.zcard(key).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get bar count for key {}: {}", key, e)))?;

                if count == 0 {
                    continue;
                }

                // ZRANGE with 0 0 WITHSCORES to get min timestamp
                let min_result: Vec<(String, f64)> = conn.zrange_withscores(key, 0, 0).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get min bar timestamp for key {}: {}", key, e)))?;

                // ZRANGE with -1 -1 WITHSCORES to get max timestamp
                let max_result: Vec<(String, f64)> = conn.zrange_withscores(key, -1, -1).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get max bar timestamp for key {}: {}", key, e)))?;

                let start = min_result.first()
                    .and_then(|(_, score)| DateTime::from_timestamp(*score as i64, 0));
                let end = max_result.first()
                    .and_then(|(_, score)| DateTime::from_timestamp(*score as i64, 0));

                // Parse key: vnrs:bars:{exchange}:{symbol}:{interval}
                let parts: Vec<&str> = key.split(':').collect();
                // parts: ["vnrs", "bars", exchange, symbol, interval]
                if parts.len() != 5 {
                    continue;
                }
                let exchange = match exchange_from_value(parts[2]) {
                    Some(e) => Some(e),
                    None => continue,
                };
                let symbol = parts[3].to_string();
                let interval = match interval_from_value(parts[4]) {
                    Some(i) => Some(i),
                    None => continue,
                };

                overviews.push(BarOverview {
                    symbol,
                    exchange,
                    interval,
                    count,
                    start,
                    end,
                });
            }

            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(overviews)
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, DatabaseError> {
        let mut conn = self.conn.clone();
        let mut overviews = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .cursor_arg(cursor)
                .arg("MATCH").arg("vnrs:ticks:*")
                .arg("COUNT").arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to scan tick keys: {}", e)))?;

            for key in &keys {
                let count: i64 = conn.zcard(key).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get tick count for key {}: {}", key, e)))?;

                if count == 0 {
                    continue;
                }

                let min_result: Vec<(String, f64)> = conn.zrange_withscores(key, 0, 0).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get min tick timestamp for key {}: {}", key, e)))?;

                let max_result: Vec<(String, f64)> = conn.zrange_withscores(key, -1, -1).await
                    .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get max tick timestamp for key {}: {}", key, e)))?;

                let start = min_result.first()
                    .and_then(|(_, score)| DateTime::from_timestamp(*score as i64, 0));
                let end = max_result.first()
                    .and_then(|(_, score)| DateTime::from_timestamp(*score as i64, 0));

                // Parse key: vnrs:ticks:{exchange}:{symbol}
                let parts: Vec<&str> = key.split(':').collect();
                // parts: ["vnrs", "ticks", exchange, symbol]
                if parts.len() != 4 {
                    continue;
                }
                let exchange = match exchange_from_value(parts[2]) {
                    Some(e) => Some(e),
                    None => continue,
                };
                let symbol = parts[3].to_string();

                overviews.push(TickOverview {
                    symbol,
                    exchange,
                    count,
                    start,
                    end,
                });
            }

            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(overviews)
    }

    async fn save_order_data(&self, orders: Vec<OrderData>) -> Result<bool, DatabaseError> {
        if orders.is_empty() {
            return Ok(true);
        }

        let mut pipe = redis::pipe();
        for order in &orders {
            let field = order.vt_orderid();
            let value = serde_json::to_string(order)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize OrderData: {}", e)))?;
            pipe.hset(Self::ORDERS_KEY, field, value).ignore();
        }

        pipe.query_async::<()>(&mut self.conn.clone()).await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "orders".to_string(),
                reason: format!("Failed to save order data: {}", e),
            })?;

        Ok(true)
    }

    async fn save_trade_data(&self, trades: Vec<TradeData>) -> Result<bool, DatabaseError> {
        if trades.is_empty() {
            return Ok(true);
        }

        let mut pipe = redis::pipe();
        for trade in &trades {
            let field = trade.vt_tradeid();
            let value = serde_json::to_string(trade)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize TradeData: {}", e)))?;
            pipe.hset(Self::TRADES_KEY, field, value).ignore();
        }

        pipe.query_async::<()>(&mut self.conn.clone()).await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "trades".to_string(),
                reason: format!("Failed to save trade data: {}", e),
            })?;

        Ok(true)
    }

    async fn save_position_data(&self, positions: Vec<PositionData>) -> Result<bool, DatabaseError> {
        if positions.is_empty() {
            return Ok(true);
        }

        let mut pipe = redis::pipe();
        for position in &positions {
            let field = position.vt_positionid();
            let value = serde_json::to_string(position)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize PositionData: {}", e)))?;
            pipe.hset(Self::POSITIONS_KEY, field, value).ignore();
        }

        pipe.query_async::<()>(&mut self.conn.clone()).await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "positions".to_string(),
                reason: format!("Failed to save position data: {}", e),
            })?;

        Ok(true)
    }

    async fn save_event(&self, event: EventRecord) -> Result<bool, DatabaseError> {
        let score = event.event_id as f64;
        let member = serde_json::to_string(&event)
            .map_err(|e| DatabaseError::SerializationError(format!("Failed to serialize EventRecord: {}", e)))?;

        self.conn.clone()
            .zadd::<_, _, _, ()>(Self::EVENTS_KEY, member, score)
            .await
            .map_err(|e| DatabaseError::InsertFailed {
                table: "events".to_string(),
                reason: format!("Failed to save event: {}", e),
            })?;

        Ok(true)
    }

    async fn load_orders(&self, gateway_name: Option<&str>) -> Result<Vec<OrderData>, DatabaseError> {
        let all: HashMap<String, String> = self.conn.clone()
            .hgetall(Self::ORDERS_KEY)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load orders: {}", e)))?;

        let mut result = Vec::with_capacity(all.len());
        for (_, value) in all {
            let order: OrderData = serde_json::from_str(&value)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize OrderData: {}", e)))?;
            if gateway_name.is_none_or(|gw| order.gateway_name == gw) {
                result.push(order);
            }
        }

        Ok(result)
    }

    async fn load_trades(&self, gateway_name: Option<&str>) -> Result<Vec<TradeData>, DatabaseError> {
        let all: HashMap<String, String> = self.conn.clone()
            .hgetall(Self::TRADES_KEY)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load trades: {}", e)))?;

        let mut result = Vec::with_capacity(all.len());
        for (_, value) in all {
            let trade: TradeData = serde_json::from_str(&value)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize TradeData: {}", e)))?;
            if gateway_name.is_none_or(|gw| trade.gateway_name == gw) {
                result.push(trade);
            }
        }

        Ok(result)
    }

    async fn load_positions(&self, gateway_name: Option<&str>) -> Result<Vec<PositionData>, DatabaseError> {
        let all: HashMap<String, String> = self.conn.clone()
            .hgetall(Self::POSITIONS_KEY)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load positions: {}", e)))?;

        let mut result = Vec::with_capacity(all.len());
        for (_, value) in all {
            let position: PositionData = serde_json::from_str(&value)
                .map_err(|e| DatabaseError::SerializationError(format!("Failed to deserialize PositionData: {}", e)))?;
            if gateway_name.is_none_or(|gw| position.gateway_name == gw) {
                result.push(position);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_key_format() {
        let key = RedisDatabase::bar_key(&Exchange::Binance, "BTCUSDT", &Interval::Minute);
        assert_eq!(key, "vnrs:bars:BINANCE:BTCUSDT:1m");
    }

    #[test]
    fn test_tick_key_format() {
        let key = RedisDatabase::tick_key(&Exchange::Binance, "BTCUSDT");
        assert_eq!(key, "vnrs:ticks:BINANCE:BTCUSDT");
    }

    #[test]
    fn test_depth_key_format() {
        let key = RedisDatabase::depth_key(&Exchange::Binance, "BTCUSDT");
        assert_eq!(key, "vnrs:depths:BINANCE:BTCUSDT");
    }

    #[test]
    fn test_bar_key_different_intervals() {
        let key_1h = RedisDatabase::bar_key(&Exchange::Binance, "ETHUSDT", &Interval::Hour);
        assert_eq!(key_1h, "vnrs:bars:BINANCE:ETHUSDT:1h");

        let key_daily = RedisDatabase::bar_key(&Exchange::Okx, "BTCUSDT", &Interval::Daily);
        assert_eq!(key_daily, "vnrs:bars:OKX:BTCUSDT:d");
    }

    #[test]
    fn test_static_keys() {
        assert_eq!(RedisDatabase::ORDERS_KEY, "vnrs:orders");
        assert_eq!(RedisDatabase::TRADES_KEY, "vnrs:trades");
        assert_eq!(RedisDatabase::POSITIONS_KEY, "vnrs:positions");
        assert_eq!(RedisDatabase::EVENTS_KEY, "vnrs:events");
    }

    #[tokio::test]
    async fn test_redis_connection_failure() {
        let result = RedisDatabase::connect("redis://127.0.0.1:19999").await;
        assert!(result.is_err(), "Connecting to non-existent Redis should fail");
    }

    #[test]
    fn test_exchange_from_value() {
        assert_eq!(exchange_from_value("BINANCE"), Some(Exchange::Binance));
        assert_eq!(exchange_from_value("OKX"), Some(Exchange::Okx));
        assert_eq!(exchange_from_value("BINANCE_USDM"), Some(Exchange::BinanceUsdm));
        assert_eq!(exchange_from_value("INVALID"), None);
    }

    #[test]
    fn test_interval_from_value() {
        assert_eq!(interval_from_value("1m"), Some(Interval::Minute));
        assert_eq!(interval_from_value("1h"), Some(Interval::Hour));
        assert_eq!(interval_from_value("d"), Some(Interval::Daily));
        assert_eq!(interval_from_value("tick"), Some(Interval::Tick));
        assert_eq!(interval_from_value("invalid"), None);
    }

    #[test]
    fn test_bar_key_roundtrip() {
        let key = RedisDatabase::bar_key(&Exchange::BinanceUsdm, "ETHUSDT", &Interval::Hour4);
        // Parse back from key: vnrs:bars:{exchange}:{symbol}:{interval}
        let parts: Vec<&str> = key.split(':').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], "vnrs");
        assert_eq!(parts[1], "bars");
        assert_eq!(exchange_from_value(parts[2]), Some(Exchange::BinanceUsdm));
        assert_eq!(parts[3], "ETHUSDT");
        assert_eq!(interval_from_value(parts[4]), Some(Interval::Hour4));
    }

    #[test]
    fn test_tick_key_roundtrip() {
        let key = RedisDatabase::tick_key(&Exchange::Okx, "BTCUSDT");
        let parts: Vec<&str> = key.split(':').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "vnrs");
        assert_eq!(parts[1], "ticks");
        assert_eq!(exchange_from_value(parts[2]), Some(Exchange::Okx));
        assert_eq!(parts[3], "BTCUSDT");
    }
}
