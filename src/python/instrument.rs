//! Python Instrument class for strategy contract metadata
//!
//! Provides Python strategies with contract metadata (tick_size, lot_size,
//! min_notional, etc.) and utility methods (round_price, round_volume).
//!
//! PyInstrument is constructed from the Rust-side ContractData, reading
//! `min_notional` from the `extra` HashMap where Binance gateways store it.

use pyo3::prelude::*;

use crate::trader::object::ContractData;

/// Python-facing instrument metadata.
///
/// Exposes contract details that Python strategies need for order sizing,
/// price rounding, and risk checks.
///
/// ```python
/// instr = engine.get_instrument("btcusdt.binance")
/// print(instr.pricetick)       # 0.01
/// print(instr.min_notional)    # 10.0
/// price = instr.round_price(50000.005)   # 50000.01
/// vol   = instr.round_volume(1.234)      # 1.0
/// ```
#[pyclass]
pub struct PyInstrument {
    #[pyo3(get)]
    pub symbol: String,
    #[pyo3(get)]
    pub exchange: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub product: String,
    /// Lot size (multiplier for volume)
    #[pyo3(get)]
    pub size: f64,
    /// Minimum price increment (tick size)
    #[pyo3(get)]
    pub pricetick: f64,
    /// Minimum order volume (step size)
    #[pyo3(get)]
    pub min_volume: f64,
    /// Maximum order volume (None = no limit)
    #[pyo3(get)]
    pub max_volume: f64,
    /// Minimum notional value for an order
    #[pyo3(get)]
    pub min_notional: f64,
    /// Margin rate for futures contracts
    #[pyo3(get)]
    pub margin_rate: f64,
    /// Whether stop orders are supported
    #[pyo3(get)]
    pub stop_supported: bool,
    /// Whether the contract uses net position mode
    #[pyo3(get)]
    pub net_position: bool,
}

impl PyInstrument {
    /// Create a PyInstrument from a ContractData.
    ///
    /// Reads `min_notional` from the `extra` HashMap if present.
    pub fn from_contract_data(contract: &ContractData) -> Self {
        let min_notional = contract
            .extra
            .as_ref()
            .and_then(|e| e.get("min_notional"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

        let margin_rate = contract
            .extra
            .as_ref()
            .and_then(|e| e.get("margin_rate"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

        PyInstrument {
            symbol: contract.symbol.clone(),
            exchange: contract.exchange.value().to_string(),
            name: contract.name.clone(),
            product: contract.product.to_string(),
            size: contract.size,
            pricetick: contract.pricetick,
            min_volume: contract.min_volume,
            max_volume: contract.max_volume.unwrap_or(0.0),
            min_notional,
            margin_rate,
            stop_supported: contract.stop_supported,
            net_position: contract.net_position,
        }
    }
}

#[pymethods]
impl PyInstrument {
    /// Round a price to the nearest valid tick.
    ///
    /// Rounds down to the nearest multiple of pricetick.
    /// Returns the price unchanged if pricetick is 0.
    fn round_price(&self, price: f64) -> f64 {
        if self.pricetick <= 0.0 {
            return price;
        }
        (price / self.pricetick).floor() * self.pricetick
    }

    /// Round a volume to the nearest valid lot size.
    ///
    /// Rounds down to the nearest multiple of min_volume (step size).
    /// Returns the volume unchanged if min_volume is 0.
    fn round_volume(&self, volume: f64) -> f64 {
        if self.min_volume <= 0.0 {
            return volume;
        }
        (volume / self.min_volume).floor() * self.min_volume
    }

    fn __repr__(&self) -> String {
        format!(
            "Instrument(symbol={}, exchange={}, pricetick={}, min_volume={}, min_notional={})",
            self.symbol, self.exchange, self.pricetick, self.min_volume, self.min_notional
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Exchange, Product};
    use std::collections::HashMap;

    fn make_contract() -> ContractData {
        let mut extra = HashMap::new();
        extra.insert("min_notional".to_string(), "10.0".to_string());
        extra.insert("margin_rate".to_string(), "0.05".to_string());

        ContractData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "btcusdt".to_string(),
            exchange: Exchange::Binance,
            name: "BTC/USDT".to_string(),
            product: Product::Spot,
            size: 1.0,
            pricetick: 0.01,
            min_volume: 0.001,
            max_volume: Some(9000.0),
            stop_supported: false,
            net_position: true,
            history_data: true,
            option_strike: None,
            option_underlying: None,
            option_type: None,
            option_listed: None,
            option_expiry: None,
            option_portfolio: None,
            option_index: None,
            extra: Some(extra),
        }
    }

    #[test]
    fn test_from_contract_data() {
        let contract = make_contract();
        let instr = PyInstrument::from_contract_data(&contract);

        assert_eq!(instr.symbol, "btcusdt");
        assert_eq!(instr.exchange, "BINANCE");
        assert_eq!(instr.name, "BTC/USDT");
        assert_eq!(instr.product, "现货");
        assert_eq!(instr.size, 1.0);
        assert_eq!(instr.pricetick, 0.01);
        assert_eq!(instr.min_volume, 0.001);
        assert_eq!(instr.max_volume, 9000.0);
        assert_eq!(instr.min_notional, 10.0);
        assert_eq!(instr.margin_rate, 0.05);
        assert!(!instr.stop_supported);
        assert!(instr.net_position);
    }

    #[test]
    fn test_from_contract_data_no_extra() {
        let mut contract = make_contract();
        contract.extra = None;
        let instr = PyInstrument::from_contract_data(&contract);

        assert_eq!(instr.min_notional, 0.0);
        assert_eq!(instr.margin_rate, 0.0);
    }

    #[test]
    fn test_from_contract_data_no_max_volume() {
        let mut contract = make_contract();
        contract.max_volume = None;
        let instr = PyInstrument::from_contract_data(&contract);

        assert_eq!(instr.max_volume, 0.0);
    }

    #[test]
    fn test_round_price() {
        let contract = make_contract();
        let instr = PyInstrument::from_contract_data(&contract);

        // 50000.005 → rounds down to 50000.00 (tick=0.01)
        let rounded = instr.round_price(50000.005);
        assert!((rounded - 50000.00).abs() < 1e-10);

        // Exact tick
        assert!((instr.round_price(50000.01) - 50000.01).abs() < 1e-10);

        // Zero price
        assert!((instr.round_price(0.0)).abs() < 1e-10);
    }

    #[test]
    fn test_round_price_zero_tick() {
        let mut contract = make_contract();
        contract.pricetick = 0.0;
        let instr = PyInstrument::from_contract_data(&contract);

        // With zero tick, return price unchanged
        assert!((instr.round_price(12345.678) - 12345.678).abs() < 1e-10);
    }

    #[test]
    fn test_round_volume() {
        let contract = make_contract();
        let instr = PyInstrument::from_contract_data(&contract);

        // 1.2345 with step=0.001 → 1.234
        let rounded = instr.round_volume(1.2345);
        assert!((rounded - 1.234).abs() < 1e-10);

        // Exact step
        assert!((instr.round_volume(0.003) - 0.003).abs() < 1e-10);
    }

    #[test]
    fn test_round_volume_zero_step() {
        let mut contract = make_contract();
        contract.min_volume = 0.0;
        let instr = PyInstrument::from_contract_data(&contract);

        assert!((instr.round_volume(1.2345) - 1.2345).abs() < 1e-10);
    }

    #[test]
    fn test_repr() {
        let contract = make_contract();
        let instr = PyInstrument::from_contract_data(&contract);
        let repr = instr.__repr__();

        assert!(repr.contains("btcusdt"));
        assert!(repr.contains("BINANCE"));
        assert!(repr.contains("0.01"));
        assert!(repr.contains("0.001"));
        assert!(repr.contains("10"));
    }
}
