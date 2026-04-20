//! Python typed data classes for TickData, OrderData, and TradeData
//!
//! Replaces dict-based Python interop with proper PyO3 `#[pyclass]` structs,
//! following the existing PyBarData pattern in backtesting_bindings.rs.

use pyo3::prelude::*;

use crate::trader::{
    Direction, Exchange, Offset, OrderData, OrderType, Status, TickData, TradeData,
};

// ---------------------------------------------------------------------------
// PyTickData
// ---------------------------------------------------------------------------

/// Python wrapper for TickData
#[pyclass]
#[derive(Clone)]
pub struct PyTickData {
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: String,

    pub datetime: String, // Internal storage as ISO-8601 / RFC 3339

    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub volume: f64,
    #[pyo3(get, set)]
    pub turnover: f64,
    #[pyo3(get, set)]
    pub open_interest: f64,
    #[pyo3(get, set)]
    pub last_price: f64,
    #[pyo3(get, set)]
    pub last_volume: f64,
    #[pyo3(get, set)]
    pub limit_up: f64,
    #[pyo3(get, set)]
    pub limit_down: f64,
    #[pyo3(get, set)]
    pub open_price: f64,
    #[pyo3(get, set)]
    pub high_price: f64,
    #[pyo3(get, set)]
    pub low_price: f64,
    #[pyo3(get, set)]
    pub pre_close: f64,
    #[pyo3(get, set)]
    pub bid_price_1: f64,
    #[pyo3(get, set)]
    pub bid_price_2: f64,
    #[pyo3(get, set)]
    pub bid_price_3: f64,
    #[pyo3(get, set)]
    pub bid_price_4: f64,
    #[pyo3(get, set)]
    pub bid_price_5: f64,
    #[pyo3(get, set)]
    pub ask_price_1: f64,
    #[pyo3(get, set)]
    pub ask_price_2: f64,
    #[pyo3(get, set)]
    pub ask_price_3: f64,
    #[pyo3(get, set)]
    pub ask_price_4: f64,
    #[pyo3(get, set)]
    pub ask_price_5: f64,
    #[pyo3(get, set)]
    pub bid_volume_1: f64,
    #[pyo3(get, set)]
    pub bid_volume_2: f64,
    #[pyo3(get, set)]
    pub bid_volume_3: f64,
    #[pyo3(get, set)]
    pub bid_volume_4: f64,
    #[pyo3(get, set)]
    pub bid_volume_5: f64,
    #[pyo3(get, set)]
    pub ask_volume_1: f64,
    #[pyo3(get, set)]
    pub ask_volume_2: f64,
    #[pyo3(get, set)]
    pub ask_volume_3: f64,
    #[pyo3(get, set)]
    pub ask_volume_4: f64,
    #[pyo3(get, set)]
    pub ask_volume_5: f64,
}

#[pymethods]
impl PyTickData {
    #[new]
    #[pyo3(signature = (
        gateway_name="".into(),
        symbol="".into(),
        exchange="".into(),
        datetime="".into(),
        name="".into(),
        volume=0.0,
        turnover=0.0,
        open_interest=0.0,
        last_price=0.0,
        last_volume=0.0,
        limit_up=0.0,
        limit_down=0.0,
        open_price=0.0,
        high_price=0.0,
        low_price=0.0,
        pre_close=0.0,
        bid_price_1=0.0,
        bid_price_2=0.0,
        bid_price_3=0.0,
        bid_price_4=0.0,
        bid_price_5=0.0,
        ask_price_1=0.0,
        ask_price_2=0.0,
        ask_price_3=0.0,
        ask_price_4=0.0,
        ask_price_5=0.0,
        bid_volume_1=0.0,
        bid_volume_2=0.0,
        bid_volume_3=0.0,
        bid_volume_4=0.0,
        bid_volume_5=0.0,
        ask_volume_1=0.0,
        ask_volume_2=0.0,
        ask_volume_3=0.0,
        ask_volume_4=0.0,
        ask_volume_5=0.0
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: String,
        datetime: String,
        name: String,
        volume: f64,
        turnover: f64,
        open_interest: f64,
        last_price: f64,
        last_volume: f64,
        limit_up: f64,
        limit_down: f64,
        open_price: f64,
        high_price: f64,
        low_price: f64,
        pre_close: f64,
        bid_price_1: f64,
        bid_price_2: f64,
        bid_price_3: f64,
        bid_price_4: f64,
        bid_price_5: f64,
        ask_price_1: f64,
        ask_price_2: f64,
        ask_price_3: f64,
        ask_price_4: f64,
        ask_price_5: f64,
        bid_volume_1: f64,
        bid_volume_2: f64,
        bid_volume_3: f64,
        bid_volume_4: f64,
        bid_volume_5: f64,
        ask_volume_1: f64,
        ask_volume_2: f64,
        ask_volume_3: f64,
        ask_volume_4: f64,
        ask_volume_5: f64,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            datetime,
            name,
            volume,
            turnover,
            open_interest,
            last_price,
            last_volume,
            limit_up,
            limit_down,
            open_price,
            high_price,
            low_price,
            pre_close,
            bid_price_1,
            bid_price_2,
            bid_price_3,
            bid_price_4,
            bid_price_5,
            ask_price_1,
            ask_price_2,
            ask_price_3,
            ask_price_4,
            ask_price_5,
            bid_volume_1,
            bid_volume_2,
            bid_volume_3,
            bid_volume_4,
            bid_volume_5,
            ask_volume_1,
            ask_volume_2,
            ask_volume_3,
            ask_volume_4,
            ask_volume_5,
        }
    }

    #[getter]
    fn get_datetime<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let dt_cls = py.import("datetime")?.getattr("datetime")?;
        match dt_cls.call_method1("fromisoformat", (&self.datetime,)) {
            Ok(dt) => Ok(dt),
            Err(_) => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid datetime format: {}",
                self.datetime
            ))),
        }
    }

    #[setter]
    fn set_datetime(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(s) = value.extract::<String>() {
            self.datetime = s;
        } else {
            self.datetime = value.call_method0("isoformat")?.extract::<String>()?;
        }
        Ok(())
    }

    /// Support dict-style access: tick["last_price"] → tick.last_price
    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match key {
            "gateway_name" => Ok(self.gateway_name.clone().into_pyobject(py)?.into_any().unbind()),
            "symbol" => Ok(self.symbol.clone().into_pyobject(py)?.into_any().unbind()),
            "exchange" => Ok(self.exchange.clone().into_pyobject(py)?.into_any().unbind()),
            "datetime" => {
                let dt_cls = py.import("datetime")?.getattr("datetime")?;
                let dt = dt_cls.call_method1("fromisoformat", (&self.datetime,))?;
                Ok(dt.into_any().unbind())
            }
            "name" => Ok(self.name.clone().into_pyobject(py)?.into_any().unbind()),
            "volume" => Ok(self.volume.into_pyobject(py)?.into_any().unbind()),
            "turnover" => Ok(self.turnover.into_pyobject(py)?.into_any().unbind()),
            "open_interest" => Ok(self.open_interest.into_pyobject(py)?.into_any().unbind()),
            "last_price" | "last" => Ok(self.last_price.into_pyobject(py)?.into_any().unbind()),
            "last_volume" => Ok(self.last_volume.into_pyobject(py)?.into_any().unbind()),
            "limit_up" => Ok(self.limit_up.into_pyobject(py)?.into_any().unbind()),
            "limit_down" => Ok(self.limit_down.into_pyobject(py)?.into_any().unbind()),
            "open_price" | "open" => Ok(self.open_price.into_pyobject(py)?.into_any().unbind()),
            "high_price" | "high" => Ok(self.high_price.into_pyobject(py)?.into_any().unbind()),
            "low_price" | "low" => Ok(self.low_price.into_pyobject(py)?.into_any().unbind()),
            "pre_close" => Ok(self.pre_close.into_pyobject(py)?.into_any().unbind()),
            "bid_price_1" | "bid1" => Ok(self.bid_price_1.into_pyobject(py)?.into_any().unbind()),
            "bid_price_2" => Ok(self.bid_price_2.into_pyobject(py)?.into_any().unbind()),
            "bid_price_3" => Ok(self.bid_price_3.into_pyobject(py)?.into_any().unbind()),
            "bid_price_4" => Ok(self.bid_price_4.into_pyobject(py)?.into_any().unbind()),
            "bid_price_5" => Ok(self.bid_price_5.into_pyobject(py)?.into_any().unbind()),
            "ask_price_1" | "ask1" => Ok(self.ask_price_1.into_pyobject(py)?.into_any().unbind()),
            "ask_price_2" => Ok(self.ask_price_2.into_pyobject(py)?.into_any().unbind()),
            "ask_price_3" => Ok(self.ask_price_3.into_pyobject(py)?.into_any().unbind()),
            "ask_price_4" => Ok(self.ask_price_4.into_pyobject(py)?.into_any().unbind()),
            "ask_price_5" => Ok(self.ask_price_5.into_pyobject(py)?.into_any().unbind()),
            "bid_volume_1" => Ok(self.bid_volume_1.into_pyobject(py)?.into_any().unbind()),
            "bid_volume_2" => Ok(self.bid_volume_2.into_pyobject(py)?.into_any().unbind()),
            "bid_volume_3" => Ok(self.bid_volume_3.into_pyobject(py)?.into_any().unbind()),
            "bid_volume_4" => Ok(self.bid_volume_4.into_pyobject(py)?.into_any().unbind()),
            "bid_volume_5" => Ok(self.bid_volume_5.into_pyobject(py)?.into_any().unbind()),
            "ask_volume_1" => Ok(self.ask_volume_1.into_pyobject(py)?.into_any().unbind()),
            "ask_volume_2" => Ok(self.ask_volume_2.into_pyobject(py)?.into_any().unbind()),
            "ask_volume_3" => Ok(self.ask_volume_3.into_pyobject(py)?.into_any().unbind()),
            "ask_volume_4" => Ok(self.ask_volume_4.into_pyobject(py)?.into_any().unbind()),
            "ask_volume_5" => Ok(self.ask_volume_5.into_pyobject(py)?.into_any().unbind()),
            _ => Err(pyo3::exceptions::PyKeyError::new_err(format!(
                "TickData has no key '{}'",
                key
            ))),
        }
    }

    /// Support dict-style .get() method: tick.get("last_price", 0.0)
    fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default_value: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.__getitem__(py, key) {
            Ok(val) => Ok(val),
            Err(_) => Ok(default_value.unwrap_or_else(|| {
                py.None().into_pyobject(py).unwrap().into_any().unbind()
            })),
        }
    }
}

impl PyTickData {
    /// Convert a Rust TickData into a PyTickData
    pub fn from_rust(tick: &TickData) -> Self {
        Self {
            gateway_name: tick.gateway_name.clone(),
            symbol: tick.symbol.clone(),
            exchange: tick.exchange.value().to_string(),
            datetime: tick.datetime.to_rfc3339(),
            name: tick.name.clone(),
            volume: tick.volume,
            turnover: tick.turnover,
            open_interest: tick.open_interest,
            last_price: tick.last_price,
            last_volume: tick.last_volume,
            limit_up: tick.limit_up,
            limit_down: tick.limit_down,
            open_price: tick.open_price,
            high_price: tick.high_price,
            low_price: tick.low_price,
            pre_close: tick.pre_close,
            bid_price_1: tick.bid_price_1,
            bid_price_2: tick.bid_price_2,
            bid_price_3: tick.bid_price_3,
            bid_price_4: tick.bid_price_4,
            bid_price_5: tick.bid_price_5,
            ask_price_1: tick.ask_price_1,
            ask_price_2: tick.ask_price_2,
            ask_price_3: tick.ask_price_3,
            ask_price_4: tick.ask_price_4,
            ask_price_5: tick.ask_price_5,
            bid_volume_1: tick.bid_volume_1,
            bid_volume_2: tick.bid_volume_2,
            bid_volume_3: tick.bid_volume_3,
            bid_volume_4: tick.bid_volume_4,
            bid_volume_5: tick.bid_volume_5,
            ask_volume_1: tick.ask_volume_1,
            ask_volume_2: tick.ask_volume_2,
            ask_volume_3: tick.ask_volume_3,
            ask_volume_4: tick.ask_volume_4,
            ask_volume_5: tick.ask_volume_5,
        }
    }

    pub fn to_rust(&self) -> PyResult<TickData> {
        let exchange = parse_exchange(&self.exchange);
        let datetime = chrono::DateTime::parse_from_rfc3339(&self.datetime)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
            .with_timezone(&chrono::Utc);

        Ok(TickData {
            gateway_name: self.gateway_name.clone(),
            symbol: self.symbol.clone(),
            exchange,
            datetime,
            name: self.name.clone(),
            volume: self.volume,
            turnover: self.turnover,
            open_interest: self.open_interest,
            last_price: self.last_price,
            last_volume: self.last_volume,
            limit_up: self.limit_up,
            limit_down: self.limit_down,
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            pre_close: self.pre_close,
            bid_price_1: self.bid_price_1,
            bid_price_2: self.bid_price_2,
            bid_price_3: self.bid_price_3,
            bid_price_4: self.bid_price_4,
            bid_price_5: self.bid_price_5,
            ask_price_1: self.ask_price_1,
            ask_price_2: self.ask_price_2,
            ask_price_3: self.ask_price_3,
            ask_price_4: self.ask_price_4,
            ask_price_5: self.ask_price_5,
            bid_volume_1: self.bid_volume_1,
            bid_volume_2: self.bid_volume_2,
            bid_volume_3: self.bid_volume_3,
            bid_volume_4: self.bid_volume_4,
            bid_volume_5: self.bid_volume_5,
            ask_volume_1: self.ask_volume_1,
            ask_volume_2: self.ask_volume_2,
            ask_volume_3: self.ask_volume_3,
            ask_volume_4: self.ask_volume_4,
            ask_volume_5: self.ask_volume_5,
            localtime: None,
            extra: None,
        })
    }
}

// ---------------------------------------------------------------------------
// PyOrderData
// ---------------------------------------------------------------------------

/// Python wrapper for OrderData
#[pyclass]
#[derive(Clone)]
pub struct PyOrderData {
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: String,
    #[pyo3(get, set)]
    pub orderid: String,
    #[pyo3(get, set)]
    pub order_type: String,
    #[pyo3(get, set)]
    pub direction: String,
    #[pyo3(get, set)]
    pub offset: String,
    #[pyo3(get, set)]
    pub price: f64,
    #[pyo3(get, set)]
    pub volume: f64,
    #[pyo3(get, set)]
    pub traded: f64,
    #[pyo3(get, set)]
    pub status: String,

    pub datetime: String, // Internal storage

    #[pyo3(get, set)]
    pub reference: String,
    #[pyo3(get, set)]
    pub post_only: bool,
    #[pyo3(get, set)]
    pub reduce_only: bool,
}

#[pymethods]
impl PyOrderData {
    #[new]
    #[pyo3(signature = (
        gateway_name="".into(),
        symbol="".into(),
        exchange="".into(),
        orderid="".into(),
        order_type="LIMIT".into(),
        direction="".into(),
        offset="NONE".into(),
        price=0.0,
        volume=0.0,
        traded=0.0,
        status="SUBMITTING".into(),
        datetime="".into(),
        reference="".into(),
        post_only=false,
        reduce_only=false
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: String,
        orderid: String,
        order_type: String,
        direction: String,
        offset: String,
        price: f64,
        volume: f64,
        traded: f64,
        status: String,
        datetime: String,
        reference: String,
        post_only: bool,
        reduce_only: bool,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            orderid,
            order_type,
            direction,
            offset,
            price,
            volume,
            traded,
            status,
            datetime,
            reference,
            post_only,
            reduce_only,
        }
    }

    #[getter]
    fn get_datetime<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if self.datetime.is_empty() {
            return Ok(py.None().into_pyobject(py)?.into_any());
        }
        let dt_cls = py.import("datetime")?.getattr("datetime")?;
        match dt_cls.call_method1("fromisoformat", (&self.datetime,)) {
            Ok(dt) => Ok(dt),
            Err(_) => Ok(py.None().into_pyobject(py)?.into_any()),
        }
    }

    #[setter]
    fn set_datetime(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.datetime = String::new();
        } else if let Ok(s) = value.extract::<String>() {
            self.datetime = s;
        } else {
            self.datetime = value.call_method0("isoformat")?.extract::<String>()?;
        }
        Ok(())
    }

    /// Support dict-style access: order["price"] → order.price
    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match key {
            "gateway_name" => Ok(self.gateway_name.clone().into_pyobject(py)?.into_any().unbind()),
            "symbol" => Ok(self.symbol.clone().into_pyobject(py)?.into_any().unbind()),
            "exchange" => Ok(self.exchange.clone().into_pyobject(py)?.into_any().unbind()),
            "orderid" => Ok(self.orderid.clone().into_pyobject(py)?.into_any().unbind()),
            "order_type" => Ok(self.order_type.clone().into_pyobject(py)?.into_any().unbind()),
            "direction" => Ok(self.direction.clone().into_pyobject(py)?.into_any().unbind()),
            "offset" => Ok(self.offset.clone().into_pyobject(py)?.into_any().unbind()),
            "price" => Ok(self.price.into_pyobject(py)?.into_any().unbind()),
            "volume" => Ok(self.volume.into_pyobject(py)?.into_any().unbind()),
            "traded" => Ok(self.traded.into_pyobject(py)?.into_any().unbind()),
            "status" => Ok(self.status.clone().into_pyobject(py)?.into_any().unbind()),
            "datetime" => {
                let dt = self.get_datetime(py)?;
                Ok(dt.unbind())
            }
            "reference" => Ok(self.reference.clone().into_pyobject(py)?.into_any().unbind()),
            "post_only" => {
                let borrowed = self.post_only.into_pyobject(py)?;
                let bound: Bound<'_, pyo3::types::PyBool> = borrowed.to_owned();
                Ok(bound.into_any().unbind())
            }
            "reduce_only" => {
                let borrowed = self.reduce_only.into_pyobject(py)?;
                let bound: Bound<'_, pyo3::types::PyBool> = borrowed.to_owned();
                Ok(bound.into_any().unbind())
            }
            _ => Err(pyo3::exceptions::PyKeyError::new_err(format!(
                "OrderData has no key '{}'",
                key
            ))),
        }
    }

    /// Support dict-style .get() method
    fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default_value: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.__getitem__(py, key) {
            Ok(val) => Ok(val),
            Err(_) => Ok(default_value.unwrap_or_else(|| {
                py.None().into_pyobject(py).unwrap().into_any().unbind()
            })),
        }
    }
}

impl PyOrderData {
    /// Convert a Rust OrderData into a PyOrderData
    pub fn from_rust(order: &OrderData) -> Self {
        let direction_str = order
            .direction
            .map(|d| match d {
                Direction::Long => "LONG",
                Direction::Short => "SHORT",
                Direction::Net => "NET",
            })
            .unwrap_or("")
            .to_string();

        let offset_str = match order.offset {
            Offset::None => "NONE",
            Offset::Open => "OPEN",
            Offset::Close => "CLOSE",
            Offset::CloseToday => "CLOSETODAY",
            Offset::CloseYesterday => "CLOSEYESTERDAY",
        };

        let status_str = match order.status {
            Status::Submitting => "SUBMITTING",
            Status::NotTraded => "NOTTRADED",
            Status::PartTraded => "PARTTRADED",
            Status::AllTraded => "ALLTRADED",
            Status::Cancelled => "CANCELLED",
            Status::Rejected => "REJECTED",
        };

        let order_type_str = match order.order_type {
            OrderType::Limit => "LIMIT",
            OrderType::Market => "MARKET",
            OrderType::Stop => "STOP",
            OrderType::StopLimit => "STOP_LIMIT",
            OrderType::Fak => "FAK",
            OrderType::Fok => "FOK",
            OrderType::Rfq => "RFQ",
            OrderType::Etf => "ETF",
        };

        Self {
            gateway_name: order.gateway_name.clone(),
            symbol: order.symbol.clone(),
            exchange: order.exchange.value().to_string(),
            orderid: order.orderid.clone(),
            order_type: order_type_str.to_string(),
            direction: direction_str,
            offset: offset_str.to_string(),
            price: order.price,
            volume: order.volume,
            traded: order.traded,
            status: status_str.to_string(),
            datetime: order
                .datetime
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            reference: order.reference.clone(),
            post_only: order.post_only,
            reduce_only: order.reduce_only,
        }
    }

    pub fn to_rust(&self) -> PyResult<OrderData> {
        let exchange = parse_exchange(&self.exchange);

        let direction = if self.direction.is_empty() {
            None
        } else {
            match self.direction.to_uppercase().as_str() {
                "LONG" | "多" => Some(Direction::Long),
                "SHORT" | "空" => Some(Direction::Short),
                "NET" | "净" => Some(Direction::Net),
                "NONE" | "" => None,
                _ => None,
            }
        };

        let offset = match self.offset.to_uppercase().as_str() {
            "OPEN" | "开" => Offset::Open,
            "CLOSE" | "平" => Offset::Close,
            "CLOSETODAY" | "平今" => Offset::CloseToday,
            "CLOSEYESTERDAY" | "平昨" => Offset::CloseYesterday,
            _ => Offset::None,
        };

        let status = match self.status.to_uppercase().as_str() {
            "SUBMITTING" | "提交中" => Status::Submitting,
            "NOTTRADED" | "未成交" => Status::NotTraded,
            "PARTTRADED" | "部分成交" => Status::PartTraded,
            "ALLTRADED" | "全部成交" => Status::AllTraded,
            "CANCELLED" | "已撤销" => Status::Cancelled,
            "REJECTED" | "拒单" => Status::Rejected,
            _ => Status::Submitting,
        };

        let order_type = match self.order_type.to_uppercase().as_str() {
            "MARKET" => OrderType::Market,
            "LIMIT" => OrderType::Limit,
            "STOP" => OrderType::Stop,
            "STOP_LIMIT" => OrderType::StopLimit,
            "FAK" => OrderType::Fak,
            "FOK" => OrderType::Fok,
            "RFQ" => OrderType::Rfq,
            "ETF" => OrderType::Etf,
            _ => OrderType::Limit,
        };

        let datetime = if self.datetime.is_empty() {
            None
        } else {
            chrono::DateTime::parse_from_rfc3339(&self.datetime)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        };

        Ok(OrderData {
            gateway_name: self.gateway_name.clone(),
            symbol: self.symbol.clone(),
            exchange,
            orderid: self.orderid.clone(),
            order_type,
            direction,
            offset,
            price: self.price,
            volume: self.volume,
            traded: self.traded,
            status,
            datetime,
            reference: self.reference.clone(),
            post_only: self.post_only,
            reduce_only: self.reduce_only,
            extra: None,
        })
    }
}

// ---------------------------------------------------------------------------
// PyTradeData
// ---------------------------------------------------------------------------

/// Python wrapper for TradeData
#[pyclass]
#[derive(Clone)]
pub struct PyTradeData {
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: String,
    #[pyo3(get, set)]
    pub orderid: String,
    #[pyo3(get, set)]
    pub tradeid: String,
    #[pyo3(get, set)]
    pub direction: String,
    #[pyo3(get, set)]
    pub offset: String,
    #[pyo3(get, set)]
    pub price: f64,
    #[pyo3(get, set)]
    pub volume: f64,

    pub datetime: String, // Internal storage
}

#[pymethods]
impl PyTradeData {
    #[new]
    #[pyo3(signature = (
        gateway_name="".into(),
        symbol="".into(),
        exchange="".into(),
        orderid="".into(),
        tradeid="".into(),
        direction="".into(),
        offset="NONE".into(),
        price=0.0,
        volume=0.0,
        datetime="".into()
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: String,
        orderid: String,
        tradeid: String,
        direction: String,
        offset: String,
        price: f64,
        volume: f64,
        datetime: String,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            orderid,
            tradeid,
            direction,
            offset,
            price,
            volume,
            datetime,
        }
    }

    #[getter]
    fn get_datetime<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if self.datetime.is_empty() {
            return Ok(py.None().into_pyobject(py)?.into_any());
        }
        let dt_cls = py.import("datetime")?.getattr("datetime")?;
        match dt_cls.call_method1("fromisoformat", (&self.datetime,)) {
            Ok(dt) => Ok(dt),
            Err(_) => Ok(py.None().into_pyobject(py)?.into_any()),
        }
    }

    #[setter]
    fn set_datetime(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.datetime = String::new();
        } else if let Ok(s) = value.extract::<String>() {
            self.datetime = s;
        } else {
            self.datetime = value.call_method0("isoformat")?.extract::<String>()?;
        }
        Ok(())
    }

    /// Support dict-style access: trade["price"] → trade.price
    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match key {
            "gateway_name" => Ok(self.gateway_name.clone().into_pyobject(py)?.into_any().unbind()),
            "symbol" => Ok(self.symbol.clone().into_pyobject(py)?.into_any().unbind()),
            "exchange" => Ok(self.exchange.clone().into_pyobject(py)?.into_any().unbind()),
            "orderid" => Ok(self.orderid.clone().into_pyobject(py)?.into_any().unbind()),
            "tradeid" => Ok(self.tradeid.clone().into_pyobject(py)?.into_any().unbind()),
            "direction" => Ok(self.direction.clone().into_pyobject(py)?.into_any().unbind()),
            "offset" => Ok(self.offset.clone().into_pyobject(py)?.into_any().unbind()),
            "price" => Ok(self.price.into_pyobject(py)?.into_any().unbind()),
            "volume" => Ok(self.volume.into_pyobject(py)?.into_any().unbind()),
            "datetime" => {
                let dt = self.get_datetime(py)?;
                Ok(dt.unbind())
            }
            _ => Err(pyo3::exceptions::PyKeyError::new_err(format!(
                "TradeData has no key '{}'",
                key
            ))),
        }
    }

    /// Support dict-style .get() method
    fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default_value: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match self.__getitem__(py, key) {
            Ok(val) => Ok(val),
            Err(_) => Ok(default_value.unwrap_or_else(|| {
                py.None().into_pyobject(py).unwrap().into_any().unbind()
            })),
        }
    }
}

impl PyTradeData {
    /// Convert a Rust TradeData into a PyTradeData
    pub fn from_rust(trade: &TradeData) -> Self {
        let direction_str = trade
            .direction
            .map(|d| match d {
                Direction::Long => "LONG",
                Direction::Short => "SHORT",
                Direction::Net => "NET",
            })
            .unwrap_or("")
            .to_string();

        let offset_str = match trade.offset {
            Offset::None => "NONE",
            Offset::Open => "OPEN",
            Offset::Close => "CLOSE",
            Offset::CloseToday => "CLOSETODAY",
            Offset::CloseYesterday => "CLOSEYESTERDAY",
        };

        Self {
            gateway_name: trade.gateway_name.clone(),
            symbol: trade.symbol.clone(),
            exchange: trade.exchange.value().to_string(),
            orderid: trade.orderid.clone(),
            tradeid: trade.tradeid.clone(),
            direction: direction_str,
            offset: offset_str.to_string(),
            price: trade.price,
            volume: trade.volume,
            datetime: trade
                .datetime
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
        }
    }

    pub fn to_rust(&self) -> PyResult<TradeData> {
        let exchange = parse_exchange(&self.exchange);

        let direction = if self.direction.is_empty() {
            None
        } else {
            match self.direction.to_uppercase().as_str() {
                "LONG" | "多" => Some(Direction::Long),
                "SHORT" | "空" => Some(Direction::Short),
                "NET" | "净" => Some(Direction::Net),
                "NONE" | "" => None,
                _ => None,
            }
        };

        let offset = match self.offset.to_uppercase().as_str() {
            "OPEN" | "开" => Offset::Open,
            "CLOSE" | "平" => Offset::Close,
            "CLOSETODAY" | "平今" => Offset::CloseToday,
            "CLOSEYESTERDAY" | "平昨" => Offset::CloseYesterday,
            _ => Offset::None,
        };

        let datetime = if self.datetime.is_empty() {
            None
        } else {
            chrono::DateTime::parse_from_rfc3339(&self.datetime)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        };

        Ok(TradeData {
            gateway_name: self.gateway_name.clone(),
            symbol: self.symbol.clone(),
            exchange,
            orderid: self.orderid.clone(),
            tradeid: self.tradeid.clone(),
            direction,
            offset,
            price: self.price,
            volume: self.volume,
            datetime,
            extra: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Parse an exchange string into the Exchange enum.
fn parse_exchange(s: &str) -> Exchange {
    match s.to_uppercase().as_str() {
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
        "BINANCE_USDM" | "BINANCEUSDM" => Exchange::BinanceUsdm,
        "BINANCE_COINM" | "BINANCECOINM" => Exchange::BinanceCoinm,
        "OKX" => Exchange::Okx,
        "BYBIT" => Exchange::Bybit,
        "LOCAL" => Exchange::Local,
        _ => Exchange::Global,
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register all data type classes with the parent module
pub fn register_data_types_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTickData>()?;
    m.add_class::<PyOrderData>()?;
    m.add_class::<PyTradeData>()?;
    Ok(())
}
