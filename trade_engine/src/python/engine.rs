//! Python engine for managing Python strategies
//! Handles the execution and communication between Rust and Python

use pyo3::prelude::*;
use std::collections::HashMap;
use crate::trader::{TickData, BarData, TradeData, OrderData, MainEngine, SubscribeRequest, Exchange};
use crate::python::strategy::PythonStrategy;
use crate::python::data_converter;

#[pyclass]
pub struct PythonEngine {
    main_engine: MainEngine,
    strategies: HashMap<String, Py<PythonStrategy>>,
    symbol_strategy_map: HashMap<String, String>,  // symbol -> strategy_name
}

impl PythonEngine {
    pub fn new(main_engine: MainEngine) -> Self {
        PythonEngine {
            main_engine,
            strategies: HashMap::new(),
            symbol_strategy_map: HashMap::new(),
        }
    }
    
    pub fn add_strategy(&mut self, py: Python, strategy: Bound<'_, PythonStrategy>) -> PyResult<()> {
        let strategy_ref = strategy.borrow();
        let strategy_name = strategy_ref.strategy_name.clone();
        let vt_symbols = strategy_ref.vt_symbols.clone();
        drop(strategy_ref);

        // Set the engine reference in the strategy
        strategy.borrow_mut().engine = Some(py.None());

        // Store the strategy
        self.strategies.insert(strategy_name.clone(), strategy.clone().unbind());

        // Map symbols to strategy
        for symbol in vt_symbols.iter() {
            self.symbol_strategy_map.insert(symbol.clone(), strategy_name.clone());
        }

        // Subscribe to market data for the strategy symbols
        for vt_symbol in vt_symbols.iter() {
            let parts: Vec<&str> = vt_symbol.split('.').collect();
            if parts.len() >= 2 {
                let symbol = parts[0].to_string();
                let _exchange = Exchange::Binance; // This would need proper implementation

                let _req = SubscribeRequest {
                    symbol,
                    exchange: Exchange::Binance,
                };

                // Subscribe to the symbol
                // self.main_engine.subscribe(req, &exchange.to_string()).await;
            }
        }

        Ok(())
    }
    
    pub fn init_strategy(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        if let Some(strategy_obj) = self.strategies.get(strategy_name) {
            let strategy = strategy_obj.bind(py);
            strategy.borrow().on_init(py)?;
        }
        Ok(())
    }
    
    pub fn start_strategy(&self, _py: Python, strategy_name: &str) -> PyResult<()> {
        // In a real implementation, this would start the strategy
        println!("Starting strategy: {}", strategy_name);
        Ok(())
    }
    
    pub fn stop_strategy(&self, py: Python, strategy_name: &str) -> PyResult<()> {
        if let Some(strategy_obj) = self.strategies.get(strategy_name) {
            let strategy = strategy_obj.bind(py);
            strategy.borrow().on_stop(py)?;
        }
        Ok(())
    }

    pub fn on_tick(&self, py: Python, tick: &TickData) -> PyResult<()> {
        let vt_symbol = format!("{}.{}", tick.symbol, "BINANCE"); // Simplified

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let strategy = strategy_obj.bind(py);
                let tick_py = data_converter::tick_to_py(py, tick)?;
                strategy.borrow().on_tick(py, tick_py.unbind().into())?;
            }
        }

        Ok(())
    }

    pub fn on_bar(&self, py: Python, bar: &BarData) -> PyResult<()> {
        let vt_symbol = format!("{}.{}", bar.symbol, "BINANCE"); // Simplified

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let strategy = strategy_obj.bind(py);
                // Create a dict with the bar data
                let bar_dict = pyo3::types::PyDict::new(py);
                bar_dict.set_item("datetime", bar.datetime.to_rfc3339())?;
                bar_dict.set_item("open", bar.open_price)?;
                bar_dict.set_item("high", bar.high_price)?;
                bar_dict.set_item("low", bar.low_price)?;
                bar_dict.set_item("close", bar.close_price)?;
                bar_dict.set_item("volume", bar.volume)?;

                let bars_dict = pyo3::types::PyDict::new(py);
                bars_dict.set_item(&vt_symbol, bar_dict)?;

                strategy.borrow().on_bars(py, bars_dict.unbind().into())?;
            }
        }

        Ok(())
    }
    
    pub fn on_trade(&self, py: Python, trade: &TradeData) -> PyResult<()> {
        let vt_symbol = trade.vt_symbol();

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let strategy = strategy_obj.bind(py);
                // Convert trade to Python object
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

                strategy.borrow().on_trade(py, trade_dict.unbind().into())?;
            }
        }

        Ok(())
    }
    
    pub fn on_order(&self, py: Python, order: &OrderData) -> PyResult<()> {
        let vt_symbol = order.vt_symbol();

        if let Some(strategy_name) = self.symbol_strategy_map.get(&vt_symbol) {
            if let Some(strategy_obj) = self.strategies.get(strategy_name) {
                let strategy = strategy_obj.bind(py);
                // Convert order to Python object
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

                strategy.borrow().on_order(py, order_dict.unbind().into())?;
            }
        }

        Ok(())
    }
    
    // Methods to be called from Python strategies
    pub fn buy(&self, _vt_symbol: &str, _price: f64, _volume: f64) -> Vec<String> {
        // In a real implementation, this would send the order through main_engine
        vec![format!("ORDER_{}", uuid::Uuid::new_v4())]
    }
    
    pub fn sell(&self, _vt_symbol: &str, _price: f64, _volume: f64) -> Vec<String> {
        // In a real implementation, this would send the order through main_engine
        vec![format!("ORDER_{}", uuid::Uuid::new_v4())]
    }
    
    pub fn short(&self, _vt_symbol: &str, _price: f64, _volume: f64) -> Vec<String> {
        // In a real implementation, this would send the order through main_engine
        vec![format!("ORDER_{}", uuid::Uuid::new_v4())]
    }
    
    pub fn cover(&self, _vt_symbol: &str, _price: f64, _volume: f64) -> Vec<String> {
        // In a real implementation, this would send the order through main_engine
        vec![format!("ORDER_{}", uuid::Uuid::new_v4())]
    }
    
    pub fn cancel_order(&self, vt_orderid: &str) {
        // In a real implementation, this would cancel the order through main_engine
        println!("Canceling order: {}", vt_orderid);
    }
    
    pub fn get_pos(&self, _vt_symbol: &str) -> f64 {
        // In a real implementation, this would get the position from main_engine
        0.0
    }
    
    pub fn send_email(&self, msg: &str) {
        println!("Email sent: {}", msg);
    }
    
    pub fn write_log(&self, msg: &str) {
        println!("Log: {}", msg);
    }
}

/// Initialize the Python module
#[pymethods]
impl PythonEngine {
    #[new]
    fn new_py(_main_engine: Py<PyAny>) -> Self {
        // For the Python interface, we'll create a minimal engine
        PythonEngine {
            main_engine: MainEngine::new(), // This would need proper initialization
            strategies: HashMap::new(),
            symbol_strategy_map: HashMap::new(),
        }
    }
    
    pub fn add_strategy_py(&mut self, py: Python, strategy: Bound<'_, PythonStrategy>) -> PyResult<()> {
        self.add_strategy(py, strategy)
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