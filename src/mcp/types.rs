//! MCP 类型定义：UICommand 枚举、UIState 状态、通道类型
//!
//! 定义 MCP Server 与 UI 线程之间的通信协议，
//! 包括 UI 命令（MCP→UI）和 UI 状态共享（UI→MCP）。

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// UI 命令，由 MCP Server 发送给 UI 线程执行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UICommand {
    // ---- 前端操作 ----
    /// 切换当前交易标的
    SwitchSymbol { symbol: String },
    /// 切换 K 线周期
    SwitchInterval { interval: String },
    /// 添加技术指标到图表
    AddIndicator {
        indicator_type: String,
        period: Option<usize>,
    },
    /// 按索引移除技术指标
    RemoveIndicator { index: usize },
    /// 清除所有技术指标
    ClearIndicators,
    /// 导航到指定标签页
    NavigateTo { tab: String },
    /// 显示通知消息
    ShowNotification { message: String, level: String },

    // ---- 后端操作（通过 MainEngine 执行） ----
    /// 连接到交易所网关
    Connect {
        gateway_name: String,
        settings: serde_json::Value,
    },
    /// 订阅行情
    Subscribe {
        symbol: String,
        exchange: String,
        gateway_name: String,
    },
    /// 发送委托订单
    SendOrder {
        symbol: String,
        exchange: String,
        direction: String,
        order_type: String,
        volume: f64,
        price: Option<f64>,
        offset: Option<String>,
        gateway_name: String,
    },
    /// 撤销委托订单
    CancelOrder {
        order_id: String,
        symbol: String,
        exchange: String,
        gateway_name: String,
    },
}

/// UI 状态，由 UI 线程共享给 MCP Server 读取
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UIState {
    /// 当前选中标的（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub current_symbol: Option<String>,
    /// 当前 K 线周期
    pub current_interval: Option<String>,
    /// 当前活动标签页
    pub active_tab: String,
    /// 图表上已加载的指标列表
    pub chart_indicators: Vec<String>,
}

/// MCP→UI 命令通道发送端类型
pub type UICommandSender = mpsc::UnboundedSender<UICommand>;

/// MCP→UI 命令通道接收端类型
pub type UICommandReceiver = mpsc::UnboundedReceiver<UICommand>;
