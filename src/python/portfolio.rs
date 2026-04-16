//! PortfolioFacade — a read-only PyO3 class exposing portfolio state to Python strategies.
//!
//! The facade wraps a snapshot of engine state (balance + positions) behind
//! `Arc<Mutex<PortfolioState>>`. Engines (live or backtest) push updates via
//! the `update_*` methods; Python strategies can only read through getters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::backtesting::{calculate_statistics, DailyResult};
use crate::trader::{AccountData, Direction, PositionData};

use super::portfolio_stats::PyPortfolioStatistics;

// ---------------------------------------------------------------------------
// PositionSnapshot — internal data holder for a single position
// ---------------------------------------------------------------------------

/// Immutable snapshot of a single position held inside `PortfolioState`.
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub exchange: String,
    pub direction: String, // "LONG", "SHORT", "NET"
    pub quantity: f64,
    pub avg_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}

impl PositionSnapshot {
    /// Build the map key used to store/look up positions.
    /// Format: `vt_symbol.direction` (e.g. "BTCUSDT.BINANCE.LONG").
    fn key(vt_symbol: &str, direction: &Direction) -> String {
        let dir_str = match direction {
            Direction::Long => "LONG",
            Direction::Short => "SHORT",
            Direction::Net => "NET",
        };
        format!("{}.{}", vt_symbol, dir_str)
    }

    /// Build the map key from a vt_symbol and direction string.
    fn key_from_str(vt_symbol: &str, direction: &str) -> String {
        format!("{}.{}", vt_symbol, direction)
    }
}

// ---------------------------------------------------------------------------
// PortfolioState — mutable inner state protected by Mutex
// ---------------------------------------------------------------------------

/// Inner state of the portfolio. Engines update this; the Python-facing
/// `PortfolioFacade` only reads from it.
#[derive(Debug, Clone)]
pub struct PortfolioState {
    pub balance: f64,
    pub frozen: f64,
    /// Positions keyed by `vt_symbol.direction`.
    pub positions: HashMap<String, PositionSnapshot>,
    /// Daily results keyed by date, used for computing portfolio statistics.
    pub daily_results: HashMap<NaiveDate, DailyResult>,
    /// Starting capital (set on first balance update or explicitly).
    pub start_capital: f64,
}

impl Default for PortfolioState {
    fn default() -> Self {
        Self {
            balance: 0.0,
            frozen: 0.0,
            positions: HashMap::new(),
            daily_results: HashMap::new(),
            start_capital: 0.0,
        }
    }
}

impl PortfolioState {
    /// Total available (unfrozen) balance.
    pub fn available(&self) -> f64 {
        self.balance - self.frozen
    }

    /// Sum of unrealized PnL across all positions.
    pub fn total_unrealized_pnl(&self) -> f64 {
        self.positions.values().map(|p| p.unrealized_pnl).sum()
    }

    /// Equity = balance + total unrealized PnL.
    pub fn equity(&self) -> f64 {
        self.balance + self.total_unrealized_pnl()
    }

    /// Net signed position for a given vt_symbol (long positive, short negative).
    /// Aggregates across all direction buckets for that symbol.
    pub fn net_position(&self, vt_symbol: &str) -> f64 {
        self.positions
            .values()
            .filter(|p| {
                // Match by checking if the key starts with `vt_symbol.`
                // The key format is `vt_symbol.DIRECTION`.
                let key = PositionSnapshot::key_from_str(vt_symbol, &p.direction);
                self.positions.contains_key(&key)
                    && p.symbol == vt_symbol.split('.').next().unwrap_or(vt_symbol)
            })
            .map(|p| match p.direction.as_str() {
                "SHORT" => -p.quantity,
                _ => p.quantity,
            })
            .sum()
    }

    /// Look up a single position by vt_symbol.
    /// Returns the first matching position (preferring LONG if both exist).
    pub fn get_position(&self, vt_symbol: &str) -> Option<&PositionSnapshot> {
        // Try LONG first, then SHORT, then NET
        for dir_str in &["LONG", "SHORT", "NET"] {
            let key = PositionSnapshot::key_from_str(vt_symbol, dir_str);
            if let Some(pos) = self.positions.get(&key) {
                return Some(pos);
            }
        }
        None
    }

    /// Get all positions as a Vec.
    pub fn get_all_positions(&self) -> Vec<&PositionSnapshot> {
        self.positions.values().collect()
    }

    // ---- Update methods called by engines ----

    /// Update from a live `AccountData` push.
    pub fn update_from_account(&mut self, account: &AccountData) {
        self.balance = account.balance;
        self.frozen = account.frozen;
    }

    /// Update from a live `PositionData` push.
    pub fn update_from_position(&mut self, position: &PositionData) {
        let vt_symbol = position.vt_symbol();
        let key = PositionSnapshot::key(&vt_symbol, &position.direction);
        let direction_str = match position.direction {
            Direction::Long => "LONG",
            Direction::Short => "SHORT",
            Direction::Net => "NET",
        };

        if position.volume == 0.0 {
            // Remove flat positions to keep the map clean
            self.positions.remove(&key);
        } else {
            let snapshot = PositionSnapshot {
                symbol: position.symbol.clone(),
                exchange: format!("{}", position.exchange),
                direction: direction_str.to_string(),
                quantity: position.volume,
                avg_price: position.price,
                unrealized_pnl: position.pnl,
                realized_pnl: 0.0, // PositionData doesn't carry realized PnL
            };
            self.positions.insert(key, snapshot);
        }
    }

    /// Update balance (used by backtesting engine).
    /// On the first call when start_capital is 0, also records it as start_capital.
    pub fn update_balance(&mut self, capital: f64) {
        if self.start_capital == 0.0 && capital > 0.0 {
            self.start_capital = capital;
        }
        self.balance = capital;
    }

    /// Update or insert a position from a backtesting fill.
    pub fn update_position_fill(
        &mut self,
        vt_symbol: &str,
        direction: Direction,
        quantity: f64,
        avg_price: f64,
        realized: f64,
    ) {
        let key = PositionSnapshot::key(vt_symbol, &direction);
        let direction_str = match direction {
            Direction::Long => "LONG",
            Direction::Short => "SHORT",
            Direction::Net => "NET",
        };

        if quantity == 0.0 {
            self.positions.remove(&key);
            return;
        }

        // Split vt_symbol into symbol + exchange parts
        let (symbol, exchange) = match vt_symbol.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), e.to_string()),
            None => (vt_symbol.to_string(), String::new()),
        };

        let snapshot = PositionSnapshot {
            symbol,
            exchange,
            direction: direction_str.to_string(),
            quantity,
            avg_price,
            unrealized_pnl: 0.0, // Must be updated separately when mark price is known
            realized_pnl: realized,
        };
        self.positions.insert(key, snapshot);
    }

    /// Update unrealized PnL for a position given a mark price.
    pub fn update_mark_price(&mut self, vt_symbol: &str, direction: &Direction, mark_price: f64) {
        let key = PositionSnapshot::key(vt_symbol, direction);
        if let Some(pos) = self.positions.get_mut(&key) {
            pos.unrealized_pnl = match pos.direction.as_str() {
                "LONG" => (mark_price - pos.avg_price) * pos.quantity,
                "SHORT" => (pos.avg_price - mark_price) * pos.quantity,
                _ => 0.0,
            };
        }
    }

    /// Record or replace a daily result for the given date.
    pub fn update_daily_result(&mut self, date: NaiveDate, result: DailyResult) {
        self.daily_results.insert(date, result);
    }

    /// Set the starting capital explicitly.
    pub fn set_start_capital(&mut self, capital: f64) {
        self.start_capital = capital;
    }
}

// ---------------------------------------------------------------------------
// PyPosition — read-only Python wrapper around PositionSnapshot
// ---------------------------------------------------------------------------

/// A read-only position object exposed to Python.
///
/// ```python
/// pos = portfolio.position("BTCUSDT.BINANCE")
/// print(pos.symbol, pos.quantity, pos.unrealized_pnl)
/// ```
#[pyclass(name = "Position")]
#[derive(Clone)]
pub struct PyPosition {
    inner: PositionSnapshot,
}

#[pymethods]
impl PyPosition {
    #[getter]
    fn symbol(&self) -> &str {
        &self.inner.symbol
    }

    #[getter]
    fn exchange(&self) -> &str {
        &self.inner.exchange
    }

    #[getter]
    fn direction(&self) -> &str {
        &self.inner.direction
    }

    #[getter]
    fn quantity(&self) -> f64 {
        self.inner.quantity
    }

    #[getter]
    fn avg_price(&self) -> f64 {
        self.inner.avg_price
    }

    #[getter]
    fn unrealized_pnl(&self) -> f64 {
        self.inner.unrealized_pnl
    }

    #[getter]
    fn realized_pnl(&self) -> f64 {
        self.inner.realized_pnl
    }

    /// Convert to a Python dict for convenient access.
    fn to_dict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("symbol", &self.inner.symbol)?;
        dict.set_item("exchange", &self.inner.exchange)?;
        dict.set_item("direction", &self.inner.direction)?;
        dict.set_item("quantity", self.inner.quantity)?;
        dict.set_item("avg_price", self.inner.avg_price)?;
        dict.set_item("unrealized_pnl", self.inner.unrealized_pnl)?;
        dict.set_item("realized_pnl", self.inner.realized_pnl)?;
        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "Position(symbol={}, exchange={}, direction={}, quantity={}, avg_price={:.4}, unrealized_pnl={:.4}, realized_pnl={:.4})",
            self.inner.symbol,
            self.inner.exchange,
            self.inner.direction,
            self.inner.quantity,
            self.inner.avg_price,
            self.inner.unrealized_pnl,
            self.inner.realized_pnl,
        )
    }
}

impl PyPosition {
    pub fn from_snapshot(snapshot: &PositionSnapshot) -> Self {
        Self {
            inner: snapshot.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// PortfolioFacade — the main PyO3 class exposed to Python strategies
// ---------------------------------------------------------------------------

/// Read-only facade that Python strategies use to query portfolio state.
///
/// ```python
/// class MyStrategy(Strategy):
///     def on_bar(self, bar):
///         balance = self.portfolio.balance
///         available = self.portfolio.available
///         equity = self.portfolio.equity
///         pos = self.portfolio.position("BTCUSDT.BINANCE")
///         net = self.portfolio.net_position("BTCUSDT.BINANCE")
/// ```
#[pyclass(name = "Portfolio")]
pub struct PortfolioFacade {
    inner: Arc<Mutex<PortfolioState>>,
}

impl PortfolioFacade {
    /// Create a new empty portfolio facade.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PortfolioState::default())),
        }
    }

    /// Create a facade backed by a shared `PortfolioState`.
    /// This allows engines to update the same state that strategies read.
    pub fn from_state(state: Arc<Mutex<PortfolioState>>) -> Self {
        Self { inner: state }
    }

    /// Get a clone of the inner `Arc<Mutex<PortfolioState>>` so engines can
    /// update the same state.
    pub fn state_handle(&self) -> Arc<Mutex<PortfolioState>> {
        Arc::clone(&self.inner)
    }

    /// Update from a live `AccountData` push.
    pub fn update_from_account(&self, account: &AccountData) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_from_account(account);
    }

    /// Update from a live `PositionData` push.
    pub fn update_from_position(&self, position: &PositionData) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_from_position(position);
    }

    /// Update balance (backtesting engine).
    pub fn update_balance(&self, capital: f64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_balance(capital);
    }

    /// Update or insert a position from a backtesting fill.
    pub fn update_position_fill(
        &self,
        vt_symbol: &str,
        direction: Direction,
        quantity: f64,
        avg_price: f64,
        realized: f64,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_position_fill(vt_symbol, direction, quantity, avg_price, realized);
    }

    /// Update mark price for a position (recalculates unrealized PnL).
    pub fn update_mark_price(&self, vt_symbol: &str, direction: &Direction, mark_price: f64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_mark_price(vt_symbol, direction, mark_price);
    }

    /// Record or replace a daily result for the given date.
    pub fn update_daily_result(&self, date: NaiveDate, result: DailyResult) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .update_daily_result(date, result);
    }

    /// Set the starting capital explicitly.
    pub fn set_start_capital(&self, capital: f64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_start_capital(capital);
    }
}

#[pymethods]
impl PortfolioFacade {
    /// Total account balance.
    #[getter]
    fn balance(&self) -> f64 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).balance
    }

    /// Available (unfrozen) balance.
    #[getter]
    fn available(&self) -> f64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .available()
    }

    /// Equity = balance + total unrealized PnL.
    #[getter]
    fn equity(&self) -> f64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .equity()
    }

    /// Total unrealized PnL across all positions.
    #[getter]
    fn unrealized_pnl(&self) -> f64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .total_unrealized_pnl()
    }

    /// Frozen margin/capital.
    #[getter]
    fn frozen(&self) -> f64 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).frozen
    }

    /// Look up a single position by `vt_symbol` (e.g. "BTCUSDT.BINANCE").
    ///
    /// Returns the first matching position, preferring LONG > SHORT > NET.
    /// Returns `None` if no position exists for that symbol.
    fn position(&self, vt_symbol: &str) -> Option<PyPosition> {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state.get_position(vt_symbol).map(PyPosition::from_snapshot)
    }

    /// All positions as a list of `Position` objects.
    #[getter]
    fn positions(&self) -> Vec<PyPosition> {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state
            .get_all_positions()
            .into_iter()
            .map(PyPosition::from_snapshot)
            .collect()
    }

    /// Net signed position for a given `vt_symbol`.
    /// Positive = long, negative = short.
    fn net_position(&self, vt_symbol: &str) -> f64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .net_position(vt_symbol)
    }

    /// Compute portfolio-level statistics from the accumulated daily results.
    ///
    /// Uses `calculate_statistics` from the backtesting module with default
    /// parameters: risk_free = 0.0, annual_days = 252.
    ///
    /// Returns an empty `PortfolioStatistics` if no daily results have been
    /// recorded yet.
    fn statistics(&self) -> PyPortfolioStatistics {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let stats = calculate_statistics(
            &state.daily_results,
            state.start_capital,
            0.0, // risk_free
            252, // annual_days
        );
        PyPortfolioStatistics::from_statistics(stats)
    }

    fn __repr__(&self) -> String {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "Portfolio(balance={:.2}, available={:.2}, equity={:.2}, positions={})",
            state.balance,
            state.available(),
            state.equity(),
            state.positions.len(),
        )
    }
}

impl Default for PortfolioFacade {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Registration helper (called from bindings.rs)
// ---------------------------------------------------------------------------

/// Register PortfolioFacade, PyPosition, and PyPortfolioStatistics with the PyO3 module.
pub fn register_portfolio_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PortfolioFacade>()?;
    m.add_class::<PyPosition>()?;
    crate::python::portfolio_stats::register_portfolio_stats_module(m)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> PortfolioState {
        PortfolioState::default()
    }

    #[test]
    fn test_default_state() {
        let state = make_state();
        assert_eq!(state.balance, 0.0);
        assert_eq!(state.frozen, 0.0);
        assert_eq!(state.available(), 0.0);
        assert_eq!(state.equity(), 0.0);
        assert_eq!(state.total_unrealized_pnl(), 0.0);
    }

    #[test]
    fn test_update_balance() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        assert_eq!(state.balance, 100_000.0);
        assert_eq!(state.available(), 100_000.0);
        assert_eq!(state.equity(), 100_000.0);
    }

    #[test]
    fn test_update_from_account() {
        let mut state = make_state();
        let account = AccountData {
            gateway_name: "TEST".into(),
            accountid: "ACC1".into(),
            balance: 50_000.0,
            frozen: 10_000.0,
            extra: None,
        };
        state.update_from_account(&account);
        assert_eq!(state.balance, 50_000.0);
        assert_eq!(state.frozen, 10_000.0);
        assert_eq!(state.available(), 40_000.0);
    }

    #[test]
    fn test_update_position_fill_long() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        assert_eq!(state.positions.len(), 1);

        let pos = state.get_position("BTCUSDT.BINANCE").unwrap();
        assert_eq!(pos.symbol, "BTCUSDT");
        assert_eq!(pos.exchange, "BINANCE");
        assert_eq!(pos.direction, "LONG");
        assert_eq!(pos.quantity, 10.0);
        assert_eq!(pos.avg_price, 50_000.0);
    }

    #[test]
    fn test_update_position_fill_short() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        state.update_position_fill("ETHUSDT.BINANCE", Direction::Short, 5.0, 3_000.0, 0.0);
        assert_eq!(state.positions.len(), 1);

        let pos = state.get_position("ETHUSDT.BINANCE").unwrap();
        assert_eq!(pos.direction, "SHORT");
        assert_eq!(pos.quantity, 5.0);
        assert_eq!(pos.avg_price, 3_000.0);
    }

    #[test]
    fn test_remove_flat_position() {
        let mut state = make_state();
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        assert_eq!(state.positions.len(), 1);

        // Close position — quantity 0 removes it
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 0.0, 55_000.0, 50_000.0);
        assert!(state.positions.is_empty());
        assert!(state.get_position("BTCUSDT.BINANCE").is_none());
    }

    #[test]
    fn test_net_position_single_long() {
        let mut state = make_state();
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        let net = state.net_position("BTCUSDT.BINANCE");
        assert_eq!(net, 10.0);
    }

    #[test]
    fn test_net_position_single_short() {
        let mut state = make_state();
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Short, 5.0, 50_000.0, 0.0);
        let net = state.net_position("BTCUSDT.BINANCE");
        assert_eq!(net, -5.0);
    }

    #[test]
    fn test_net_position_no_position() {
        let state = make_state();
        let net = state.net_position("BTCUSDT.BINANCE");
        assert_eq!(net, 0.0);
    }

    #[test]
    fn test_net_position_long_and_short_same_symbol() {
        let mut state = make_state();
        // Some exchanges report long and short as separate positions
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 8.0, 50_000.0, 0.0);
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Short, 3.0, 51_000.0, 0.0);
        // Net = 8 - 3 = 5
        let net = state.net_position("BTCUSDT.BINANCE");
        assert_eq!(net, 5.0);
    }

    #[test]
    fn test_mark_price_updates_unrealized_pnl() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);

        // Mark price moves up — long should have positive unrealized PnL
        state.update_mark_price("BTCUSDT.BINANCE", &Direction::Long, 55_000.0);
        let pos = state.get_position("BTCUSDT.BINANCE").unwrap();
        // (55000 - 50000) * 10 = 50000
        assert!((pos.unrealized_pnl - 50_000.0).abs() < 1e-10);

        // Equity should reflect unrealized PnL
        assert!((state.equity() - 150_000.0).abs() < 1e-10);
    }

    #[test]
    fn test_mark_price_short_unrealized_pnl() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Short, 5.0, 50_000.0, 0.0);

        // Mark price drops — short should have positive unrealized PnL
        state.update_mark_price("BTCUSDT.BINANCE", &Direction::Short, 48_000.0);
        let pos = state.get_position("BTCUSDT.BINANCE").unwrap();
        // (50000 - 48000) * 5 = 10000
        assert!((pos.unrealized_pnl - 10_000.0).abs() < 1e-10);
    }

    #[test]
    fn test_update_from_position_data() {
        let mut state = make_state();
        let pos_data = PositionData {
            gateway_name: "TEST".into(),
            symbol: "BTCUSDT".into(),
            exchange: crate::trader::Exchange::Binance,
            direction: Direction::Long,
            volume: 10.0,
            frozen: 2.0,
            price: 50_000.0,
            pnl: 5_000.0,
            yd_volume: 5.0,
            extra: None,
        };
        state.update_from_position(&pos_data);

        let pos = state.get_position("BTCUSDT.BINANCE").unwrap();
        assert_eq!(pos.quantity, 10.0);
        assert_eq!(pos.avg_price, 50_000.0);
        assert_eq!(pos.unrealized_pnl, 5_000.0);
    }

    #[test]
    fn test_update_from_position_data_flat_removes() {
        let mut state = make_state();
        // First add a position
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        assert_eq!(state.positions.len(), 1);

        // Then receive a PositionData with volume 0 — should remove the position
        let pos_data = PositionData {
            gateway_name: "TEST".into(),
            symbol: "BTCUSDT".into(),
            exchange: crate::trader::Exchange::Binance,
            direction: Direction::Long,
            volume: 0.0,
            frozen: 0.0,
            price: 0.0,
            pnl: 0.0,
            yd_volume: 0.0,
            extra: None,
        };
        state.update_from_position(&pos_data);
        assert!(state.positions.is_empty());
    }

    #[test]
    fn test_position_key_uniqueness() {
        let mut state = make_state();
        // Same symbol, different directions — should be separate entries
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Short, 5.0, 51_000.0, 0.0);
        assert_eq!(state.positions.len(), 2);
    }

    #[test]
    fn test_get_all_positions() {
        let mut state = make_state();
        state.update_position_fill("BTCUSDT.BINANCE", Direction::Long, 10.0, 50_000.0, 0.0);
        state.update_position_fill("ETHUSDT.BINANCE", Direction::Long, 20.0, 3_000.0, 0.0);
        let all = state.get_all_positions();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_realized_pnl_tracking() {
        let mut state = make_state();
        state.update_balance(100_000.0);
        // Open long, then close with realized PnL
        state.update_position_fill(
            "BTCUSDT.BINANCE",
            Direction::Long,
            0.0, // flat
            55_000.0,
            50_000.0, // realized PnL from closing
        );
        // Position was removed because quantity is 0
        assert!(state.positions.is_empty());
    }

    #[test]
    fn test_daily_results_field() {
        let mut state = make_state();
        assert!(state.daily_results.is_empty());

        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let result = DailyResult::new(date, 50_000.0);
        state.update_daily_result(date, result);
        assert_eq!(state.daily_results.len(), 1);
    }

    #[test]
    fn test_update_daily_result_replaces() {
        let mut state = make_state();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        let mut result1 = DailyResult::new(date, 50_000.0);
        result1.net_pnl = 100.0;
        state.update_daily_result(date, result1);

        let mut result2 = DailyResult::new(date, 51_000.0);
        result2.net_pnl = 200.0;
        state.update_daily_result(date, result2);

        assert_eq!(state.daily_results.len(), 1);
        assert!((state.daily_results[&date].net_pnl - 200.0).abs() < 1e-10);
    }

    #[test]
    fn test_start_capital_auto_set() {
        let mut state = make_state();
        assert_eq!(state.start_capital, 0.0);
        state.update_balance(100_000.0);
        assert_eq!(state.start_capital, 100_000.0);
        // Second update does not change start_capital
        state.update_balance(105_000.0);
        assert_eq!(state.start_capital, 100_000.0);
    }

    #[test]
    fn test_set_start_capital_explicit() {
        let mut state = make_state();
        state.set_start_capital(50_000.0);
        assert_eq!(state.start_capital, 50_000.0);
        // update_balance won't override because start_capital is already set
        state.update_balance(100_000.0);
        assert_eq!(state.start_capital, 50_000.0);
    }

    #[test]
    fn test_statistics_empty_daily_results() {
        let state = make_state();
        let stats = calculate_statistics(&state.daily_results, state.start_capital, 0.0, 252);
        assert_eq!(stats.total_days, 0);
        assert_eq!(stats.start_date, "");
    }

    #[test]
    fn test_statistics_with_daily_results() {
        let mut state = make_state();
        state.set_start_capital(100_000.0);

        let date1 = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let mut result1 = DailyResult::new(date1, 50_000.0);
        result1.net_pnl = 1_000.0;
        result1.commission = 50.0;
        result1.slippage = 10.0;
        result1.turnover = 100_000.0;
        result1.trade_count = 5;
        state.update_daily_result(date1, result1);

        let date2 = NaiveDate::from_ymd_opt(2025, 1, 16).unwrap();
        let mut result2 = DailyResult::new(date2, 51_000.0);
        result2.net_pnl = -500.0;
        result2.commission = 30.0;
        result2.slippage = 5.0;
        result2.turnover = 50_000.0;
        result2.trade_count = 3;
        state.update_daily_result(date2, result2);

        let stats = calculate_statistics(&state.daily_results, state.start_capital, 0.0, 252);
        assert_eq!(stats.total_days, 2);
        assert_eq!(stats.profit_days, 1);
        assert_eq!(stats.loss_days, 1);
        assert!((stats.total_net_pnl - 500.0).abs() < 1e-10);
        assert!((stats.end_balance - 100_500.0).abs() < 1e-10);
    }
}
