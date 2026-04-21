//! Python engine for managing Python strategies
//! Handles the execution and communication between Rust and Python

use crate::python::data_converter;
use crate::python::strategy::Strategy;
use crate::python::strategy_adapter::PythonStrategyAdapter;
use crate::python::{MessageBus, OrderFactory, PortfolioFacade, PortfolioState, PyStrategyContext};
use crate::strategy::{StrategyEngine, StrategySetting};
use crate::trader::{
    BarData, BaseEngine, CancelRequest, Direction, Exchange, GatewayEvent, MainEngine, Offset,
    OrderData, OrderType, OrderRequest, SubscribeRequest, TickData, TradeData, EVENT_BAR,
    EVENT_TICK, EVENT_ORDER, EVENT_TRADE,
};
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::trader::utility::extract_vt_symbol;

#[pyclass]
pub struct PythonEngine {
    main_engine: Arc<MainEngine>,
    /// Optional reference to the live StrategyEngine for routing Python strategies
    strategy_engine: Option<Arc<StrategyEngine>>,
    strategies: HashMap<String, Py<Strategy>>,
    symbol_strategy_map: HashMap<String, String>,
    portfolio_state: Arc<Mutex<PortfolioState>>,
}

impl PythonEngine {
    pub fn new(main_engine: MainEngine) -> Self {
        PythonEngine {
            main_engine: Arc::new(main_engine),
            strategy_engine: None,
            strategies: HashMap::new(),
            symbol_strategy_map: HashMap::new(),
            portfolio_state: Arc::new(Mutex::new(PortfolioState::default())),
        }
    }

    /// Create a PythonEngine from an already-shared MainEngine reference.
    /// This allows the engine to share the same MainEngine instance with
    /// PythonEngineBridge registered on that MainEngine.
    pub fn new_from_arc(main_engine: Arc<MainEngine>) -> Self {
        PythonEngine {
            main_engine,
            strategy_engine: None,
            strategies: HashMap::new(),
            symbol_strategy_map: HashMap::new(),
            portfolio_state: Arc::new(Mutex::new(PortfolioState::default())),
        }
    }

    /// Set the live StrategyEngine reference so that Python strategies
    /// added via `add_strategy` are also registered for live market data
    pub fn set_strategy_engine(&mut self, engine: Arc<StrategyEngine>) {
        self.strategy_engine = Some(engine);
    }

    /// Get a reference to the live StrategyEngine, if set.
    pub fn get_strategy_engine(&self) -> Option<&Arc<StrategyEngine>> {
        self.strategy_engine.as_ref()
    }

    pub fn add_strategy(
        &mut self,
        py: Python,
        strategy: Bound<'_, Strategy>,
        engine_ref: Py<PyAny>,
    ) -> PyResult<()> {
        let strategy_ref = strategy.borrow();
        let strategy_name = strategy_ref.strategy_name.clone();
        let vt_symbols = strategy_ref.vt_symbols.clone();
        drop(strategy_ref);

        // Set engine reference
        strategy.borrow_mut().engine = Some(engine_ref.clone_ref(py));

        // Create and inject PortfolioFacade
        let portfolio_facade = PortfolioFacade::from_state(self.portfolio_state.clone());
        let portfolio_py = Py::new(py, portfolio_facade)?;
        strategy.borrow_mut().portfolio = Some(portfolio_py);

        // Create and inject OrderFactory
        let order_factory = OrderFactory::from_engine(engine_ref, "");
        let factory_py = Py::new(py, order_factory)?;
        strategy.borrow_mut().order_factory = Some(factory_py);

        // Create and inject MessageBus (wrapping the MainEngine's Rust MessageBus)
        let message_bus = MessageBus::from_rust_message_bus(self.main_engine.get_message_bus().clone());
        let bus_py = Py::new(py, message_bus)?;
        strategy.borrow_mut().message_bus = Some(bus_py);

        // Create and inject PyStrategyContext
        // For live trading with a StrategyEngine, the context will be updated
        // by add_strategy_live() which shares the engine's caches.
        // For non-StrategyEngine paths, this provides an empty context.
        let context = PyStrategyContext::new_empty();
        let context_py = Py::new(py, context)?;
        strategy.borrow_mut().context = Some(context_py);

        self.strategies
            .insert(strategy_name.clone(), strategy.clone().unbind());

        for symbol in vt_symbols.iter() {
            self.symbol_strategy_map
                .insert(symbol.clone(), strategy_name.clone());
        }

        // If a live StrategyEngine is set, also register the PythonStrategyAdapter
        // so the strategy receives live market data events through the StrategyEngine
        if let Some(ref strat_engine) = self.strategy_engine {
            let py_obj: Py<PyAny> = strategy.clone().unbind().into_any();
            let adapter = PythonStrategyAdapter::from_py_object(
                py_obj,
                strategy_name.clone(),
                vt_symbols.clone(),
            );
            let strat_engine = strat_engine.clone();
            let sn = strategy_name.clone();
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(async move {
                        let setting = StrategySetting::new();
                        if let Err(e) = strat_engine.add_python_strategy(adapter, setting).await {
                            tracing::error!(
                                "Failed to add Python strategy '{}' to live StrategyEngine: {}",
                                sn, e
                            );
                        }
                    });
                }
                Err(_) => {
                    tracing::warn!(
                        "No tokio runtime available to register Python strategy '{}' with StrategyEngine",
                        strategy_name
                    );
                }
            }
        } else {
            // No live StrategyEngine — subscribe directly through MainEngine
            for vt_symbol in vt_symbols.iter() {
                let exchange = crate::trader::utility::extract_vt_symbol(vt_symbol)
                    .map(|(_, e)| e)
                    .unwrap_or(Exchange::Binance);
                let symbol = vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string();
                let req = SubscribeRequest { symbol, exchange };
                if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(exchange) {
                    let engine = self.main_engine.clone();
                    let gw = gw_name.clone();
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(async {
                            if let Err(e) = engine.subscribe(req, &gw).await {
                                tracing::warn!("Python strategy subscribe failed: {}", e);
                            }
                        });
                    });
                }
            }
        }

        Ok(())
    }

    pub fn init_strategy(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        if let Some(strategy_obj) = self.strategies.get(strategy_name) {
            // Call on_init via Python method dispatch (supports subclass overrides)
            strategy_obj.call_method0(py, "on_init")?;
            strategy_obj.bind(py).borrow_mut().set_inited();
        }
        Ok(())
    }

    pub fn start_strategy(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        if let Some(strategy_obj) = self.strategies.get(strategy_name) {
            // Call on_start via Python method dispatch (supports subclass overrides)
            strategy_obj.call_method0(py, "on_start")?;
            strategy_obj.bind(py).borrow_mut().set_trading();
            tracing::info!("Started strategy: {}", strategy_name);
        }
        Ok(())
    }

    pub fn stop_strategy(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        if let Some(strategy_obj) = self.strategies.get(strategy_name) {
            // Call on_stop via Python method dispatch (supports subclass overrides)
            strategy_obj.call_method0(py, "on_stop")?;
            strategy_obj.bind(py).borrow_mut().set_stopped();
        }
        Ok(())
    }

    pub fn on_tick(&self, py: Python, tick: &TickData) -> PyResult<()> {
        let vt_symbol = format!("{}.{}", tick.symbol, tick.exchange.value());

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let tick_py = data_converter::tick_to_py(py, tick)?;
                // Call on_tick via Python method dispatch
                strategy_obj.call_method1(py, "on_tick", (tick_py.into_any(),))?;
            }
        }

        Ok(())
    }

    pub fn on_bar(&self, py: Python, bar: &BarData) -> PyResult<()> {
        let vt_symbol = format!("{}.{}", bar.symbol, bar.exchange.value());

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let bar_dict = pyo3::types::PyDict::new(py);
                bar_dict.set_item("datetime", bar.datetime.to_rfc3339())?;
                bar_dict.set_item("open", bar.open_price)?;
                bar_dict.set_item("high", bar.high_price)?;
                bar_dict.set_item("low", bar.low_price)?;
                bar_dict.set_item("close", bar.close_price)?;
                bar_dict.set_item("volume", bar.volume)?;

                let bars_dict = pyo3::types::PyDict::new(py);
                bars_dict.set_item(&vt_symbol, bar_dict)?;

                // Call on_bars via Python method dispatch
                strategy_obj.call_method1(py, "on_bars", (bars_dict,))?;
            }
        }

        Ok(())
    }

    pub fn on_trade(&self, py: Python, trade: &TradeData) -> PyResult<()> {
        let vt_symbol = trade.vt_symbol();

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let trade_dict = pyo3::types::PyDict::new(py);
                trade_dict.set_item("symbol", &trade.symbol)?;
                trade_dict.set_item("exchange", format!("{:?}", trade.exchange))?;
                trade_dict.set_item("orderid", &trade.orderid)?;
                trade_dict.set_item("tradeid", &trade.tradeid)?;
                trade_dict.set_item("direction", format!("{:?}", trade.direction))?;
                trade_dict.set_item("offset", format!("{:?}", trade.offset))?;
                trade_dict.set_item("price", trade.price)?;
                trade_dict.set_item("volume", trade.volume)?;
                if let Some(dt) = trade.datetime {
                    trade_dict.set_item("datetime", dt.to_rfc3339())?;
                }
                trade_dict.set_item("gateway_name", &trade.gateway_name)?;

                // Call on_trade via Python method dispatch
                strategy_obj.call_method1(py, "on_trade", (trade_dict,))?;
            }
        }

        Ok(())
    }

    pub fn on_order(&self, py: Python, order: &OrderData) -> PyResult<()> {
        let vt_symbol = order.vt_symbol();

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let order_dict = pyo3::types::PyDict::new(py);
                order_dict.set_item("symbol", &order.symbol)?;
                order_dict.set_item("exchange", format!("{:?}", order.exchange))?;
                order_dict.set_item("orderid", &order.orderid)?;
                order_dict.set_item("direction", format!("{:?}", order.direction))?;
                order_dict.set_item("offset", format!("{:?}", order.offset))?;
                order_dict.set_item("price", order.price)?;
                order_dict.set_item("volume", order.volume)?;
                order_dict.set_item("traded", order.traded)?;
                order_dict.set_item("status", format!("{:?}", order.status))?;
                if let Some(dt) = order.datetime {
                    order_dict.set_item("datetime", dt.to_rfc3339())?;
                }
                order_dict.set_item("gateway_name", &order.gateway_name)?;

                // Call on_order via Python method dispatch
                strategy_obj.call_method1(py, "on_order", (order_dict,))?;
            }
        }

        Ok(())
    }

    /// Send an order through the MainEngine
    pub fn send_order(
        &self,
        vt_symbol: &str,
        direction: Direction,
        offset: Offset,
        price: f64,
        volume: f64,
        order_type: OrderType,
    ) -> Vec<String> {
        let (symbol, exchange) = match extract_vt_symbol(vt_symbol) {
            Some((s, e)) => (s, e),
            None => {
                tracing::error!("Invalid vt_symbol format: {}", vt_symbol);
                return Vec::new();
            }
        };

        let req = OrderRequest {
            symbol,
            exchange,
            direction,
            order_type,
            volume,
            price,
            offset,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        };

        let gateway_name = match exchange {
            Exchange::Binance | Exchange::BinanceUsdm | Exchange::BinanceCoinm => {
                let gateways = self.main_engine.get_all_gateway_names();
                if gateways.contains(&"BINANCE_SPOT".to_string()) {
                    "BINANCE_SPOT".to_string()
                } else if gateways.contains(&"BINANCE_USDT".to_string()) {
                    "BINANCE_USDT".to_string()
                } else {
                    tracing::warn!("No Binance gateway available for order");
                    return Vec::new();
                }
            }
            _ => {
                tracing::warn!("Unsupported exchange for order: {:?}", exchange);
                return Vec::new();
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let me = self.main_engine.clone();
                let gw = gateway_name;
                match handle.block_on(async { me.send_order(req, &gw).await }) {
                    Ok(vt_orderid) => vec![vt_orderid],
                    Err(e) => {
                        tracing::error!("Failed to send order: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(_) => {
                tracing::error!("No tokio runtime available to send order");
                Vec::new()
            }
        }
    }

    pub fn buy(&self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        self.send_order(vt_symbol, Direction::Long, Offset::Open, price, volume, OrderType::Limit)
    }

    pub fn sell(&self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        self.send_order(vt_symbol, Direction::Short, Offset::Close, price, volume, OrderType::Limit)
    }

    pub fn short(&self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        self.send_order(vt_symbol, Direction::Short, Offset::Open, price, volume, OrderType::Limit)
    }

    pub fn cover(&self, vt_symbol: &str, price: f64, volume: f64) -> Vec<String> {
        self.send_order(vt_symbol, Direction::Long, Offset::Close, price, volume, OrderType::Limit)
    }

    /// Cancel an existing order through the MainEngine
    pub fn cancel_order(&self, vt_orderid: &str) {
        // vt_orderid format: "gateway_name.orderid"
        let parts: Vec<&str> = vt_orderid.splitn(2, '.').collect();
        if parts.len() != 2 {
            tracing::error!("Invalid vt_orderid format: {}", vt_orderid);
            return;
        }

        let gateway_name = parts[0].to_string();
        let orderid = parts[1].to_string();

        if let Some(order) = self.main_engine.get_order(vt_orderid) {
            let req = CancelRequest {
                orderid,
                symbol: order.symbol.clone(),
                exchange: order.exchange,
                gateway_name: String::new(),
            };

            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    let me = self.main_engine.clone();
                    let gw = gateway_name;
                    if let Err(e) = handle.block_on(async { me.cancel_order(req, &gw).await }) {
                        tracing::error!("Failed to cancel order {}: {}", vt_orderid, e);
                    }
                }
                Err(_) => {
                    tracing::error!("No tokio runtime available to cancel order");
                }
            }
        } else {
            tracing::warn!("Order {} not found for cancellation", vt_orderid);
        }
    }

    /// Get position volume for a symbol
    pub fn get_pos(&self, vt_symbol: &str) -> f64 {
        let mut total_volume: f64 = 0.0;
        for position in self.main_engine.get_all_positions() {
            if position.vt_symbol() == vt_symbol {
                total_volume += match position.direction {
                    Direction::Long => position.volume,
                    Direction::Short => -position.volume,
                    Direction::Net => position.volume,
                };
            }
        }
        total_volume
    }

    pub fn send_email(&self, msg: &str) {
        tracing::warn!("send_email called but email is not configured: {}", msg);
    }

    pub fn write_log(&self, msg: &str) {
        tracing::info!("[策略] {}", msg);
    }
}

/// Initialize the Python module
#[pymethods]
impl PythonEngine {
    #[new]
    fn new_py(_main_engine: Py<PyAny>) -> Self {
        PythonEngine {
            main_engine: MainEngine::new(),
            strategy_engine: None,
            strategies: HashMap::new(),
            symbol_strategy_map: HashMap::new(),
            portfolio_state: Arc::new(Mutex::new(PortfolioState::default())),
        }
    }

    pub fn add_strategy_py(
        &mut self,
        _py: Python,
        strategy: Bound<'_, Strategy>,
        engine_ref: Py<PyAny>,
    ) -> PyResult<()> {
        self.add_strategy(_py, strategy, engine_ref)
    }

    pub fn init_strategy_py(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        self.init_strategy(py, strategy_name)
    }

    pub fn start_strategy_py(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        self.start_strategy(py, strategy_name)
    }

    pub fn stop_strategy_py(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        self.stop_strategy(py, strategy_name)
    }
}

/// Thin wrapper around `Arc<PythonEngine>` that implements `BaseEngine`.
///
/// This allows `PythonEngine` to be registered with `MainEngine` as a sub-engine
/// for standalone usage (when no `StrategyEngine` is set). When a `StrategyEngine`
/// IS set, Python strategies receive events through the StrategyEngine's event
/// routing instead, and this wrapper's `process_event` becomes a no-op.
pub struct PythonEngineBridge {
    inner: Arc<std::sync::Mutex<PythonEngine>>,
}

impl PythonEngineBridge {
    pub fn new(engine: PythonEngine) -> Self {
        PythonEngineBridge {
            inner: Arc::new(std::sync::Mutex::new(engine)),
        }
    }

    /// Create a bridge from an already-shared PythonEngine reference.
    /// This allows the PythonEngineWrapper and the bridge to share the
    /// same PythonEngine instance, so events dispatched through MainEngine
    /// reach the same strategies that the wrapper's on_tick/on_bar/etc forward to.
    pub fn from_shared(inner: Arc<std::sync::Mutex<PythonEngine>>) -> Self {
        PythonEngineBridge { inner }
    }
}

impl BaseEngine for PythonEngineBridge {
    fn engine_name(&self) -> &str {
        "python"
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        let engine = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Only process if we don't have a StrategyEngine (standalone mode)
        // Otherwise, events flow through StrategyEngine → PythonStrategyAdapter
        if engine.strategy_engine.is_some() {
            return;
        }

        Python::attach(|py| {
            match event_type {
                t if t == EVENT_TICK || t.starts_with("eTick.") => {
                    if let GatewayEvent::Tick(tick) = event {
                        let _ = engine.on_tick(py, tick);
                    }
                }
                t if t == EVENT_BAR || t.starts_with("eBar.") => {
                    if let GatewayEvent::Bar(bar) = event {
                        let _ = engine.on_bar(py, bar);
                    }
                }
                t if t == EVENT_ORDER => {
                    if let GatewayEvent::Order(order) = event {
                        let _ = engine.on_order(py, order);
                    }
                }
                t if t == EVENT_TRADE => {
                    if let GatewayEvent::Trade(trade) = event {
                        let _ = engine.on_trade(py, trade);
                    }
                }
                _ => {}
            }
        });
    }
}
