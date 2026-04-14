//! Trader module - Core trading platform functionality.
//!
//! This module provides the essential components for building a trading platform,
//! including:
//!
//! - **constant**: Trading constants like Direction, Exchange, OrderType, etc.
//! - **object**: Data structures for TickData, BarData, OrderData, etc.
//! - **event**: Event type definitions for the event-driven architecture
//! - **gateway**: Abstract gateway trait for exchange connections
//! - **engine**: Main engine and OMS engine for order management
//! - **converter**: Offset converter for handling position offsets
//! - **setting**: Global settings management
//! - **utility**: Utility functions and helper classes
//! - **database**: Database abstraction for data persistence
//! - **datafeed**: Datafeed abstraction for market data
//! - **logger**: Logging utilities
//! - **optimize**: Parameter optimization utilities
//! - **app**: Application trait for extending functionality
//! - **ui**: Graphical user interface components (requires "gui" feature)

pub mod app;
pub mod constant;
pub mod converter;
pub mod database;
pub mod datafeed;
pub mod engine;
pub mod event;
pub mod gateway;
pub mod logger;
pub mod object;
pub mod optimize;
pub mod setting;
pub mod utility;

#[cfg(feature = "gui")]
pub mod ui;

// Re-exports for convenience
pub use app::{AppInfo, BaseApp};
pub use constant::{
    Currency, Direction, Exchange, Interval, Offset, OptionType, OrderType, Product, Status,
};
pub use converter::{OffsetConverter, PositionHolding};
pub use database::{BarOverview, BaseDatabase, MemoryDatabase, TickOverview};
pub use datafeed::{BaseDatafeed, EmptyDatafeed};
pub use engine::{BaseEngine, LogEngine, MainEngine, OmsEngine};
pub use event::*;
pub use gateway::{BaseGateway, GatewayEvent, GatewayEventSender, GatewaySettings, GatewaySettingValue};
pub use logger::{init_logger, Logger, DEBUG, ERROR, INFO, WARNING, CRITICAL};
pub use object::{
    AccountData, BarData, CancelRequest, ContractData, HistoryRequest, LogData, OrderData,
    OrderRequest, PositionData, QuoteData, QuoteRequest, SubscribeRequest, TickData, TradeData,
};
pub use optimize::{check_optimization_setting, run_bf_optimization, run_ga_optimization, OptimizationSetting};
pub use setting::{Settings, SettingValue, SETTINGS};
pub use utility::{
    ceil_to, extract_vt_symbol, floor_to, generate_vt_symbol, get_digits, get_file_path,
    get_folder_path, load_json, round_to, save_json, ArrayManager, BarGenerator, TEMP_DIR,
    TRADER_DIR,
};
