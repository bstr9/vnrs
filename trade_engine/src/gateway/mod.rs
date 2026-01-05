//! Gateway module - Exchange gateway implementations.
//!
//! This module provides gateway implementations for connecting to various exchanges:
//!
//! - **binance**: Binance exchange gateways (Spot, USDT Futures, Inverse Futures)

pub mod binance;

// Re-exports
pub use binance::{
    BinanceSpotGateway,
    BinanceUsdtGateway,
};
