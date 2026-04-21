//! PyO3 bindings for ArrayManager — technical indicator calculator.
//!
//! Exposes 25+ indicators (SMA, EMA, MACD, RSI, ATR, Bollinger, KDJ, CCI,
//! OBV, ADX, Aroon, Stochastic, Donchian, Keltner, SAR, etc.) to Python
//! strategies.

use pyo3::prelude::*;

use crate::trader::ArrayManager;

/// Python-facing wrapper around the Rust `ArrayManager`.
///
/// Maintains a rolling window of OHLCV bar data and computes technical
/// indicators on demand.  A typical usage pattern:
///
/// ```python
/// from trade_engine import ArrayManager
///
/// am = ArrayManager(100)
/// for bar in bars:
///     am.update(bar["open"], bar["high"], bar["low"], bar["close"], bar["volume"], bar.get("turnover", 0))
///     if am.inited:
///         sma5 = am.sma(5)
///         macd, signal, hist = am.macd(12, 26, 9)
/// ```
#[pyclass(name = "ArrayManager")]
pub struct PyArrayManager {
    inner: ArrayManager,
    size: usize,
}

#[pymethods]
impl PyArrayManager {
    /// Create a new ArrayManager with the given window size.
    ///
    /// Args:
    ///     size: Number of bars to keep in the rolling window (default 100).
    #[new]
    #[pyo3(signature = (size=100))]
    fn new(size: usize) -> Self {
        PyArrayManager {
            inner: ArrayManager::new(size),
            size,
        }
    }

    /// Push a new bar into the rolling window.
    ///
    /// Args:
    ///     open:     Opening price
    ///     high:     Highest price
    ///     low:      Lowest price
    ///     close:    Closing price
    ///     volume:   Trade volume
    ///     turnover: Trade turnover (default 0.0)
    #[pyo3(signature = (open, high, low, close, volume, turnover=0.0))]
    fn update(&mut self, open: f64, high: f64, low: f64, close: f64, volume: f64, turnover: f64) {
        use crate::trader::object::BarData;
        use crate::trader::constant::{Exchange, Interval};

        let bar = BarData {
            gateway_name: String::new(),
            symbol: String::new(),
            exchange: Exchange::Local,
            datetime: chrono::Utc::now(),
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover,
            open_interest: 0.0,
            extra: None,
        };
        self.inner.update_bar(&bar);
    }

    /// Whether the ArrayManager has received enough bars to fill its window.
    #[getter]
    fn inited(&self) -> bool {
        self.inner.is_inited()
    }

    /// The window size (number of bars tracked).
    #[getter]
    fn size(&self) -> usize {
        self.size
    }

    // ==================== Raw Arrays ====================

    /// Get the open price array.
    #[getter]
    fn open(&self) -> Vec<f64> {
        self.inner.open().to_vec()
    }

    /// Get the high price array.
    #[getter]
    fn high(&self) -> Vec<f64> {
        self.inner.high().to_vec()
    }

    /// Get the low price array.
    #[getter]
    fn low(&self) -> Vec<f64> {
        self.inner.low().to_vec()
    }

    /// Get the close price array.
    #[getter]
    fn close(&self) -> Vec<f64> {
        self.inner.close().to_vec()
    }

    /// Get the volume array.
    #[getter]
    fn volume(&self) -> Vec<f64> {
        self.inner.volume().to_vec()
    }

    /// Get the turnover array.
    #[getter]
    fn turnover(&self) -> Vec<f64> {
        self.inner.turnover().to_vec()
    }

    // ==================== Moving Averages ====================

    /// Simple Moving Average.
    ///
    /// Args:
    ///     n: Period length.
    ///
    /// Returns:
    ///     The SMA value (0.0 if not enough data).
    fn sma(&self, n: usize) -> f64 {
        self.inner.sma(n)
    }

    /// Simple Moving Average array.
    fn sma_array(&self, n: usize) -> Vec<f64> {
        self.inner.sma_array(n)
    }

    /// Exponential Moving Average.
    ///
    /// Args:
    ///     n: Period length.
    fn ema(&self, n: usize) -> f64 {
        self.inner.ema(n)
    }

    /// Exponential Moving Average array.
    fn ema_array(&self, n: usize) -> Vec<f64> {
        self.inner.ema_array(n)
    }

    // ==================== Momentum Indicators ====================

    /// Relative Strength Index.
    ///
    /// Args:
    ///     n: Period length.
    fn rsi(&self, n: usize) -> f64 {
        self.inner.rsi(n)
    }

    /// RSI array.
    fn rsi_array(&self, n: usize) -> Vec<f64> {
        self.inner.rsi_array(n)
    }

    /// Rate of Change.
    ///
    /// Args:
    ///     n: Period length.
    fn roc(&self, n: usize) -> f64 {
        self.inner.roc(n)
    }

    /// ROC array.
    fn roc_array(&self, n: usize) -> Vec<f64> {
        self.inner.roc_array(n)
    }

    /// Momentum (price change over n periods).
    ///
    /// Args:
    ///     n: Period length.
    fn mom(&self, n: usize) -> f64 {
        self.inner.mom(n)
    }

    // ==================== Volatility Indicators ====================

    /// Standard Deviation.
    ///
    /// Args:
    ///     n: Period length.
    fn std(&self, n: usize) -> f64 {
        self.inner.std(n)
    }

    /// Standard Deviation array.
    fn std_array(&self, n: usize) -> Vec<f64> {
        self.inner.std_array(n)
    }

    /// Average True Range.
    ///
    /// Args:
    ///     n: Period length.
    fn atr(&self, n: usize) -> f64 {
        self.inner.atr(n)
    }

    /// ATR array.
    fn atr_array(&self, n: usize) -> Vec<f64> {
        self.inner.atr_array(n)
    }

    /// True Range.
    fn trange(&self) -> f64 {
        self.inner.trange()
    }

    /// True Range array.
    fn trange_array(&self) -> Vec<f64> {
        self.inner.trange_array()
    }

    /// Normalized ATR (ATR / Close * 100).
    ///
    /// Args:
    ///     n: Period length.
    fn natr(&self, n: usize) -> f64 {
        self.inner.natr(n)
    }

    // ==================== Trend Indicators ====================

    /// Moving Average Convergence Divergence.
    ///
    /// Args:
    ///     fast:   Fast EMA period (default 12)
    ///     slow:   Slow EMA period (default 26)
    ///     signal: Signal EMA period (default 9)
    ///
    /// Returns:
    ///     Tuple of (macd_line, signal_line, histogram).
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn macd(&self, fast: usize, slow: usize, signal: usize) -> (f64, f64, f64) {
        self.inner.macd(fast, slow, signal)
    }

    /// MACD arrays.
    ///
    /// Returns:
    ///     Tuple of (macd_array, signal_array, histogram_array).
    #[pyo3(signature = (fast=12, slow=26, signal=9))]
    fn macd_array(&self, fast: usize, slow: usize, signal: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        self.inner.macd_array(fast, slow, signal)
    }

    /// Commodity Channel Index.
    ///
    /// Args:
    ///     n: Period length.
    fn cci(&self, n: usize) -> f64 {
        self.inner.cci(n)
    }

    /// CCI array.
    fn cci_array(&self, n: usize) -> Vec<f64> {
        self.inner.cci_array(n)
    }

    // ==================== Channel Indicators ====================

    /// Bollinger Bands.
    ///
    /// Args:
    ///     n:   Period length (default 20).
    ///     dev: Standard deviation multiplier (default 2.0).
    ///
    /// Returns:
    ///     Tuple of (upper, middle, lower).
    #[pyo3(signature = (n=20, dev=2.0))]
    fn boll(&self, n: usize, dev: f64) -> (f64, f64, f64) {
        self.inner.boll(n, dev)
    }

    /// Bollinger Bands arrays.
    ///
    /// Returns:
    ///     Tuple of (upper_array, middle_array, lower_array).
    #[pyo3(signature = (n=20, dev=2.0))]
    fn boll_array(&self, n: usize, dev: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        self.inner.boll_array(n, dev)
    }

    /// Keltner Channel.
    ///
    /// Args:
    ///     n:          Period length (default 20).
    ///     multiplier: ATR multiplier (default 2.0).
    ///
    /// Returns:
    ///     Tuple of (upper, middle, lower).
    #[pyo3(signature = (n=20, multiplier=2.0))]
    fn keltner(&self, n: usize, multiplier: f64) -> (f64, f64, f64) {
        self.inner.keltner(n, multiplier)
    }

    /// Keltner Channel arrays.
    #[pyo3(signature = (n=20, multiplier=2.0))]
    fn keltner_array(&self, n: usize, multiplier: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        self.inner.keltner_array(n, multiplier)
    }

    /// Donchian Channel.
    ///
    /// Args:
    ///     n: Period length.
    ///
    /// Returns:
    ///     Tuple of (upper, lower).
    fn donchian(&self, n: usize) -> (f64, f64) {
        self.inner.donchian(n)
    }

    /// Donchian Channel arrays.
    fn donchian_array(&self, n: usize) -> (Vec<f64>, Vec<f64>) {
        self.inner.donchian_array(n)
    }

    // ==================== Oscillators ====================

    /// Fast Stochastic Oscillator (%K).
    ///
    /// Args:
    ///     period: Look-back period.
    fn stoch_fast(&self, period: usize) -> f64 {
        self.inner.stoch_fast(period)
    }

    /// Slow Stochastic Oscillator (smoothed %K).
    ///
    /// Args:
    ///     stochastic_period: Raw %K period.
    ///     ema_period:        Smoothing period.
    fn stoch_slow(&self, stochastic_period: usize, ema_period: usize) -> f64 {
        self.inner.stoch_slow(stochastic_period, ema_period)
    }

    /// Full Stochastic Oscillator.
    ///
    /// Args:
    ///     k_period: %K look-back period.
    ///     d_period: %D smoothing period.
    ///
    /// Returns:
    ///     Tuple of (%K, %D).
    fn stoch(&self, k_period: usize, d_period: usize) -> (f64, f64) {
        self.inner.stoch(k_period, d_period)
    }

    /// Williams %R.
    ///
    /// Args:
    ///     n: Look-back period.
    fn willr(&self, n: usize) -> f64 {
        self.inner.willr(n)
    }

    // ==================== Volume Indicators ====================

    /// On Balance Volume.
    fn obv(&self) -> f64 {
        self.inner.obv()
    }

    /// OBV array.
    fn obv_array(&self) -> Vec<f64> {
        self.inner.obv_array()
    }

    /// Money Flow Index.
    ///
    /// Args:
    ///     n: Period length.
    fn mfi(&self, n: usize) -> f64 {
        self.inner.mfi(n)
    }

    /// MFI array.
    fn mfi_array(&self, n: usize) -> Vec<f64> {
        self.inner.mfi_array(n)
    }

    // ==================== Price Extremes ====================

    /// Highest high over n periods.
    fn highest(&self, n: usize) -> f64 {
        self.inner.highest(n)
    }

    /// Lowest low over n periods.
    fn lowest(&self, n: usize) -> f64 {
        self.inner.lowest(n)
    }

    // ==================== Directional Movement ====================

    /// Average Directional Index.
    ///
    /// Args:
    ///     n: Period length (minimum 2).
    fn adx(&self, n: usize) -> f64 {
        self.inner.adx(n)
    }

    /// Plus Directional Indicator (+DI).
    ///
    /// Args:
    ///     n: Period length.
    fn plus_di(&self, n: usize) -> f64 {
        self.inner.plus_di(n)
    }

    /// Minus Directional Indicator (-DI).
    ///
    /// Args:
    ///     n: Period length.
    fn minus_di(&self, n: usize) -> f64 {
        self.inner.minus_di(n)
    }

    // ==================== Parabolic SAR ====================

    /// Parabolic SAR.
    ///
    /// Args:
    ///     acceleration: Acceleration factor (default 0.02).
    ///     maximum:      Maximum acceleration (default 0.2).
    ///
    /// Returns:
    ///     The SAR value.
    #[pyo3(signature = (acceleration=0.02, maximum=0.2))]
    fn sar(&self, acceleration: f64, maximum: f64) -> f64 {
        self.inner.sar(acceleration, maximum)
    }

    // ==================== Aroon Indicator ====================

    /// Aroon Indicator.
    ///
    /// Args:
    ///     n: Look-back period.
    ///
    /// Returns:
    ///     Tuple of (aroon_up, aroon_down).
    fn aroon(&self, n: usize) -> (f64, f64) {
        self.inner.aroon(n)
    }

    /// Aroon Oscillator.
    ///
    /// Args:
    ///     n: Look-back period.
    fn aroonosc(&self, n: usize) -> f64 {
        self.inner.aroonosc(n)
    }

    // ==================== Ultimate Oscillator ====================

    /// Ultimate Oscillator.
    ///
    /// Args:
    ///     period1: Short period (default 7).
    ///     period2: Medium period (default 14).
    ///     period3: Long period (default 28).
    #[pyo3(signature = (period1=7, period2=14, period3=28))]
    fn ultosc(&self, period1: usize, period2: usize, period3: usize) -> f64 {
        self.inner.ultosc(period1, period2, period3)
    }

    // ==================== Balance of Power ====================

    /// Balance of Power.
    fn bop(&self) -> f64 {
        self.inner.bop()
    }
}

/// Register the ArrayManager submodule with the parent trade_engine module.
pub fn register_arraymanager_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyArrayManager>()?;
    Ok(())
}
