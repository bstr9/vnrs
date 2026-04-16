//! MCP Tools 模块入口
//!
//! 包含 trading（后端交易操作）和 ui（前端界面操作）两组工具。

pub mod trading;
pub mod ui;

pub use trading::TradingTools;
pub use ui::UITools;
