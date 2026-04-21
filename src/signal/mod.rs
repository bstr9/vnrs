//! SignalBus module — AI signal pub/sub for trading strategies.
//!
//! This module provides a topic-based pub/sub system that bridges AI signal
//! sources (sentiment analysis, RL strategy output) with traditional strategy
//! consumers. Strategies subscribe to named topics and receive typed `Signal`
//! instances without directly depending on model inference.
//!
//! # Modules
//!
//! - **types**: Core signal data structures (`Signal`, `SignalDirection`, `SignalStrength`)
//! - **bus**: `SignalBus` — topic-based pub/sub with signal caching
//! - **subscriber**: Subscriber identification types (`SubscriberId`, `Subscription`)
//!
//! # Feature Flag
//!
//! This module is gated behind the `signal` feature flag.
//!
//! ```toml
//! [dependencies]
//! trade_engine = { features = ["signal"] }
//! ```
//!
//! # Integration with StrategyEngine
//!
//! Strategies can read cached signals during `on_bar` callbacks by accessing
//! a shared `SignalBus` reference:
//!
//! ```rust,ignore
//! // In strategy on_bar:
//! if let Some(signal) = signal_bus.get_latest("sentiment.btc") {
//!     if signal.is_directional() && signal.is_stronger_than(0.7) {
//!         // Combine AI signal with traditional indicators
//!     }
//! }
//! ```

pub mod bus;
pub mod subscriber;
pub mod types;

pub use bus::SignalBus;
pub use subscriber::{SubscriberId, Subscription};
pub use types::{Signal, SignalDirection, SignalStrength};
