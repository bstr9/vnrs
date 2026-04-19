//! PyO3 bindings for SynchronizedBarGenerator.
//!
//! Exposes the multi-symbol bar synchronizer to Python strategies.

use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::trader::object::BarData;
use crate::trader::{SynchronizedBarGenerator, SynchronizedBars};

// ---------------------------------------------------------------------------
// PySynchronizedBars — Python-facing synchronized bar batch
// ---------------------------------------------------------------------------

/// A synchronized batch of bars sharing the same timestamp.
///
/// ```python
/// sync = generator.update_bar("BTCUSDT.BINANCE", bar)
/// if sync is not None:
///     btc_bar = sync.get_bar("BTCUSDT.BINANCE")
///     print(sync.datetime, sync.symbols)
/// ```
#[pyclass(name = "SyncBarEvent")]
#[derive(Clone)]
pub struct PySynchronizedBars {
    datetime: String,
    bars: Vec<(String, PyBarInfo)>,
}

#[pymethods]
impl PySynchronizedBars {
    /// Shared timestamp of all bars in this batch (RFC 3339).
    #[getter]
    fn datetime(&self) -> &str {
        &self.datetime
    }

    /// List of vt_symbols present in this synchronized batch.
    #[getter]
    fn symbols(&self) -> Vec<String> {
        self.bars.iter().map(|(s, _)| s.clone()).collect()
    }

    /// Get bar data for a specific vt_symbol as a dict, or None.
    fn get_bar(&self, vt_symbol: &str, py: Python) -> Option<Py<PyDict>> {
        self.bars
            .iter()
            .find(|(s, _)| s == vt_symbol)
            .map(|(_, bar)| bar.to_py_dict(py))
    }

    /// All bars as a dict of vt_symbol -> bar dict.
    fn to_dict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        for (sym, bar) in &self.bars {
            dict.set_item(sym, bar.to_py_dict(py))?;
        }
        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "SyncBarEvent(datetime='{}', symbols={:?})",
            self.datetime,
            self.symbols()
        )
    }
}

impl PySynchronizedBars {
    pub fn from_rust(sync: SynchronizedBars, _py: Python) -> Self {
        let mut bars = Vec::new();
        for (sym, bar) in &sync.bars {
            bars.push((sym.clone(), PyBarInfo::from_bar(bar)));
        }
        Self {
            datetime: sync.datetime.to_rfc3339(),
            bars,
        }
    }
}

// ---------------------------------------------------------------------------
// PyBarInfo — lightweight bar info for Python
// ---------------------------------------------------------------------------

/// Lightweight bar data holder for synchronized bar events.
#[derive(Clone)]
struct PyBarInfo {
    open_price: f64,
    high_price: f64,
    low_price: f64,
    close_price: f64,
    volume: f64,
    datetime: String,
}

impl PyBarInfo {
    fn from_bar(bar: &BarData) -> Self {
        Self {
            open_price: bar.open_price,
            high_price: bar.high_price,
            low_price: bar.low_price,
            close_price: bar.close_price,
            volume: bar.volume,
            datetime: bar.datetime.to_rfc3339(),
        }
    }

    fn to_py_dict(&self, py: Python) -> Py<PyDict> {
        let dict = PyDict::new(py);
        dict.set_item("datetime", &self.datetime)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("open", self.open_price)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("high", self.high_price)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("low", self.low_price)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("close", self.close_price)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("volume", self.volume)
            .expect("setting str key in PyDict should not fail");
        dict.into()
    }
}

// ---------------------------------------------------------------------------
// PySyncBarGenerator — main PyO3 class
// ---------------------------------------------------------------------------

/// Multi-symbol bar synchronizer exposed to Python.
///
/// Collects bars from multiple symbols and emits synchronized batches
/// when all registered symbols have data for the same timestamp.
///
/// ```python
/// gen = SyncBarGenerator(["BTCUSDT.BINANCE", "ETHUSDT.BINANCE"])
///
/// result = gen.update_bar("BTCUSDT.BINANCE", bar1)  # returns None
/// result = gen.update_bar("ETHUSDT.BINANCE", bar2)  # returns SyncBarEvent
/// ```
#[pyclass(name = "SyncBarGenerator")]
pub struct PySyncBarGenerator {
    inner: Arc<Mutex<SynchronizedBarGenerator>>,
    vt_symbols: Arc<Mutex<Vec<String>>>,
}

#[pymethods]
impl PySyncBarGenerator {
    /// Create a new generator for the given list of vt_symbols.
    ///
    /// Args:
    ///     vt_symbols: List of symbol identifiers (e.g. ["BTCUSDT.BINANCE", "ETHUSDT.BINANCE"])
    #[new]
    fn new(vt_symbols: Vec<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SynchronizedBarGenerator::new(
                vt_symbols.clone(),
            ))),
            vt_symbols: Arc::new(Mutex::new(vt_symbols)),
        }
    }

    fn add_symbol(&self, vt_symbol: String) {
        let mut symbols = self.vt_symbols.lock().unwrap_or_else(|e| e.into_inner());
        if !symbols.contains(&vt_symbol) {
            symbols.push(vt_symbol);
        }
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) =
            SynchronizedBarGenerator::new(symbols.clone());
    }

    /// Feed a bar for a given vt_symbol.
    ///
    /// Returns a SyncBarEvent when all registered symbols have a bar
    /// for the same timestamp, or None if still waiting.
    ///
    /// Args:
    ///     vt_symbol: Symbol identifier (must be registered)
    ///     bar: Dict with keys: datetime, open, high, low, close, volume
    fn update_bar(
        &self,
        vt_symbol: &str,
        bar: &Bound<'_, PyDict>,
        py: Python,
    ) -> PyResult<Option<PySynchronizedBars>> {
        let rust_bar = dict_to_bar(vt_symbol, bar)?;
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let result = guard.on_bar(vt_symbol, rust_bar);
        Ok(result.map(|sync| PySynchronizedBars::from_rust(sync, py)))
    }

    /// Alias for update_bar — accepts tick-like dict data.
    fn update_tick(
        &self,
        vt_symbol: &str,
        tick: &Bound<'_, PyDict>,
        py: Python,
    ) -> PyResult<Option<PySynchronizedBars>> {
        self.update_bar(vt_symbol, tick, py)
    }

    fn get_synchronized_bars(&self) -> Option<PySynchronizedBars> {
        None
    }

    /// Number of incomplete timestamps currently buffered.
    #[getter]
    fn pending_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_count()
    }

    /// Clear all pending data.
    fn reset(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// List of registered vt_symbols.
    #[getter]
    fn vt_symbols(&self) -> Vec<String> {
        self.vt_symbols
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn __repr__(&self) -> String {
        let symbols = self.vt_symbols.lock().unwrap_or_else(|e| e.into_inner());
        let pending = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_count();
        format!(
            "SyncBarGenerator(symbols={:?}, pending={})",
            symbols.as_slice(),
            pending
        )
    }
}

// ---------------------------------------------------------------------------
// Dict -> BarData converter
// ---------------------------------------------------------------------------

fn dict_to_bar(vt_symbol: &str, dict: &Bound<'_, PyDict>) -> PyResult<BarData> {
    let parts: Vec<&str> = vt_symbol.split('.').collect();
    let symbol = parts.first().unwrap_or(&"").to_string();
    let exchange_str = parts.get(1).map(|s| s.to_uppercase()).unwrap_or_default();
    let exchange = match exchange_str.as_str() {
        "BINANCE" => crate::trader::constant::Exchange::Binance,
        "BINANCE_USDM" => crate::trader::constant::Exchange::BinanceUsdm,
        "BINANCE_COINM" => crate::trader::constant::Exchange::BinanceCoinm,
        _ => crate::trader::constant::Exchange::Local,
    };

    let datetime_str: String = dict
        .get_item("datetime")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing 'datetime' key"))?
        .extract()?;

    let datetime = chrono::DateTime::parse_from_rfc3339(&datetime_str)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid datetime: {}", e)))?
        .with_timezone(&chrono::Utc);

    Ok(BarData {
        gateway_name: String::new(),
        symbol,
        exchange,
        datetime,
        interval: Some(crate::trader::constant::Interval::Minute),
        open_price: dict
            .get_item("open")?
            .map(|v| v.extract::<f64>())
            .transpose()?
            .unwrap_or(0.0),
        high_price: dict
            .get_item("high")?
            .map(|v| v.extract::<f64>())
            .transpose()?
            .unwrap_or(0.0),
        low_price: dict
            .get_item("low")?
            .map(|v| v.extract::<f64>())
            .transpose()?
            .unwrap_or(0.0),
        close_price: dict
            .get_item("close")?
            .map(|v| v.extract::<f64>())
            .transpose()?
            .unwrap_or(0.0),
        volume: dict
            .get_item("volume")?
            .map(|v| v.extract::<f64>())
            .transpose()?
            .unwrap_or(0.0),
        turnover: 0.0,
        open_interest: 0.0,
        extra: None,
    })
}

// ---------------------------------------------------------------------------
// Registration helper (called from bindings.rs)
// ---------------------------------------------------------------------------

/// Register PySyncBarGenerator and PySynchronizedBars with the PyO3 module.
pub fn register_sync_bar_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySyncBarGenerator>()?;
    m.add_class::<PySynchronizedBars>()?;
    Ok(())
}
