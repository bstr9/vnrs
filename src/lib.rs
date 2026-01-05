//! Trade Engine - A high-performance trading engine written in Rust
//!
//! This crate provides a complete trading platform framework including:
//!
//! - Market data handling (ticks, bars)
//! - Order management system
//! - Gateway abstraction for multiple exchanges
//! - Event-driven architecture
//! - Strategy optimization tools
//! - Chart visualization (with `gui` feature)
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use trade_engine::trader::{MainEngine, Exchange, SubscribeRequest};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create main engine
//!     let engine = MainEngine::new();
//!     
//!     // Subscribe to market data
//!     let req = SubscribeRequest::new("BTCUSDT".to_string(), Exchange::Binance);
//!     // engine.subscribe(req, "binance").await;
//! }
//! ```

pub mod event;
pub mod trader;
pub mod rpc;
pub mod alpha;
pub mod gateway;
pub mod strategy;
pub mod backtesting;

#[cfg(feature = "gui")]
pub mod chart;

// 条件性导入python模块
#[cfg(feature = "python")]
pub mod python;

// Re-export commonly used types
pub use event::{Event, EventEngine, EVENT_TIMER};
pub use rpc::{client::RpcClient as RpcClient, server::RpcServer as RpcServer};
pub use alpha::{AlphaLab, AlphaDataset, AlphaModel, AlphaStrategy, Segment, logger as alpha_logger, AlphaBarData};
pub use strategy::{StrategyEngine, StrategyTemplate, StrategyContext, StrategyType, StrategyState};
pub use backtesting::{BacktestingEngine as CtaBacktestingEngine, BacktestingMode, DailyResult, BacktestingResult};
#[cfg(feature = "python")]
pub use python::{PythonStrategy, PythonEngine};
pub use trader::{
    // Constants
    Direction, Exchange, Interval, Offset, OrderType, Product, Status,
    // Data objects
    AccountData, BarData, ContractData, OrderData, PositionData, QuoteData, TickData, TradeData,
    // Requests
    CancelRequest, HistoryRequest, OrderRequest, QuoteRequest, SubscribeRequest,
    // Engine
    MainEngine, OmsEngine, BaseEngine,
    // Gateway
    BaseGateway, GatewayEvent, GatewaySettings,
    // Utilities
    ArrayManager, BarGenerator,
};

// Re-export Binance gateways
pub use gateway::binance::{BinanceSpotGateway, BinanceUsdtGateway};

#[cfg(feature = "gui")]
pub use chart::{ChartWidget, BarManager, CandleItem, VolumeItem};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
