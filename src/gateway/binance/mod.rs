//! Binance exchange gateway implementations.
//!
//! Supports:
//! - Spot trading (BinanceSpotGateway)
//! - USDT-M Futures trading (BinanceUsdtGateway)

mod config;
mod constants;
mod rest_client;
mod websocket_client;
mod spot_gateway;
mod usdt_gateway;

pub use config::{BinanceConfigs, BinanceGatewayConfig};
pub use spot_gateway::BinanceSpotGateway;
pub use usdt_gateway::BinanceUsdtGateway;
pub use constants::*;
