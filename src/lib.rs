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

#![deny(clippy::unwrap_used)]
#![warn(clippy::map_unwrap_or, clippy::needless_pass_by_value, clippy::unused_self, clippy::too_many_lines)]
// Allow pedantic lints endemic to this codebase (PyO3 bindings, event handlers, trading domain, etc.).
// These are stylistic/pedantic preferences that don't affect correctness.
// Cast lints are handled per-item because they are force-enabled via -W on the command line.
#![allow(
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::redundant_closure,
    clippy::filter_map_identity,
    // Documentation style
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::doc_link_with_quotes,
    clippy::doc_lazy_continuation,
    // Must-use / return type style
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    // Unnecessary wrappers / format style
    clippy::unnecessary_wraps,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::uninlined_format_args,
    clippy::format_push_string,
    // Pattern / match style
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::match_wildcard_for_single_variants,
    clippy::unnested_or_patterns,
    clippy::wildcard_imports,
    clippy::ignored_unit_patterns,
    clippy::if_not_else,
    clippy::redundant_else,
    clippy::comparison_chain,
    clippy::enum_glob_use,
    // Code style preferences
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::manual_midpoint,
    clippy::items_after_statements,
    clippy::single_char_pattern,
    clippy::assigning_clones,
    clippy::cloned_instead_of_copied,
    clippy::explicit_iter_loop,
    clippy::implicit_clone,
    clippy::inefficient_to_string,
    clippy::trivially_copy_pass_by_ref,
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::used_underscore_binding,
    clippy::no_effect_underscore_binding,
    clippy::unreadable_literal,
    clippy::non_std_lazy_statics,
    clippy::default_trait_access,
    clippy::derivable_impls,
    clippy::field_reassign_with_default,
    clippy::semicolon_if_nothing_returned,
    clippy::unnecessary_map_or,
    clippy::redundant_closure_for_method_calls,
    // Domain-specific: trading code uses f64 comparisons naturally
    clippy::float_cmp,
    clippy::implicit_hasher,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::missing_fields_in_debug,
    clippy::ref_option,
    clippy::unused_async,
    // Cast lints (trading code uses f64/i64 casts extensively)
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]

pub mod error;
pub mod event;
pub mod trader;
pub mod rpc;
#[cfg(feature = "feature-store")]
pub mod feature;

#[cfg(feature = "alpha")]
pub mod alpha;
pub mod gateway;
pub mod strategy;
pub mod backtesting;

#[cfg(feature = "model-registry")]
pub mod model;

#[cfg(feature = "shadow")]
pub mod shadow;

#[cfg(feature = "agent")]
pub mod agent;

#[cfg(feature = "rl")]
pub mod rl;

#[cfg(feature = "signal")]
pub mod signal;

#[cfg(feature = "gui")]
pub mod chart;

#[cfg(feature = "gui")]
pub mod mcp;

#[cfg(feature = "python")]
pub mod python;

pub use event::{Event, EventEngine, EVENT_TIMER};
pub use rpc::{client::RpcClient as RpcClient, server::RpcServer as RpcServer};
#[cfg(feature = "alpha")]
pub use alpha::{AlphaLab, AlphaDataset, AlphaModel, AlphaStrategy, AlphaStrategyAdapter, Segment, logger as alpha_logger, AlphaBarData};
pub use strategy::{StrategyEngine, StrategyTemplate, StrategyContext, StrategyType, StrategyState, StrategySetting, StrategyRiskConfig, VolatilityStrategy, TrailingStopConfig, DailyRiskStats, TradingMode, PaperTradingEngine, PaperPosition, PaperOrder};
pub use backtesting::{BacktestingEngine as CtaBacktestingEngine, BacktestingMode, DailyResult, BacktestingResult,
    OptimizationEngine, OptimizationResult, Parameter, ParameterSet,
    OutOfSampleResult, WalkForwardResult, WalkForwardWindow,
    ParameterStabilityReport, ParameterStabilityInfo, LiveDeploymentConfig};
#[cfg(feature = "signal")]
pub use signal::{SignalBus, Signal, SignalDirection, SignalStrength, SubscriberId, Subscription};
#[cfg(feature = "model-registry")]
pub use model::{ModelRegistry, ModelServer, ZmqModelServer, ModelEntry, ModelMetrics, ModelStage, Prediction, HealthStatus};
#[cfg(feature = "shadow")]
pub use shadow::{ShadowEngine, ShadowModel, ShadowStage, ShadowPrediction, PredictionComparison, ShadowMetrics, PromotionPolicy, PromotionDecision, ComparisonStore, RollingMetrics, ConfidenceDistribution};
#[cfg(feature = "python")]
pub use python::{Strategy, PythonEngine, PythonEngineBridge, StrategyEngineHandle};
pub use trader::{
    // Constants
    Direction, Exchange, Interval, Offset, OrderType, Product, Status, StpMode,
    // Data objects
    AccountData, BarData, ContractData, OrderData, PositionData, QuoteData, TickData, TradeData,
    // Requests
    CancelRequest, HistoryRequest, OrderRequest, QuoteRequest, SubscribeRequest,
    // Engine
    MainEngine, OmsEngine, BaseEngine,
    // Gateway
    BaseGateway, GatewayEvent, GatewaySettings,
    // Typed identifiers
    ClientOrderId, InstrumentId, PositionId, StrategyId,
    // Utilities
    ArrayManager, BarGenerator, BarSynthesizer,
    // Synchronized bar generator
    SynchronizedBarGenerator, SynchronizedBars,
    // Database
    EventRecord, FileDatabase,
    // Alert engine
    AlertEngine, AlertChannel, AlertConfig, AlertLevel, AlertMessage, LogAlertChannel, WebhookAlertChannel, WebhookConfig,
    // Algo engine
    AlgoEngine, AlgoId, AlgoOrderState, AlgoStatus, AlgoType, TwapConfig, VwapConfig, OrderExecutor,
    // Data download manager
    DataDownloadManager, DownloadConfig, DownloadProgress, DownloadResult,
    // Portfolio manager
    PortfolioManager, PositionSummary, PortfolioSummary, PortfolioMetrics,
    // Stop order engine
    StopOrderEngine, StopOrder, StopOrderRequest, StopOrderType, StopOrderStatus, StopOrderId,
    // Bracket order engine
    BracketOrderEngine, ContingencyType, OrderGroupState, OrderRole, OrderGroup,
    BracketOrderRequest, OcoOrderRequest, OtoOrderRequest, GroupId, ChildOrder,
    // Order emulator engine
    OrderEmulator, EmulatedOrderType, EmulatedOrderStatus, EmulatedOrder, EmulatedOrderRequest, EmulatedOrderId,
    // Clock abstraction
    Clock, LiveClock, TestClock,
    // Contract manager
    ContractManager,
    // Trading session manager
    TradingSessionManager, TradingSession,
    // Order book
    OrderBook, OrderBookManager, OrderBookSnapshot, DepthData,
    // Reconciliation engine
    ReconciliationEngine, PositionDrift, OrderDrift, ReconciliationResult,
    // Trading report & analytics
    TradingReport, StrategyReport, SymbolPnl, DailySummary, TradeRecord, EquityPoint, ReportEngine, StrategyPnlData,
    // Message bus
    MessageBus, BusMessage,
    // Data engine
    DataEngine, TickBarAggregator, DefaultBarAggregator,
};

#[cfg(feature = "prometheus")]
pub use trader::metrics::{
    MetricsEngine, MetricsServer,
    BALANCE, ORDERS_TOTAL, PNL_TOTAL, POSITION_VALUE, REGISTRY,
    STRATEGY_ACTIVE, TICK_COUNT, TRADES_TOTAL,
};

// Re-export Binance gateways
pub use gateway::binance::{BinanceSpotGateway, BinanceUsdtGateway};

#[cfg(feature = "gui")]
pub use chart::{ChartWidget, BarManager, CandleItem, VolumeItem};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
