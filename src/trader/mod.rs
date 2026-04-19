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
//! - **alert**: Alert engine for notifications on critical events
//! - **ui**: Graphical user interface components (requires "gui" feature)

pub mod alert;
pub mod algo;
pub mod app;
pub mod bar_synthesizer;
pub mod bracket_order;
pub mod constant;
pub mod clock;
pub mod contract_manager;
pub mod converter;
pub mod database;
pub mod data_download;
pub mod data_engine;
pub mod datafeed;
pub mod engine;
pub mod event;
pub mod gateway;
pub mod identifier;
pub mod logger;
pub mod message_bus;
pub mod object;
pub mod optimize;
pub mod portfolio;
pub mod recorder;
pub mod risk;
pub mod setting;
pub mod stop_order;
pub mod sync_bar_generator;
pub mod utility;
pub mod order_emulator;
pub mod order_book;
pub mod reconciliation;
pub mod session;

#[cfg(feature = "sqlite")]
pub mod sqlite_database;

#[cfg(feature = "gui")]
pub mod ui;

// Re-exports for convenience
pub use alert::{AlertChannel, AlertConfig, AlertEngine, AlertLevel, AlertMessage, LogAlertChannel, WebhookAlertChannel, WebhookConfig};
pub use algo::{AlgoEngine, AlgoId, AlgoOrderState, AlgoStatus, AlgoType, TwapConfig, VwapConfig, OrderExecutor};
pub use app::{AppInfo, BaseApp};
pub use bracket_order::{BracketOrderEngine, ContingencyType, OrderGroupState, OrderRole, OrderGroup, BracketOrderRequest, OcoOrderRequest, OtoOrderRequest, GroupId, ChildOrder};
pub use clock::{Clock, LiveClock, TestClock};
pub use constant::{
    Currency, Direction, Exchange, Interval, Offset, OptionType, OrderType, Product, Status,
};
pub use contract_manager::ContractManager;
pub use converter::{OffsetConverter, PositionHolding};
pub use database::{BarOverview, BaseDatabase, EventRecord, FileDatabase, MemoryDatabase, TickOverview};
#[cfg(feature = "sqlite")]
pub use sqlite_database::SqliteDatabase;
pub use data_download::{DataDownloadManager, DownloadConfig, DownloadProgress, DownloadResult};
pub use data_engine::{DataEngine, TickBarAggregator, DefaultBarAggregator};
pub use datafeed::{BaseDatafeed, EmptyDatafeed};
pub use engine::{BaseEngine, LogEngine, MainEngine, OmsEngine};
pub use event::*;
pub use gateway::{BaseGateway, GatewayEvent, GatewayEventSender, GatewaySettings, GatewaySettingValue};
pub use identifier::{ClientOrderId, InstrumentId, PositionId, StrategyId};
pub use logger::{init_logger, Logger, DEBUG, ERROR, INFO, WARNING, CRITICAL};
pub use message_bus::{BusMessage, MessageBus};
pub use object::{
    AccountData, BarData, CancelRequest, ContractData, DepthData, HistoryRequest, LogData, OrderData,
    OrderRequest, PositionData, QuoteData, QuoteRequest, SubscribeRequest, TickData, TradeData,
};
pub use optimize::{check_optimization_setting, run_bf_optimization, run_ga_optimization, OptimizationSetting};
pub use portfolio::{PortfolioManager, PositionSummary, PortfolioSummary, PortfolioMetrics};
pub use recorder::{DataRecorder, RecordStatus, RecorderConfig};
pub use risk::{DailyStats, RiskCheckResult, RiskConfig, RiskManager};
pub use setting::{Settings, SettingValue, SETTINGS};
pub use stop_order::{StopOrderEngine, StopOrder, StopOrderRequest, StopOrderType, StopOrderStatus, StopOrderId};
pub use order_emulator::{OrderEmulator, EmulatedOrderType, EmulatedOrderStatus, EmulatedOrder, EmulatedOrderRequest, EmulatedOrderId, EmulatorSendOrderCallback, EmulatorCancelOrderCallback};
pub use order_book::{OrderBook, OrderBookManager, OrderBookSnapshot};
pub use reconciliation::{PositionDrift, OrderDrift, ReconciliationResult, ReconciliationEngine};
pub use session::{TradingSessionManager, TradingSession};
pub use sync_bar_generator::{SynchronizedBarGenerator, SynchronizedBars};
pub use bar_synthesizer::BarSynthesizer;
pub use utility::{
    ceil_to, extract_vt_symbol, floor_to, generate_vt_symbol, get_digits, get_file_path,
    get_folder_path, load_json, round_to, save_json, ArrayManager, BarGenerator, TEMP_DIR,
    TRADER_DIR,
};
