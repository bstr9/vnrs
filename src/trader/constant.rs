//! General constant enums used in the trading platform.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Direction of order/trade/position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    /// Long position (多)
    Long,
    /// Short position (空)
    Short,
    /// Net position (净)
    Net,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Long => write!(f, "多"),
            Direction::Short => write!(f, "空"),
            Direction::Net => write!(f, "净"),
        }
    }
}

/// Offset of order/trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Offset {
    #[default]
    None,
    /// Open position (开)
    Open,
    /// Close position (平)
    Close,
    /// Close today position (平今)
    CloseToday,
    /// Close yesterday position (平昨)
    CloseYesterday,
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Offset::None => write!(f, ""),
            Offset::Open => write!(f, "开"),
            Offset::Close => write!(f, "平"),
            Offset::CloseToday => write!(f, "平今"),
            Offset::CloseYesterday => write!(f, "平昨"),
        }
    }
}

/// Order status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Status {
    /// Submitting order (提交中)
    #[default]
    Submitting,
    /// Not traded (未成交)
    NotTraded,
    /// Partially traded (部分成交)
    PartTraded,
    /// All traded (全部成交)
    AllTraded,
    /// Cancelled (已撤销)
    Cancelled,
    /// Rejected (拒单)
    Rejected,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Submitting => write!(f, "提交中"),
            Status::NotTraded => write!(f, "未成交"),
            Status::PartTraded => write!(f, "部分成交"),
            Status::AllTraded => write!(f, "全部成交"),
            Status::Cancelled => write!(f, "已撤销"),
            Status::Rejected => write!(f, "拒单"),
        }
    }
}

/// Product class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Product {
    /// Equity (股票)
    Equity,
    /// Futures (期货)
    Futures,
    /// Option (期权)
    Option,
    /// Index (指数)
    Index,
    /// Forex (外汇)
    Forex,
    /// Spot (现货)
    Spot,
    /// ETF
    Etf,
    /// Bond (债券)
    Bond,
    /// Warrant (权证)
    Warrant,
    /// Spread (价差)
    Spread,
    /// Fund (基金)
    Fund,
    /// CFD
    Cfd,
    /// Swap (互换)
    Swap,
}

impl fmt::Display for Product {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Product::Equity => write!(f, "股票"),
            Product::Futures => write!(f, "期货"),
            Product::Option => write!(f, "期权"),
            Product::Index => write!(f, "指数"),
            Product::Forex => write!(f, "外汇"),
            Product::Spot => write!(f, "现货"),
            Product::Etf => write!(f, "ETF"),
            Product::Bond => write!(f, "债券"),
            Product::Warrant => write!(f, "权证"),
            Product::Spread => write!(f, "价差"),
            Product::Fund => write!(f, "基金"),
            Product::Cfd => write!(f, "CFD"),
            Product::Swap => write!(f, "互换"),
        }
    }
}

/// Order type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OrderType {
    /// Limit order (限价)
    #[default]
    Limit,
    /// Market order (市价)
    Market,
    /// Stop order
    Stop,
    /// Fill and Kill
    Fak,
    /// Fill or Kill
    Fok,
    /// Request for Quote (询价)
    Rfq,
    /// ETF order
    Etf,
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Limit => write!(f, "限价"),
            OrderType::Market => write!(f, "市价"),
            OrderType::Stop => write!(f, "STOP"),
            OrderType::Fak => write!(f, "FAK"),
            OrderType::Fok => write!(f, "FOK"),
            OrderType::Rfq => write!(f, "询价"),
            OrderType::Etf => write!(f, "ETF"),
        }
    }
}

/// Option type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    /// Call option (看涨期权)
    Call,
    /// Put option (看跌期权)
    Put,
}

impl fmt::Display for OptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionType::Call => write!(f, "看涨期权"),
            OptionType::Put => write!(f, "看跌期权"),
        }
    }
}

/// Exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    // Chinese exchanges
    /// China Financial Futures Exchange
    Cffex,
    /// Shanghai Futures Exchange
    Shfe,
    /// Zhengzhou Commodity Exchange
    Czce,
    /// Dalian Commodity Exchange
    Dce,
    /// Shanghai International Energy Exchange
    Ine,
    /// Guangzhou Futures Exchange
    Gfex,
    /// Shanghai Stock Exchange
    Sse,
    /// Shenzhen Stock Exchange
    Szse,
    /// Beijing Stock Exchange
    Bse,
    /// Shanghai-HK Stock Connect
    Shhk,
    /// Shenzhen-HK Stock Connect
    Szhk,
    /// Shanghai Gold Exchange
    Sge,
    /// Wuxi Steel Exchange
    Wxe,
    /// CFETS Bond Market Maker Trading System
    Cfets,
    /// CFETS X-Bond Anonymous Trading System
    Xbond,

    // Global exchanges
    /// Smart Router for US stocks
    Smart,
    /// New York Stock Exchange
    Nyse,
    /// Nasdaq Exchange
    Nasdaq,
    /// ARCA Exchange
    Arca,
    /// Direct Edge Exchange
    Edgea,
    /// Nasdaq Island ECN
    Island,
    /// Bats Global Markets
    Bats,
    /// The Investors Exchange
    Iex,
    /// American Stock Exchange
    Amex,
    /// Toronto Stock Exchange
    Tse,
    /// New York Mercantile Exchange
    Nymex,
    /// COMEX of CME
    Comex,
    /// Globex of CME
    Globex,
    /// Forex ECN of Interactive Brokers
    Idealpro,
    /// Chicago Mercantile Exchange
    Cme,
    /// Intercontinental Exchange
    Ice,
    /// Stock Exchange of Hong Kong
    Sehk,
    /// Hong Kong Futures Exchange
    Hkfe,
    /// Singapore Global Exchange
    Sgx,
    /// Chicago Board of Trade
    Cbot,
    /// Chicago Board Options Exchange
    Cboe,
    /// CBOE Futures Exchange
    Cfe,
    /// Dubai Mercantile Exchange
    Dme,
    /// Eurex Exchange
    Eurex,
    /// Asia Pacific Exchange
    Apex,
    /// London Metal Exchange
    Lme,
    /// Bursa Malaysia Derivatives
    Bmd,
    /// Tokyo Commodity Exchange
    Tocom,
    /// Euronext Exchange
    Eunx,
    /// Korean Exchange
    Krx,
    /// OTC Product (Forex/CFD/Pink Sheet Equity)
    Otc,
    /// Paper Trading Exchange of IB
    Ibkrats,
    /// Binance Spot
    Binance,
    /// Binance USD-M Futures
    BinanceUsdm,
    /// Binance Coin-M Futures
    BinanceCoinm,

    // Special Function
    /// For local generated data
    Local,
    /// For those exchanges not supported yet
    Global,
}

impl Exchange {
    /// Get the exchange value string
    pub fn value(&self) -> &'static str {
        match self {
            Exchange::Cffex => "CFFEX",
            Exchange::Shfe => "SHFE",
            Exchange::Czce => "CZCE",
            Exchange::Dce => "DCE",
            Exchange::Ine => "INE",
            Exchange::Gfex => "GFEX",
            Exchange::Sse => "SSE",
            Exchange::Szse => "SZSE",
            Exchange::Bse => "BSE",
            Exchange::Shhk => "SHHK",
            Exchange::Szhk => "SZHK",
            Exchange::Sge => "SGE",
            Exchange::Wxe => "WXE",
            Exchange::Cfets => "CFETS",
            Exchange::Xbond => "XBOND",
            Exchange::Smart => "SMART",
            Exchange::Nyse => "NYSE",
            Exchange::Nasdaq => "NASDAQ",
            Exchange::Arca => "ARCA",
            Exchange::Edgea => "EDGEA",
            Exchange::Island => "ISLAND",
            Exchange::Bats => "BATS",
            Exchange::Iex => "IEX",
            Exchange::Amex => "AMEX",
            Exchange::Tse => "TSE",
            Exchange::Nymex => "NYMEX",
            Exchange::Comex => "COMEX",
            Exchange::Globex => "GLOBEX",
            Exchange::Idealpro => "IDEALPRO",
            Exchange::Cme => "CME",
            Exchange::Ice => "ICE",
            Exchange::Sehk => "SEHK",
            Exchange::Hkfe => "HKFE",
            Exchange::Sgx => "SGX",
            Exchange::Cbot => "CBOT",
            Exchange::Cboe => "CBOE",
            Exchange::Cfe => "CFE",
            Exchange::Dme => "DME",
            Exchange::Eurex => "EUX",
            Exchange::Apex => "APEX",
            Exchange::Lme => "LME",
            Exchange::Bmd => "BMD",
            Exchange::Tocom => "TOCOM",
            Exchange::Eunx => "EUNX",
            Exchange::Krx => "KRX",
            Exchange::Otc => "OTC",
            Exchange::Ibkrats => "IBKRATS",
            Exchange::Binance => "BINANCE",
            Exchange::BinanceUsdm => "BINANCE_USDM",
            Exchange::BinanceCoinm => "BINANCE_COINM",
            Exchange::Local => "LOCAL",
            Exchange::Global => "GLOBAL",
        }
    }
}

impl fmt::Display for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value())
    }
}

/// Currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    Usd,
    Hkd,
    Cny,
    Cad,
    Eur,
    Gbp,
    Jpy,
    Usdt,
    Btc,
    Eth,
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Currency::Usd => write!(f, "USD"),
            Currency::Hkd => write!(f, "HKD"),
            Currency::Cny => write!(f, "CNY"),
            Currency::Cad => write!(f, "CAD"),
            Currency::Eur => write!(f, "EUR"),
            Currency::Gbp => write!(f, "GBP"),
            Currency::Jpy => write!(f, "JPY"),
            Currency::Usdt => write!(f, "USDT"),
            Currency::Btc => write!(f, "BTC"),
            Currency::Eth => write!(f, "ETH"),
        }
    }
}

/// Interval of bar data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Interval {
    /// 1 second
    Second,
    /// 1 minute
    Minute,
    /// 15 minutes
    Minute15,
    /// 1 hour
    Hour,
    /// 4 hours
    Hour4,
    /// Daily
    Daily,
    /// Weekly
    Weekly,
    /// Tick data
    Tick,
}

impl Interval {
    /// Get interval value string
    pub fn value(&self) -> &'static str {
        match self {
            Interval::Second => "1s",
            Interval::Minute => "1m",
            Interval::Minute15 => "15m",
            Interval::Hour => "1h",
            Interval::Hour4 => "4h",
            Interval::Daily => "d",
            Interval::Weekly => "w",
            Interval::Tick => "tick",
        }
    }
    
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Interval::Second => "1秒",
            Interval::Minute => "1分钟",
            Interval::Minute15 => "15分钟",
            Interval::Hour => "1小时",
            Interval::Hour4 => "4小时",
            Interval::Daily => "1日",
            Interval::Weekly => "1周",
            Interval::Tick => "Tick",
        }
    }
    
    /// Get all intervals for UI selection
    pub fn all() -> Vec<Interval> {
        vec![
            Interval::Second,
            Interval::Minute,
            Interval::Minute15,
            Interval::Hour,
            Interval::Hour4,
            Interval::Daily,
            Interval::Weekly,
        ]
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_display() {
        assert_eq!(format!("{}", Direction::Long), "多");
        assert_eq!(format!("{}", Direction::Short), "空");
    }

    #[test]
    fn test_exchange_value() {
        assert_eq!(Exchange::Binance.value(), "BINANCE");
        assert_eq!(Exchange::Sse.value(), "SSE");
    }

    #[test]
    fn test_interval_value() {
        assert_eq!(Interval::Minute.value(), "1m");
        assert_eq!(Interval::Hour.value(), "1h");
    }
}
