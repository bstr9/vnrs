//! Python wrapper for StrategyContext
//!
//! Exposes `get_tick`, `get_bar`, `get_bars`, `load_bar` to Python strategies
//! via `self.context` on the Strategy class.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use crate::python::backtesting_bindings::PyBarData;
use crate::python::data_types::PyTickData;
use crate::strategy::template::StrategyContext;
use crate::trader::{BarData, TickData};

/// Python wrapper for StrategyContext providing market data access.
///
/// Shared `Arc<Mutex<...>>` caches allow the context to reflect updates
/// from the live StrategyEngine in real-time.
#[pyclass]
pub struct PyStrategyContext {
    tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
    bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
    historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
}

#[pymethods]
impl PyStrategyContext {
    /// Get the latest tick data for a symbol.
    ///
    /// Returns a PyTickData object if found, or None if no tick data is available.
    fn get_tick(&self, vt_symbol: String) -> Option<PyTickData> {
        self.tick_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol)
            .map(PyTickData::from_rust)
    }

    /// Get the latest bar data for a symbol.
    ///
    /// Returns a PyBarData object if found, or None if no bar data is available.
    fn get_bar(&self, vt_symbol: String) -> Option<PyBarData> {
        self.bar_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol)
            .map(PyBarData::from_rust)
    }

    /// Get the last `count` historical bars for a symbol.
    ///
    /// Returns a list of PyBarData objects. If the symbol has fewer than
    /// `count` bars cached, all available bars are returned.
    fn get_bars(&self, vt_symbol: String, count: usize) -> Vec<PyBarData> {
        let guard = self
            .historical_bars
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match guard.get(&vt_symbol) {
            Some(bars) => {
                let start = bars.len().saturating_sub(count);
                bars[start..].iter().map(PyBarData::from_rust).collect()
            }
            None => Vec::new(),
        }
    }

    /// Load historical bars for a symbol (simplified: returns from cache only).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     days: Number of days of history (used as a hint; actual data comes from cache)
    ///     interval: Bar interval string (e.g., "1m", "1h") — currently unused
    ///
    /// Returns a list of PyBarData objects from the cached historical bars.
    #[pyo3(signature = (vt_symbol, days, interval=None))]
    fn load_bar(&self, vt_symbol: String, days: i32, interval: Option<&str>) -> Vec<PyBarData> {
        // Simplified: no DB access, just return from cache.
        // Use `days` as a rough estimate of bar count (days * ~1440 minute bars).
        let estimated_count = (days as usize).saturating_mul(1440);
        let _ = interval; // currently unused
        self.get_bars(vt_symbol, estimated_count)
    }
}

impl PyStrategyContext {
    /// Create a PyStrategyContext from the shared caches of a live StrategyContext.
    /// Updates to the StrategyContext's caches (e.g., from the live StrategyEngine)
    /// are immediately visible through this wrapper.
    pub fn from_caches(
        tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
        bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
        historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
    ) -> Self {
        Self {
            tick_cache,
            bar_cache,
            historical_bars,
        }
    }

    /// Create a PyStrategyContext that shares the same underlying caches
    /// as an existing StrategyContext. Updates to the StrategyContext's caches
    /// (e.g., from the live StrategyEngine) are immediately visible.
    pub fn from_strategy_context(ctx: &StrategyContext) -> Self {
        Self {
            tick_cache: ctx.tick_cache.clone(),
            bar_cache: ctx.bar_cache.clone(),
            historical_bars: ctx.historical_bars.clone(),
        }
    }

    /// Create a PyStrategyContext with fresh empty caches.
    /// Used for backtesting where there is no live StrategyContext.
    pub fn new_empty() -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

