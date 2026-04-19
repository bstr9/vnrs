//! Data converter for Rust-Python interop
//! Converts between Rust data structures and Python/Arrow representations

use crate::trader::{BarData, TickData};
#[cfg(feature = "alpha")]
use polars::prelude::*;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Parse an exchange string into the Exchange enum.
fn parse_exchange_str(s: &str) -> crate::trader::Exchange {
    match s.to_uppercase().as_str() {
        "BINANCE" => crate::trader::Exchange::Binance,
        "BINANCE_USDM" | "BINANCEUSDM" => crate::trader::Exchange::BinanceUsdm,
        "BINANCE_COINM" | "BINANCECOINM" => crate::trader::Exchange::BinanceCoinm,
        "OKX" => crate::trader::Exchange::Okx,
        "BYBIT" => crate::trader::Exchange::Bybit,
        "LOCAL" => crate::trader::Exchange::Local,
        _ => crate::trader::Exchange::Global,
    }
}

/// Parse an interval string into the Interval enum.
fn parse_interval_str(s: &str) -> Option<crate::trader::Interval> {
    match s.to_lowercase().as_str() {
        "1s" => Some(crate::trader::Interval::Second),
        "1m" | "minute" => Some(crate::trader::Interval::Minute),
        "5m" => Some(crate::trader::Interval::Minute5),
        "15m" => Some(crate::trader::Interval::Minute15),
        "30m" => Some(crate::trader::Interval::Minute30),
        "1h" | "hour" => Some(crate::trader::Interval::Hour),
        "4h" => Some(crate::trader::Interval::Hour4),
        "d" | "1d" | "daily" => Some(crate::trader::Interval::Daily),
        "w" | "weekly" => Some(crate::trader::Interval::Weekly),
        "tick" => Some(crate::trader::Interval::Tick),
        _ => None,
    }
}

macro_rules! get_required {
    ($dict:expr, $key:expr, $type:ty) => {
        $dict
            .get_item($key)?
            .ok_or_else(|| {
                pyo3::exceptions::PyKeyError::new_err(format!("Missing required key: {}", $key))
            })?
            .extract::<$type>()?
    };
}

/// Convert Rust BarData to Python dict
pub fn bar_to_py<'py>(py: Python<'py>, bar: &BarData) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("symbol", &bar.symbol)?;
    dict.set_item("exchange", format!("{:?}", bar.exchange))?;
    dict.set_item("datetime", bar.datetime.to_rfc3339())?;
    dict.set_item("open", bar.open_price)?;
    dict.set_item("high", bar.high_price)?;
    dict.set_item("low", bar.low_price)?;
    dict.set_item("close", bar.close_price)?;
    dict.set_item("volume", bar.volume)?;
    dict.set_item("turnover", bar.turnover)?;
    dict.set_item("open_interest", bar.open_interest)?;
    dict.set_item("gateway_name", &bar.gateway_name)?;

    if let Some(interval) = bar.interval {
        dict.set_item("interval", format!("{:?}", interval))?;
    }

    Ok(dict)
}

/// Convert Python dict to Rust BarData
pub fn py_to_bar(_py: Python, py_dict: &Bound<'_, PyDict>) -> PyResult<BarData> {
    let symbol: String = get_required!(py_dict, "symbol", String);
    let datetime_str: String = get_required!(py_dict, "datetime", String);

    // Parse datetime
    let datetime = chrono::DateTime::parse_from_rfc3339(&datetime_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid datetime: {}", e)))?
        .into();

    // Parse exchange
    let exchange = if let Some(exchange_py) = py_dict.get_item("exchange")? {
        let exchange_str: String = exchange_py.extract()?;
        parse_exchange_str(&exchange_str)
    } else {
        crate::trader::Exchange::Global
    };

    // Parse interval
    let interval = if let Some(interval_py) = py_dict.get_item("interval")? {
        let interval_str: String = interval_py.extract()?;
        parse_interval_str(&interval_str)
    } else {
        None
    };

    Ok(BarData {
        symbol,
        exchange,
        datetime,
        interval,
        volume: get_required!(py_dict, "volume", f64),
        turnover: get_required!(py_dict, "turnover", f64),
        open_interest: get_required!(py_dict, "open_interest", f64),
        open_price: get_required!(py_dict, "open", f64),
        high_price: get_required!(py_dict, "high", f64),
        low_price: get_required!(py_dict, "low", f64),
        close_price: get_required!(py_dict, "close", f64),
        gateway_name: get_required!(py_dict, "gateway_name", String),
        extra: None,
    })
}

/// Convert vector of BarData to Polars DataFrame representation
#[cfg(feature = "alpha")]
pub fn bars_to_arrow(bars: &[BarData]) -> Result<DataFrame, Box<dyn std::error::Error>> {
    let mut symbols = Vec::new();
    let mut exchanges = Vec::new();
    let mut datetimes = Vec::new();
    let mut opens = Vec::new();
    let mut highs = Vec::new();
    let mut lows = Vec::new();
    let mut closes = Vec::new();
    let mut volumes = Vec::new();
    let mut turnovers = Vec::new();
    let mut open_interests = Vec::new();
    let mut gateway_names = Vec::new();
    let mut intervals = Vec::new();

    for bar in bars {
        symbols.push(bar.symbol.clone());
        exchanges.push(format!("{:?}", bar.exchange));
        datetimes.push(bar.datetime.timestamp_millis());
        opens.push(bar.open_price);
        highs.push(bar.high_price);
        lows.push(bar.low_price);
        closes.push(bar.close_price);
        volumes.push(bar.volume);
        turnovers.push(bar.turnover);
        open_interests.push(bar.open_interest);
        gateway_names.push(bar.gateway_name.clone());
        intervals.push(
            bar.interval
                .map(|i| format!("{:?}", i))
                .unwrap_or("".to_string()),
        );
    }

    let df = DataFrame::new(vec![
        Column::new("symbol".into(), symbols),
        Column::new("exchange".into(), exchanges),
        Column::new("datetime".into(), datetimes),
        Column::new("open".into(), opens),
        Column::new("high".into(), highs),
        Column::new("low".into(), lows),
        Column::new("close".into(), closes),
        Column::new("volume".into(), volumes),
        Column::new("turnover".into(), turnovers),
        Column::new("open_interest".into(), open_interests),
        Column::new("gateway_name".into(), gateway_names),
        Column::new("interval".into(), intervals),
    ])?;

    Ok(df)
}

/// Convert Polars DataFrame to vector of BarData
#[cfg(feature = "alpha")]
pub fn arrow_to_bars(df: &DataFrame) -> Result<Vec<BarData>, Box<dyn std::error::Error>> {
    let mut bars = Vec::new();

    let symbols = df.column("symbol")?.str()?;
    let datetimes = df.column("datetime")?.i64()?;
    let opens = df.column("open")?.f64()?;
    let highs = df.column("high")?.f64()?;
    let lows = df.column("low")?.f64()?;
    let closes = df.column("close")?.f64()?;
    let volumes = df.column("volume")?.f64()?;
    let turnovers = df.column("turnover")?.f64()?;
    let open_interests = df.column("open_interest")?.f64()?;
    let gateway_names = df.column("gateway_name")?.str()?;

    let exchanges = match df.column("exchange") {
        Ok(col) => col.str()?,
        Err(_) => {
            return Ok(Vec::new());
        }
    };

    // Try to get interval column (optional)
    let intervals = df.column("interval").ok().and_then(|col| col.str().ok());

    for i in 0..df.height() {
        let dt_millis = datetimes.get(i).unwrap_or(0);
        let datetime =
            chrono::DateTime::from_timestamp_millis(dt_millis).unwrap_or_else(chrono::Utc::now);
        let exchange_str = exchanges.get(i).unwrap_or("BINANCE");
        let exchange = parse_exchange_str(exchange_str);

        // Parse interval from column if available
        let interval = intervals
            .as_ref()
            .and_then(|ints| ints.get(i))
            .and_then(|s| if s.is_empty() { None } else { parse_interval_str(s) });

        let bar = BarData {
            symbol: symbols.get(i).unwrap_or("").to_string(),
            exchange,
            datetime,
            interval,
            volume: volumes.get(i).unwrap_or(0.0),
            turnover: turnovers.get(i).unwrap_or(0.0),
            open_interest: open_interests.get(i).unwrap_or(0.0),
            open_price: opens.get(i).unwrap_or(0.0),
            high_price: highs.get(i).unwrap_or(0.0),
            low_price: lows.get(i).unwrap_or(0.0),
            close_price: closes.get(i).unwrap_or(0.0),
            gateway_name: gateway_names.get(i).unwrap_or("").to_string(),
            extra: None,
        };
        bars.push(bar);
    }

    Ok(bars)
}

/// Convert Rust TickData to Python
pub fn tick_to_py<'py>(py: Python<'py>, tick: &TickData) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("symbol", &tick.symbol)?;
    dict.set_item("exchange", format!("{:?}", tick.exchange))?;
    dict.set_item("datetime", tick.datetime.to_rfc3339())?;
    dict.set_item("name", &tick.name)?;
    dict.set_item("volume", tick.volume)?;
    dict.set_item("turnover", tick.turnover)?;
    dict.set_item("open_interest", tick.open_interest)?;
    dict.set_item("last_price", tick.last_price)?;
    dict.set_item("last_volume", tick.last_volume)?;
    dict.set_item("limit_up", tick.limit_up)?;
    dict.set_item("limit_down", tick.limit_down)?;
    dict.set_item("open_price", tick.open_price)?;
    dict.set_item("high_price", tick.high_price)?;
    dict.set_item("low_price", tick.low_price)?;
    dict.set_item("pre_close", tick.pre_close)?;
    dict.set_item("bid_price_1", tick.bid_price_1)?;
    dict.set_item("bid_price_2", tick.bid_price_2)?;
    dict.set_item("bid_price_3", tick.bid_price_3)?;
    dict.set_item("bid_price_4", tick.bid_price_4)?;
    dict.set_item("bid_price_5", tick.bid_price_5)?;
    dict.set_item("ask_price_1", tick.ask_price_1)?;
    dict.set_item("ask_price_2", tick.ask_price_2)?;
    dict.set_item("ask_price_3", tick.ask_price_3)?;
    dict.set_item("ask_price_4", tick.ask_price_4)?;
    dict.set_item("ask_price_5", tick.ask_price_5)?;
    dict.set_item("bid_volume_1", tick.bid_volume_1)?;
    dict.set_item("bid_volume_2", tick.bid_volume_2)?;
    dict.set_item("bid_volume_3", tick.bid_volume_3)?;
    dict.set_item("bid_volume_4", tick.bid_volume_4)?;
    dict.set_item("bid_volume_5", tick.bid_volume_5)?;
    dict.set_item("ask_volume_1", tick.ask_volume_1)?;
    dict.set_item("ask_volume_2", tick.ask_volume_2)?;
    dict.set_item("ask_volume_3", tick.ask_volume_3)?;
    dict.set_item("ask_volume_4", tick.ask_volume_4)?;
    dict.set_item("ask_volume_5", tick.ask_volume_5)?;
    dict.set_item("gateway_name", &tick.gateway_name)?;

    Ok(dict)
}

/// Parse direction string into Direction enum
fn parse_direction_str(s: &str) -> Option<crate::trader::Direction> {
    match s.to_uppercase().as_str() {
        "LONG" | "多" => Some(crate::trader::Direction::Long),
        "SHORT" | "空" => Some(crate::trader::Direction::Short),
        "NET" | "净" => Some(crate::trader::Direction::Net),
        _ => None,
    }
}

/// Parse offset string into Offset enum
fn parse_offset_str(s: &str) -> crate::trader::Offset {
    match s.to_uppercase().as_str() {
        "OPEN" | "开" => crate::trader::Offset::Open,
        "CLOSE" | "平" => crate::trader::Offset::Close,
        "CLOSETODAY" | "平今" => crate::trader::Offset::CloseToday,
        "CLOSEYESTERDAY" | "平昨" => crate::trader::Offset::CloseYesterday,
        _ => crate::trader::Offset::None,
    }
}

/// Parse status string into Status enum
fn parse_status_str(s: &str) -> crate::trader::Status {
    match s.to_uppercase().as_str() {
        "SUBMITTING" | "提交中" => crate::trader::Status::Submitting,
        "NOTTRADED" | "未成交" => crate::trader::Status::NotTraded,
        "PARTTRADED" | "部分成交" => crate::trader::Status::PartTraded,
        "ALLTRADED" | "全部成交" => crate::trader::Status::AllTraded,
        "CANCELLED" | "已撤销" => crate::trader::Status::Cancelled,
        "REJECTED" | "拒单" => crate::trader::Status::Rejected,
        _ => crate::trader::Status::Submitting,
    }
}

/// Get an optional item from a dict, returning None if missing or extraction fails
fn get_optional<T: for<'a, 'py> pyo3::FromPyObject<'a, 'py>>(dict: &Bound<'_, PyDict>, key: &str) -> Option<T> {
    dict.get_item(key).ok().flatten().and_then(|v| v.extract::<T>().ok())
}

/// Convert Python dict to Rust TickData
pub fn py_to_tick(_py: Python, py_dict: &Bound<'_, PyDict>) -> PyResult<TickData> {
    let symbol: String = get_required!(py_dict, "symbol", String);
    let datetime_str: String = get_required!(py_dict, "datetime", String);

    let datetime = chrono::DateTime::parse_from_rfc3339(&datetime_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid datetime: {}", e)))?
        .into();

    let exchange = get_optional::<String>(py_dict, "exchange")
        .map(|s| parse_exchange_str(&s))
        .unwrap_or(crate::trader::Exchange::Global);

    let gateway_name = get_optional::<String>(py_dict, "gateway_name").unwrap_or_default();

    Ok(TickData {
        symbol,
        exchange,
        datetime,
        gateway_name,
        name: get_optional::<String>(py_dict, "name").unwrap_or_default(),
        volume: get_optional::<f64>(py_dict, "volume").unwrap_or(0.0),
        turnover: get_optional::<f64>(py_dict, "turnover").unwrap_or(0.0),
        open_interest: get_optional::<f64>(py_dict, "open_interest").unwrap_or(0.0),
        last_price: get_optional::<f64>(py_dict, "last_price").unwrap_or(0.0),
        last_volume: get_optional::<f64>(py_dict, "last_volume").unwrap_or(0.0),
        limit_up: get_optional::<f64>(py_dict, "limit_up").unwrap_or(0.0),
        limit_down: get_optional::<f64>(py_dict, "limit_down").unwrap_or(0.0),
        open_price: get_optional::<f64>(py_dict, "open_price").unwrap_or(0.0),
        high_price: get_optional::<f64>(py_dict, "high_price").unwrap_or(0.0),
        low_price: get_optional::<f64>(py_dict, "low_price").unwrap_or(0.0),
        pre_close: get_optional::<f64>(py_dict, "pre_close").unwrap_or(0.0),
        bid_price_1: get_optional::<f64>(py_dict, "bid_price_1").unwrap_or(0.0),
        bid_price_2: get_optional::<f64>(py_dict, "bid_price_2").unwrap_or(0.0),
        bid_price_3: get_optional::<f64>(py_dict, "bid_price_3").unwrap_or(0.0),
        bid_price_4: get_optional::<f64>(py_dict, "bid_price_4").unwrap_or(0.0),
        bid_price_5: get_optional::<f64>(py_dict, "bid_price_5").unwrap_or(0.0),
        ask_price_1: get_optional::<f64>(py_dict, "ask_price_1").unwrap_or(0.0),
        ask_price_2: get_optional::<f64>(py_dict, "ask_price_2").unwrap_or(0.0),
        ask_price_3: get_optional::<f64>(py_dict, "ask_price_3").unwrap_or(0.0),
        ask_price_4: get_optional::<f64>(py_dict, "ask_price_4").unwrap_or(0.0),
        ask_price_5: get_optional::<f64>(py_dict, "ask_price_5").unwrap_or(0.0),
        bid_volume_1: get_optional::<f64>(py_dict, "bid_volume_1").unwrap_or(0.0),
        bid_volume_2: get_optional::<f64>(py_dict, "bid_volume_2").unwrap_or(0.0),
        bid_volume_3: get_optional::<f64>(py_dict, "bid_volume_3").unwrap_or(0.0),
        bid_volume_4: get_optional::<f64>(py_dict, "bid_volume_4").unwrap_or(0.0),
        bid_volume_5: get_optional::<f64>(py_dict, "bid_volume_5").unwrap_or(0.0),
        ask_volume_1: get_optional::<f64>(py_dict, "ask_volume_1").unwrap_or(0.0),
        ask_volume_2: get_optional::<f64>(py_dict, "ask_volume_2").unwrap_or(0.0),
        ask_volume_3: get_optional::<f64>(py_dict, "ask_volume_3").unwrap_or(0.0),
        ask_volume_4: get_optional::<f64>(py_dict, "ask_volume_4").unwrap_or(0.0),
        ask_volume_5: get_optional::<f64>(py_dict, "ask_volume_5").unwrap_or(0.0),
        localtime: None,
        extra: None,
    })
}

/// Convert Python dict to Rust OrderData
pub fn py_to_order(_py: Python, py_dict: &Bound<'_, PyDict>) -> PyResult<crate::trader::OrderData> {
    let symbol: String = get_required!(py_dict, "symbol", String);
    let orderid: String = get_required!(py_dict, "orderid", String);

    let exchange = get_optional::<String>(py_dict, "exchange")
        .map(|s| parse_exchange_str(&s))
        .unwrap_or(crate::trader::Exchange::Global);

    let gateway_name = get_optional::<String>(py_dict, "gateway_name").unwrap_or_default();

    let direction = get_optional::<String>(py_dict, "direction")
        .and_then(|s| parse_direction_str(&s));

    let offset = get_optional::<String>(py_dict, "offset")
        .map(|s| parse_offset_str(&s))
        .unwrap_or(crate::trader::Offset::None);

    let status = get_optional::<String>(py_dict, "status")
        .map(|s| parse_status_str(&s))
        .unwrap_or(crate::trader::Status::Submitting);

    let datetime = get_optional::<String>(py_dict, "datetime")
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.into());

    Ok(crate::trader::OrderData {
        symbol,
        exchange,
        orderid,
        gateway_name,
        direction,
        offset,
        status,
        datetime,
        order_type: crate::trader::OrderType::Limit,
        price: get_optional::<f64>(py_dict, "price").unwrap_or(0.0),
        volume: get_optional::<f64>(py_dict, "volume").unwrap_or(0.0),
        traded: get_optional::<f64>(py_dict, "traded").unwrap_or(0.0),
        reference: String::new(),
        extra: None,
    })
}

/// Convert Python dict to Rust TradeData
pub fn py_to_trade(_py: Python, py_dict: &Bound<'_, PyDict>) -> PyResult<crate::trader::TradeData> {
    let symbol: String = get_required!(py_dict, "symbol", String);
    let orderid: String = get_required!(py_dict, "orderid", String);
    let tradeid: String = get_required!(py_dict, "tradeid", String);

    let exchange = get_optional::<String>(py_dict, "exchange")
        .map(|s| parse_exchange_str(&s))
        .unwrap_or(crate::trader::Exchange::Global);

    let gateway_name = get_optional::<String>(py_dict, "gateway_name").unwrap_or_default();

    let direction = get_optional::<String>(py_dict, "direction")
        .and_then(|s| parse_direction_str(&s));

    let offset = get_optional::<String>(py_dict, "offset")
        .map(|s| parse_offset_str(&s))
        .unwrap_or(crate::trader::Offset::None);

    let datetime = get_optional::<String>(py_dict, "datetime")
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.into());

    Ok(crate::trader::TradeData {
        symbol,
        exchange,
        orderid,
        tradeid,
        gateway_name,
        direction,
        offset,
        datetime,
        price: get_optional::<f64>(py_dict, "price").unwrap_or(0.0),
        volume: get_optional::<f64>(py_dict, "volume").unwrap_or(0.0),
        extra: None,
    })
}
