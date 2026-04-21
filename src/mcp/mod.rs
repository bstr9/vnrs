//! MCP Server 模块 — 为交易系统提供 MCP 协议接口
//!
//! 本模块实现了完整的 MCP Server，包含 7 组 50+ 工具：
//! - **Trading Tools**: connect, subscribe, send_order, cancel_order, query_history, list_contracts,
//!   analyze_sentiment, disconnect, unsubscribe, modify_order, batch_orders, close_position, set_leverage
//! - **UI Tools**: switch_symbol, switch_interval, add_indicator, remove_indicator, clear_indicators,
//!   navigate_to, show_notification
//! - **Market Tools**: get_ticker, get_orderbook, get_candles, get_trades, get_funding_rate,
//!   get_mark_price, get_index_price, get_liquidations, get_open_interest, get_ticker_24h
//! - **Account Tools**: get_balance, get_positions, get_position, get_trade_history, get_fee_rate,
//!   get_account_summary
//! - **Strategy Tools**: list_strategies, get_strategy_status, start_strategy, stop_strategy,
//!   pause_strategy, get_strategy_params, set_strategy_params, get_strategy_performance
//! - **Risk Tools**: get_risk_metrics, set_stop_loss, set_take_profit, check_margin, get_exposure
//! - **Backtest Tools**: run_backtest, get_backtest_result, list_backtests, compare_strategies
//! - **Resources**: 实时交易数据和 UI 状态查询
//! - **Prompts**: 预定义交易分析提示模板
//! - **Sampling**: 支持 MCP Sampling，工具可向 Client 请求 LLM 推理
//!
//! # 使用方式
//!
//! ## STDIO 模式（本地客户端，如 Claude Desktop）
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::TradingMcpServer;
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! tokio::spawn(async move {
//!     server.serve_stdio().await.unwrap();
//! });
//! ```
//!
//! ## HTTP/SSE 模式（远程 Web 客户端）
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::TradingMcpServer;
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! tokio::spawn(async move {
//!     let addr = "127.0.0.1:3000".parse().unwrap();
//!     server.serve_http(addr).await.unwrap();
//! });
//! ```
//!
//! ## 使用 McpConfig 自动选择传输模式
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::{TradingMcpServer, McpConfig};
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! let config = McpConfig::from_env(); // 读取 MCP_MODE 环境变量
//! tokio::spawn(async move {
//!     server.serve_with_config(&config).await.unwrap();
//! });
//! ```

pub mod types;
pub mod tools;
pub mod resources;
pub mod prompts;
pub mod server;

pub use server::TradingMcpServer;
pub use types::{McpConfig, McpTransport, SamplingConfig, SamplingAuditEntry, UICommand, UICommandReceiver, UICommandSender, UIState};
