//! Python bindings for OffsetConverter
//!
//! Exposes the OffsetConverter's order conversion logic to Python strategies,
//! allowing them to preview how a close order would be split into
//! CloseToday / CloseYesterday legs on SHFE/INE exchanges.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use crate::trader::converter::OffsetConverter;
use crate::trader::{ContractData, Direction, Exchange, MainEngine, Offset, OrderRequest, OrderType};

// ---------------------------------------------------------------------------
// PyOrderRequest
// ---------------------------------------------------------------------------

/// Python wrapper for OrderRequest
///
/// Mirrors the fields of `crate::trader::object::OrderRequest` using
/// Python-friendly string enums (exchange, direction, offset, order_type).
#[pyclass]
#[derive(Clone)]
pub struct PyOrderRequest {
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: String,
    #[pyo3(get, set)]
    pub direction: String,
    #[pyo3(get, set)]
    pub order_type: String,
    #[pyo3(get, set)]
    pub volume: f64,
    #[pyo3(get, set)]
    pub price: f64,
    #[pyo3(get, set)]
    pub offset: String,
    #[pyo3(get, set)]
    pub reference: String,
}

#[pymethods]
impl PyOrderRequest {
    #[new]
    #[pyo3(signature = (
        symbol,
        exchange,
        direction,
        order_type="LIMIT".into(),
        volume=0.0,
        price=0.0,
        offset="NONE".into(),
        reference="".into()
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        symbol: String,
        exchange: String,
        direction: String,
        order_type: String,
        volume: f64,
        price: f64,
        offset: String,
        reference: String,
    ) -> Self {
        Self {
            symbol,
            exchange,
            direction,
            order_type,
            volume,
            price,
            offset,
            reference,
        }
    }
}

impl PyOrderRequest {
    /// Convert to Rust OrderRequest
    pub fn to_rust(&self) -> PyResult<OrderRequest> {
        let exchange = parse_exchange(&self.exchange);
        let direction = parse_direction(&self.direction)?;
        let order_type = parse_order_type(&self.order_type);
        let offset = parse_offset(&self.offset);

        Ok(OrderRequest {
            symbol: self.symbol.clone(),
            exchange,
            direction,
            order_type,
            volume: self.volume,
            price: self.price,
            offset,
            reference: self.reference.clone(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        })
    }

    /// Convert from Rust OrderRequest
    pub fn from_rust(req: &OrderRequest) -> Self {
        Self {
            symbol: req.symbol.clone(),
            exchange: req.exchange.value().to_string(),
            direction: format_direction(req.direction),
            order_type: format_order_type(req.order_type),
            volume: req.volume,
            price: req.price,
            offset: format_offset(req.offset),
            reference: req.reference.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// PyOffsetConverter
// ---------------------------------------------------------------------------

/// Python wrapper for OffsetConverter
///
/// The OffsetConverter handles position offset conversion for exchanges that
/// require explicit CloseToday / CloseYesterday legs (e.g., SHFE, INE).
///
/// Usage::
///
///     converter = OffsetConverter()
///     converter.add_contract(vt_symbol="au2312.SHFE", exchange="SHFE", ...)
///     converter.update_position(...)
///     requests = converter.convert_order_request(req, lock=False, net=False)
///
/// When a close order on SHFE/INE needs to be split, the converter returns
/// multiple OrderRequest objects with appropriate CloseToday/CloseYesterday
/// offsets and volume allocations.
#[pyclass(name = "OffsetConverter")]
pub struct PyOffsetConverter {
    /// Internal contract store — shared with the Rust OffsetConverter's
    /// contract lookup closure so Python can register contracts.
    contracts: Arc<Mutex<HashMap<String, ContractData>>>,
    inner: OffsetConverter,
}

impl PyOffsetConverter {
    /// Create a new OffsetConverter wired to the MainEngine's OmsEngine
    /// for contract lookups. This allows Python to query/preview offset
    /// conversion using live contract data without duplicating state.
    pub fn from_main_engine(main_engine: &Arc<MainEngine>) -> PyResult<Self> {
        let oms = main_engine.oms().clone();
        let get_contract: Box<dyn Fn(&str) -> Option<ContractData> + Send + Sync> =
            Box::new(move |vt_symbol: &str| oms.get_contract(vt_symbol));

        Ok(Self {
            contracts: Arc::new(Mutex::new(HashMap::new())),
            inner: OffsetConverter::new(get_contract),
        })
    }
}

#[pymethods]
impl PyOffsetConverter {
    /// Create a new OffsetConverter.
    ///
    /// Optionally seed it with an existing contract map.  If omitted, the
    /// converter starts empty and contracts are added via `add_contract()`.
    #[new]
    #[pyo3(signature = ())]
    pub fn new() -> PyResult<Self> {
        let contracts: Arc<Mutex<HashMap<String, ContractData>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let contracts_clone = contracts.clone();
        let get_contract: Box<dyn Fn(&str) -> Option<ContractData> + Send + Sync> =
            Box::new(move |vt_symbol: &str| {
                contracts_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(vt_symbol)
                    .cloned()
            });

        Ok(Self {
            contracts,
            inner: OffsetConverter::new(get_contract),
        })
    }

    /// Register a contract so the converter knows which symbols require
    /// offset conversion (i.e., `net_position == false`).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     exchange: Exchange string (e.g., "SHFE", "INE")
    ///     name: Human-readable name
    ///     product: Product type string ("FUTURES", "SPOT", etc.)
    ///     size: Contract multiplier
    ///     pricetick: Minimum price movement
    ///     net_position: Whether the contract uses net position mode
    #[pyo3(signature = (vt_symbol, exchange, name, product="FUTURES".into(), size=1.0, pricetick=0.01, net_position=false))]
    #[allow(clippy::too_many_arguments)]
    pub fn add_contract(
        &self,
        vt_symbol: String,
        exchange: String,
        name: String,
        product: String,
        size: f64,
        pricetick: f64,
        net_position: bool,
    ) -> PyResult<()> {
        let exchange_enum = parse_exchange(&exchange);
        let product_enum = parse_product(&product);

        // Parse symbol from vt_symbol (take the part before the last dot)
        let symbol = vt_symbol
            .rsplitn(2, '.')
            .last()
            .unwrap_or(&vt_symbol)
            .to_string();

        let mut contract = ContractData::new(
            String::new(), // gateway_name
            symbol,
            exchange_enum,
            name,
            product_enum,
            size,
            pricetick,
        );
        contract.net_position = net_position;

        self.contracts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(vt_symbol, contract);

        Ok(())
    }

    /// Convert an order request according to offset rules.
    ///
    /// For SHFE/INE exchanges with long-short position mode, a single close
    /// order may be split into multiple legs (CloseToday + CloseYesterday).
    /// For lock/net mode, the conversion follows the respective strategy.
    ///
    /// Args:
    ///     req: PyOrderRequest to convert
    ///     lock: Whether to use lock mode (for lock-strategy accounts)
    ///     net: Whether to use net mode (for net-position accounts)
    ///
    /// Returns:
    ///     List of PyOrderRequest objects (may be 1 if no split needed)
    #[pyo3(signature = (req, lock=false, net=false))]
    pub fn convert_order_request(
        &mut self,
        req: &PyOrderRequest,
        lock: bool,
        net: bool,
    ) -> PyResult<Vec<PyOrderRequest>> {
        let rust_req = req.to_rust()?;
        let results = self.inner.convert_order_request(&rust_req, lock, net);
        Ok(results.iter().map(PyOrderRequest::from_rust).collect())
    }

    /// Update position data for a symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG", "SHORT", or "NET"
    ///     volume: Total position volume
    ///     yd_volume: Yesterday's position volume
    #[pyo3(signature = (vt_symbol, direction, volume, yd_volume=0.0))]
    pub fn update_position(
        &mut self,
        vt_symbol: String,
        direction: String,
        volume: f64,
        yd_volume: f64,
    ) -> PyResult<()> {
        let direction_enum = parse_direction(&direction)?;
        let exchange = self
            .contracts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol)
            .map(|c| c.exchange)
            .unwrap_or(Exchange::Binance);

        let symbol = vt_symbol
            .rsplitn(2, '.')
            .last()
            .unwrap_or(&vt_symbol)
            .to_string();

        let mut position = crate::trader::PositionData::new(
            String::new(), // gateway_name
            symbol,
            exchange,
            direction_enum,
        );
        position.volume = volume;
        position.yd_volume = yd_volume;

        self.inner.update_position(&position);
        Ok(())
    }

    /// Update trade data for a symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     offset: "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY"
    ///     price: Trade price
    ///     volume: Trade volume
    #[pyo3(signature = (vt_symbol, direction, offset, price, volume))]
    pub fn update_trade(
        &mut self,
        vt_symbol: String,
        direction: String,
        offset: String,
        price: f64,
        volume: f64,
    ) -> PyResult<()> {
        let direction_enum = parse_direction(&direction)?;
        let offset_enum = parse_offset(&offset);
        let exchange = self
            .contracts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol)
            .map(|c| c.exchange)
            .unwrap_or(Exchange::Binance);

        let symbol = vt_symbol
            .rsplitn(2, '.')
            .last()
            .unwrap_or(&vt_symbol)
            .to_string();

        let trade = crate::trader::TradeData {
            gateway_name: String::new(),
            symbol,
            exchange,
            orderid: String::new(),
            tradeid: String::new(),
            direction: Some(direction_enum),
            offset: offset_enum,
            price,
            volume,
            datetime: None,
            extra: None,
        };

        self.inner.update_trade(&trade);
        Ok(())
    }

    /// Check if a symbol requires offset conversion (i.e., uses long-short mode).
    ///
    /// Returns True for futures on SHFE/INE exchanges that require explicit
    /// CloseToday/CloseYesterday splitting, False otherwise.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///
    /// Returns:
    ///     True if the contract requires offset conversion, False otherwise
    pub fn is_split_required(&self, vt_symbol: &str) -> bool {
        self.inner.is_convert_required(vt_symbol)
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers (shared with data_types.rs patterns)
// ---------------------------------------------------------------------------

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

fn parse_direction(s: &str) -> PyResult<Direction> {
    match s.to_uppercase().as_str() {
        "LONG" | "BUY" | "多" => Ok(Direction::Long),
        "SHORT" | "SELL" | "空" => Ok(Direction::Short),
        "NET" | "净" => Ok(Direction::Net),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid direction '{}'",
            s
        ))),
    }
}

fn parse_offset(s: &str) -> Offset {
    match s.to_uppercase().as_str() {
        "OPEN" | "开" => Offset::Open,
        "CLOSE" | "平" => Offset::Close,
        "CLOSETODAY" | "CLOSE_TODAY" | "平今" => Offset::CloseToday,
        "CLOSEYESTERDAY" | "CLOSE_YESTERDAY" | "平昨" => Offset::CloseYesterday,
        _ => Offset::None,
    }
}

fn parse_order_type(s: &str) -> OrderType {
    match s.to_uppercase().as_str() {
        "MARKET" => OrderType::Market,
        "LIMIT" => OrderType::Limit,
        "STOP" => OrderType::Stop,
        "STOP_LIMIT" => OrderType::StopLimit,
        "FAK" => OrderType::Fak,
        "FOK" => OrderType::Fok,
        _ => OrderType::Limit,
    }
}

fn parse_product(s: &str) -> crate::trader::Product {
    match s.to_uppercase().as_str() {
        "FUTURES" | "期货" => crate::trader::Product::Futures,
        "SPOT" | "现货" => crate::trader::Product::Spot,
        "OPTION" | "期权" => crate::trader::Product::Option,
        "ETF" => crate::trader::Product::Etf,
        "INDEX" | "指数" => crate::trader::Product::Index,
        "BOND" | "债券" => crate::trader::Product::Bond,
        _ => crate::trader::Product::Spot,
    }
}

fn format_direction(d: Direction) -> String {
    match d {
        Direction::Long => "LONG".to_string(),
        Direction::Short => "SHORT".to_string(),
        Direction::Net => "NET".to_string(),
    }
    .to_string()
}

fn format_order_type(t: OrderType) -> String {
    match t {
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        OrderType::Stop => "STOP",
        OrderType::StopLimit => "STOP_LIMIT",
        OrderType::Fak => "FAK",
        OrderType::Fok => "FOK",
        OrderType::Rfq => "RFQ",
        OrderType::Etf => "ETF",
        OrderType::Gtd => "GTD",
        OrderType::PeggedBest => "PEGGED_BEST",
    }
    .to_string()
}

fn format_offset(o: Offset) -> String {
    match o {
        Offset::None => "NONE",
        Offset::Open => "OPEN",
        Offset::Close => "CLOSE",
        Offset::CloseToday => "CLOSETODAY",
        Offset::CloseYesterday => "CLOSEYESTERDAY",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register offset converter classes with the parent module
pub fn register_offset_converter_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyOffsetConverter>()?;
    m.add_class::<PyOrderRequest>()?;
    Ok(())
}
