//! MCP Tools 模块入口
//!
//! 包含 7 组工具：
//! - trading：后端交易操作（connect / subscribe / send_order / cancel_order / ...）
//! - ui：前端界面操作（switch_symbol / switch_interval / add_indicator / ...）
//! - market：行情数据查询（get_ticker / get_orderbook / get_candles / ...）
//! - account：账户与持仓查询（get_balance / get_positions / get_trade_history / ...）
//! - strategy：策略管理（list_strategies / start_strategy / stop_strategy / ...）
//! - risk：风险管理（get_risk_metrics / set_stop_loss / check_margin / ...）
//! - backtest：回测管理（run_backtest / get_backtest_result / list_backtests / ...）

pub mod trading;
pub mod ui;
pub mod market;
pub mod account;
pub mod strategy;
pub mod risk;
pub mod backtest;

pub use trading::TradingTools;
pub use ui::UITools;
pub use market::MarketTools;
pub use account::AccountTools;
pub use strategy::StrategyTools;
pub use risk::RiskTools;
pub use backtest::BacktestTools;
