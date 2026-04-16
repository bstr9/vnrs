//! Typed Identifiers
//!
//! Type-safe identifier types inspired by nautilus_trader.
//! Prevents mixing up symbol strings, order IDs, and position IDs at compile time.
//!
//! All identifiers support `FromStr` (format: "value" or "SYMBOL.EXCHANGE")
//! and `Display` for serialization, maintaining backward compatibility with
//! the existing `vt_symbol` convention.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::constant::Exchange;

/// Unique instrument identifier combining symbol and exchange.
///
/// Format: `SYMBOL.EXCHANGE` (e.g., `BTCUSDT.BINANCE`, `rb2401.SHFE`)
///
/// This replaces bare `String` vt_symbol parameters throughout the API,
/// making it impossible to pass an order ID where an instrument ID is expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId {
    /// Trading symbol (e.g., "BTCUSDT", "rb2401")
    symbol: String,
    /// Exchange where the instrument is listed
    exchange: Exchange,
}

impl InstrumentId {
    /// Create a new InstrumentId from symbol and exchange
    pub fn new(symbol: String, exchange: Exchange) -> Self {
        Self { symbol, exchange }
    }

    /// Get the symbol part
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Get the exchange part
    pub fn exchange(&self) -> Exchange {
        self.exchange
    }

    /// Convert to the legacy vt_symbol format for backward compatibility
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

impl fmt::Display for InstrumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.symbol, self.exchange.value())
    }
}

impl FromStr for InstrumentId {
    type Err = String;

    /// Parse from "SYMBOL.EXCHANGE" format.
    ///
    /// ```
    /// use trade_engine::trader::identifier::InstrumentId;
    /// use std::str::FromStr;
    ///
    /// let id = InstrumentId::from_str("BTCUSDT.BINANCE").unwrap();
    /// assert_eq!(id.symbol(), "BTCUSDT");
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid InstrumentId format '{}': expected SYMBOL.EXCHANGE",
                s
            ));
        }
        // rsplitn gives us [EXCHANGE, SYMBOL] (reversed because rsplitn)
        let symbol = parts[1].to_string();
        let exchange_str = parts[0];

        let exchange = parse_exchange(exchange_str).ok_or_else(|| {
            format!(
                "Unknown exchange '{}' in InstrumentId '{}'",
                exchange_str, s
            )
        })?;

        Ok(Self { symbol, exchange })
    }
}

/// Unique client order identifier.
///
/// This is generated client-side before an order is sent to the exchange.
/// It allows strategies to track their own orders without waiting for
/// exchange-assigned order IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientOrderId(String);

impl ClientOrderId {
    /// Create a new ClientOrderId
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Generate a unique ClientOrderId using a counter prefix
    pub fn generate(prefix: &str, counter: u64) -> Self {
        Self(format!("{}_{}", prefix, counter))
    }

    /// Get the underlying string value
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClientOrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ClientOrderId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("ClientOrderId cannot be empty".to_string());
        }
        Ok(Self(s.to_string()))
    }
}

/// Unique position identifier.
///
/// In futures trading, a position is identified by (instrument + direction).
/// In spot trading, a position is identified by instrument only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PositionId(String);

impl PositionId {
    /// Create a new PositionId
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Generate a PositionId from instrument and optional direction
    pub fn from_instrument(instrument_id: &InstrumentId, direction: Option<&str>) -> Self {
        match direction {
            Some(dir) => Self(format!("{}.{}", instrument_id, dir)),
            None => Self(instrument_id.to_string()),
        }
    }

    /// Get the underlying string value
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PositionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for PositionId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("PositionId cannot be empty".to_string());
        }
        Ok(Self(s.to_string()))
    }
}

/// Unique strategy identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(String);

impl StrategyId {
    /// Create a new StrategyId
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the underlying string value
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StrategyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for StrategyId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("StrategyId cannot be empty".to_string());
        }
        Ok(Self(s.to_string()))
    }
}

/// Parse an exchange string value into an Exchange enum.
///
/// Matches against `Exchange::value()` output (e.g., "BINANCE", "SHFE").
fn parse_exchange(s: &str) -> Option<Exchange> {
    use Exchange::*;
    match s {
        "CFFEX" => Some(Cffex),
        "SHFE" => Some(Shfe),
        "CZCE" => Some(Czce),
        "DCE" => Some(Dce),
        "INE" => Some(Ine),
        "GFEX" => Some(Gfex),
        "SSE" => Some(Sse),
        "SZSE" => Some(Szse),
        "BSE" => Some(Bse),
        "SHHK" => Some(Shhk),
        "SZHK" => Some(Szhk),
        "SGE" => Some(Sge),
        "WXE" => Some(Wxe),
        "CFETS" => Some(Cfets),
        "XBOND" => Some(Xbond),
        "SMART" => Some(Smart),
        "NYSE" => Some(Nyse),
        "NASDAQ" => Some(Nasdaq),
        "ARCA" => Some(Arca),
        "EDGEA" => Some(Edgea),
        "ISLAND" => Some(Island),
        "BATS" => Some(Bats),
        "IEX" => Some(Iex),
        "AMEX" => Some(Amex),
        "TSE" => Some(Tse),
        "NYMEX" => Some(Nymex),
        "COMEX" => Some(Comex),
        "GLOBEX" => Some(Globex),
        "IDEALPRO" => Some(Idealpro),
        "CME" => Some(Cme),
        "ICE" => Some(Ice),
        "SEHK" => Some(Sehk),
        "HKFE" => Some(Hkfe),
        "SGX" => Some(Sgx),
        "CBOT" => Some(Cbot),
        "CBOE" => Some(Cboe),
        "CFE" => Some(Cfe),
        "DME" => Some(Dme),
        "EUX" => Some(Eurex),
        "APEX" => Some(Apex),
        "LME" => Some(Lme),
        "BMD" => Some(Bmd),
        "TOCOM" => Some(Tocom),
        "EUNX" => Some(Eunx),
        "KRX" => Some(Krx),
        "OTC" => Some(Otc),
        "IBKRATS" => Some(Ibkrats),
        "BINANCE" => Some(Binance),
        "BINANCE_USDM" => Some(BinanceUsdm),
        "BINANCE_COINM" => Some(BinanceCoinm),
        "LOCAL" => Some(Local),
        "GLOBAL" => Some(Global),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_id_from_str() {
        let id = InstrumentId::from_str("BTCUSDT.BINANCE").unwrap();
        assert_eq!(id.symbol(), "BTCUSDT");
        assert_eq!(id.exchange(), Exchange::Binance);
    }

    #[test]
    fn test_instrument_id_display() {
        let id = InstrumentId::new("BTCUSDT".to_string(), Exchange::Binance);
        assert_eq!(format!("{}", id), "BTCUSDT.BINANCE");
    }

    #[test]
    fn test_instrument_id_vt_symbol_compat() {
        let id = InstrumentId::from_str("rb2401.SHFE").unwrap();
        assert_eq!(id.vt_symbol(), "rb2401.SHFE");
    }

    #[test]
    fn test_instrument_id_invalid_format() {
        assert!(InstrumentId::from_str("NODOT").is_err());
    }

    #[test]
    fn test_instrument_id_unknown_exchange() {
        assert!(InstrumentId::from_str("BTCUSDT.UNKNOWN_EXCHANGE").is_err());
    }

    #[test]
    fn test_client_order_id() {
        let id = ClientOrderId::generate("STRAT", 42);
        assert_eq!(id.value(), "STRAT_42");
        assert_eq!(format!("{}", id), "STRAT_42");
    }

    #[test]
    fn test_client_order_id_from_str() {
        let id = ClientOrderId::from_str("ORDER_123").unwrap();
        assert_eq!(id.value(), "ORDER_123");
    }

    #[test]
    fn test_client_order_id_empty() {
        assert!(ClientOrderId::from_str("").is_err());
    }

    #[test]
    fn test_position_id_from_instrument() {
        let inst = InstrumentId::new("BTCUSDT".to_string(), Exchange::Binance);
        let pid = PositionId::from_instrument(&inst, None);
        assert_eq!(pid.value(), "BTCUSDT.BINANCE");

        let pid_long = PositionId::from_instrument(&inst, Some("LONG"));
        assert_eq!(pid_long.value(), "BTCUSDT.BINANCE.LONG");
    }

    #[test]
    fn test_position_id_from_str() {
        let pid = PositionId::from_str("BTCUSDT.BINANCE.LONG").unwrap();
        assert_eq!(pid.value(), "BTCUSDT.BINANCE.LONG");
    }

    #[test]
    fn test_strategy_id() {
        let sid = StrategyId::new("my_strategy".to_string());
        assert_eq!(sid.value(), "my_strategy");
        assert_eq!(format!("{}", sid), "my_strategy");
    }

    #[test]
    fn test_strategy_id_from_str() {
        let sid = StrategyId::from_str("dual_ma").unwrap();
        assert_eq!(sid.value(), "dual_ma");
    }

    #[test]
    fn test_instrument_id_with_underscore_exchange() {
        // BINANCE_USDM contains underscore - rsplitn handles this correctly
        let id = InstrumentId::from_str("BTCUSDT.BINANCE_USDM").unwrap();
        assert_eq!(id.symbol(), "BTCUSDT");
        assert_eq!(id.exchange(), Exchange::BinanceUsdm);
    }

    #[test]
    fn test_instrument_id_roundtrip() {
        let id = InstrumentId::new("ETHUSDT".to_string(), Exchange::Binance);
        let s = id.to_string();
        let id2 = InstrumentId::from_str(&s).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn test_parse_exchange_all_variants() {
        // Spot test a few exchanges
        assert_eq!(parse_exchange("BINANCE"), Some(Exchange::Binance));
        assert_eq!(parse_exchange("SHFE"), Some(Exchange::Shfe));
        assert_eq!(parse_exchange("SSE"), Some(Exchange::Sse));
        assert_eq!(parse_exchange("BINANCE_USDM"), Some(Exchange::BinanceUsdm));
        assert_eq!(parse_exchange("LOCAL"), Some(Exchange::Local));
        assert_eq!(parse_exchange("NONEXISTENT"), None);
    }
}
