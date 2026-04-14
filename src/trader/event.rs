//! Event type strings used in the trading platform.

/// Timer event type
pub const EVENT_TIMER: &str = "eTimer";

/// Tick data event type (with optional symbol suffix)
pub const EVENT_TICK: &str = "eTick.";

/// Trade data event type (with optional symbol suffix)
pub const EVENT_TRADE: &str = "eTrade.";

/// Order data event type (with optional orderid suffix)
pub const EVENT_ORDER: &str = "eOrder.";

/// Position data event type (with optional symbol suffix)
pub const EVENT_POSITION: &str = "ePosition.";

/// Account data event type (with optional accountid suffix)
pub const EVENT_ACCOUNT: &str = "eAccount.";

/// Quote data event type (with optional symbol suffix)
pub const EVENT_QUOTE: &str = "eQuote.";

/// Contract data event type
pub const EVENT_CONTRACT: &str = "eContract.";

/// Log event type
pub const EVENT_LOG: &str = "eLog";
