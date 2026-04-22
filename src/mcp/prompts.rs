//! MCP Prompts 实现
//!
//! 提供交易相关的标准化、参数化 Prompt 模板，
//! 供 MCP 客户端（如 Claude Desktop）发现和调用。
//!
//! # 支持的 Prompts
//!
//! - `pre_trade_check` — 交易前分析检查清单
//! - `risk_assessment` — 风险评估模板
//! - `position_analysis` — 持仓分析
//! - `market_overview` — 市场概览
//! - `strategy_review` — 策略表现回顾
//! - `backtest_analysis` — 回测结果分析
//! - `parameter_optimization` — 策略参数优化
//! - `portfolio_risk` — 组合风险评估
//! - `margin_check` — 保证金充足率检查
//! - `exposure_analysis` — 风险敞口分析

use rmcp::{
    ErrorData as McpError,
    model::*,
    schemars,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

// ---- 参数结构体（每个 prompt 独立，派生 JsonSchema）----

/// 交易前分析检查清单参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PreTradeCheckParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
    /// 时间周期（如 1m, 5m, 15m, 1h, 4h, 1d）
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
}

fn default_timeframe() -> String {
    "1h".to_string()
}

/// 风险评估参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RiskAssessmentParams {
    /// 标的符号
    pub symbol: String,
    /// 持仓数量
    #[serde(default)]
    pub position_size: Option<f64>,
}

/// 持仓分析参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PositionAnalysisParams {
    /// 标的符号
    pub symbol: String,
}

/// 市场概览参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct MarketOverviewParams {
    /// 标的符号列表，逗号分隔（如 BTCUSDT,ETHUSDT）
    #[serde(default = "default_symbols")]
    pub symbols: String,
    /// 时间周期
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
}

fn default_symbols() -> String {
    "BTCUSDT,ETHUSDT".to_string()
}

/// 策略表现回顾参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct StrategyReviewParams {
    /// 策略ID
    pub strategy_id: String,
    /// 回顾周期（如 1d, 7d, 30d, 90d）
    #[serde(default = "default_period")]
    pub period: String,
}

fn default_period() -> String {
    "30d".to_string()
}

/// 回测结果分析参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct BacktestAnalysisParams {
    /// 回测ID
    pub backtest_id: String,
    /// 关注指标（如 sharpe_ratio, max_drawdown, win_rate）
    #[serde(default)]
    pub metrics: Option<String>,
}

/// 策略参数优化参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ParameterOptimizationParams {
    /// 策略ID
    pub strategy_id: String,
    /// 当前参数（JSON 格式）
    #[serde(default)]
    pub current_params: Option<String>,
}

/// 组合风险评估参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PortfolioRiskParams {
    /// 标的符号列表，逗号分隔（如 BTCUSDT,ETHUSDT）
    #[serde(default = "default_symbols")]
    pub symbols: String,
}

/// 保证金充足率检查参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct MarginCheckParams {}

/// 风险敞口分析参数
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ExposureAnalysisParams {
    /// 标的符号（可选，为空则分析全仓）
    #[serde(default)]
    pub symbol: Option<String>,
}

// ---- Prompt 定义和处理器 ----

/// 返回所有可用的 Prompt 定义列表
pub fn list_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "pre_trade_check",
            Some("交易前分析检查清单 — 对指定标的和时间周期进行全面的交易前分析"),
            Some(vec![
                PromptArgument::new("symbol")
                    .with_title("标的符号")
                    .with_description("交易标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）")
                    .with_required(true),
                PromptArgument::new("timeframe")
                    .with_title("时间周期")
                    .with_description("分析时间周期（1m/5m/15m/1h/4h/1d）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "risk_assessment",
            Some("风险评估模板 — 评估指定标的和持仓的风险水平"),
            Some(vec![
                PromptArgument::new("symbol")
                    .with_title("标的符号")
                    .with_description("交易标的符号")
                    .with_required(true),
                PromptArgument::new("position_size")
                    .with_title("持仓数量")
                    .with_description("当前持仓数量（未提供则仅做通用风险评估）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "position_analysis",
            Some("持仓分析 — 对指定标的的持仓进行全面分析"),
            Some(vec![
                PromptArgument::new("symbol")
                    .with_title("标的符号")
                    .with_description("交易标的符号")
                    .with_required(true),
            ]),
        ),
        Prompt::new(
            "market_overview",
            Some("市场概览 — 多标的、多时间维度的市场整体概览"),
            Some(vec![
                PromptArgument::new("symbols")
                    .with_title("标的列表")
                    .with_description("逗号分隔的标的符号列表（默认 BTCUSDT,ETHUSDT）")
                    .with_required(false),
                PromptArgument::new("timeframe")
                    .with_title("时间周期")
                    .with_description("分析时间周期")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "strategy_review",
            Some("策略表现回顾 — 评估指定策略在特定周期内的表现"),
            Some(vec![
                PromptArgument::new("strategy_id")
                    .with_title("策略ID")
                    .with_description("策略唯一标识符")
                    .with_required(true),
                PromptArgument::new("period")
                    .with_title("回顾周期")
                    .with_description("回顾时间范围（1d/7d/30d/90d，默认30d）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "backtest_analysis",
            Some("回测结果分析 — 对指定回测的运行结果进行深度分析"),
            Some(vec![
                PromptArgument::new("backtest_id")
                    .with_title("回测ID")
                    .with_description("回测运行唯一标识符")
                    .with_required(true),
                PromptArgument::new("metrics")
                    .with_title("关注指标")
                    .with_description("关注的指标列表（如 sharpe_ratio, max_drawdown, win_rate）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "parameter_optimization",
            Some("策略参数优化 — 基于当前参数建议优化方向"),
            Some(vec![
                PromptArgument::new("strategy_id")
                    .with_title("策略ID")
                    .with_description("策略唯一标识符")
                    .with_required(true),
                PromptArgument::new("current_params")
                    .with_title("当前参数")
                    .with_description("当前策略参数（JSON 格式）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "portfolio_risk",
            Some("组合风险评估 — 评估整体持仓组合的风险水平"),
            Some(vec![
                PromptArgument::new("symbols")
                    .with_title("标的列表")
                    .with_description("逗号分隔的标的符号列表（默认 BTCUSDT,ETHUSDT）")
                    .with_required(false),
            ]),
        ),
        Prompt::new(
            "margin_check",
            Some("保证金充足率检查 — 检查当前账户保证金是否充足"),
            Some(vec![]),
        ),
        Prompt::new(
            "exposure_analysis",
            Some("风险敞口分析 — 分析当前持仓的风险敞口和集中度"),
            Some(vec![
                PromptArgument::new("symbol")
                    .with_title("标的符号")
                    .with_description("分析指定标的的敞口（为空则分析全仓）")
                    .with_required(false),
            ]),
        ),
    ]
}

/// 获取指定 Prompt 的内容，解析参数后返回消息列表
pub fn get_prompt(
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<GetPromptResult, McpError> {
    match name {
        "pre_trade_check" => {
            let params = parse_args::<PreTradeCheckParams>(arguments)?;
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对标的 {} 在 {} 周期进行交易前分析检查，包括以下方面：\n\n\
                         1. **市场趋势判断**：当前主要趋势方向、支撑/阻力位\n\
                         2. **成交量分析**：近期成交量变化、量价配合情况\n\
                         3. **技术指标信号**：MACD/RSI/布林带等主要指标状态\n\
                         4. **资金管理**：建议仓位比例、止损位设置\n\
                         5. **风险因素**：近期重要事件、波动率水平\n\
                         6. **交易决策建议**：是否适合开仓、建议方向和入场价位\n\n\
                         请基于当前可获取的数据给出详细分析。",
                        params.symbol, params.timeframe
                    ),
                ),
            ]))
        }
        "risk_assessment" => {
            let params = parse_args::<RiskAssessmentParams>(arguments)?;
            let position_info = match params.position_size {
                Some(size) => format!("当前持仓数量：{}", size),
                None => "未提供持仓数量，仅做通用风险评估".to_string(),
            };
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对标的 {} 进行风险评估，{}。\n\n\
                         请从以下维度进行评估：\n\n\
                         1. **市场风险**：\n\
                            - 当前波动率水平和历史对比\n\
                            - 最大回撤风险估计\n\
                            - 极端行情（闪崩/暴涨）概率\n\n\
                         2. **流动性风险**：\n\
                            - 订单簿深度分析\n\
                            - 滑点预估\n\
                            - 大额订单冲击成本\n\n\
                         3. **杠杆风险**（如适用）：\n\
                            - 强平价格计算\n\
                            - 保证金充足率\n\
                            - 爆仓风险等级\n\n\
                         4. **相关性风险**：\n\
                            - 与其他持仓的相关性\n\
                            - 系统性风险暴露\n\n\
                         5. **综合风险评级**：低/中/高/极高\n\
                         6. **风险缓解建议**",
                        params.symbol, position_info
                    ),
                ),
            ]))
        }
        "position_analysis" => {
            let params = parse_args::<PositionAnalysisParams>(arguments)?;
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对标的 {} 的持仓进行全面分析：\n\n\
                         1. **持仓概览**：\n\
                            - 当前持仓方向和多空数量\n\
                            - 开仓均价和当前价格\n\
                            - 浮动盈亏和盈亏比\n\n\
                         2. **持仓风险评估**：\n\
                            - 当前止损位是否合理\n\
                            - 最大可能亏损\n\
                            - 盈亏比是否达标\n\n\
                         3. **技术面分析**：\n\
                            - 当前趋势是否支持持仓方向\n\
                            - 关键技术位和可能的反转信号\n\
                            - 成交量变化对持仓的影响\n\n\
                         4. **操作建议**：\n\
                            - 是否继续持有\n\
                            - 止损/止盈调整建议\n\
                            - 加仓/减仓时机",
                        params.symbol
                    ),
                ),
            ]))
        }
        "market_overview" => {
            let params = parse_args::<MarketOverviewParams>(arguments)?;
            let symbols_list: Vec<&str> = params.symbols.split(',').map(|s| s.trim()).collect();
            let symbols_display = symbols_list.join("、");
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请提供以下标的市场概览（{} 周期）：{}\n\n\
                         对每个标的，请分析：\n\n\
                         1. **价格走势**：\n\
                            - 当前价格和涨跌幅\n\
                            - 近期走势特征（趋势/震荡/突破）\n\
                            - 关键支撑和阻力位\n\n\
                         2. **市场情绪**：\n\
                            - 多空力量对比\n\
                            - 资金流向\n\
                            - 恐惧/贪婪指数\n\n\
                         3. **板块联动**：\n\
                            - 标的之间的相关性\n\
                            - 领涨/领跌品种\n\
                            - 整体市场方向判断\n\n\
                         4. **操作机会**：\n\
                            - 最值得关注的机会\n\
                            - 需要回避的风险\n\
                            - 建议关注的入场时机",
                        params.timeframe, symbols_display
                    ),
                ),
            ]))
        }
        "strategy_review" => {
            let params = parse_args::<StrategyReviewParams>(arguments)?;
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对策略 {} 在过去 {} 的表现进行全面回顾：\n\n\
                         1. **收益表现**：\n\
                            - 累计收益率和年化收益率\n\
                            - 超额收益（相对于基准）\n\
                            - 收益率分布特征\n\n\
                         2. **风险指标**：\n\
                            - 最大回撤和回撤持续时间\n\
                            - 夏普比率和索提诺比率\n\
                            - 波动率和下行风险\n\n\
                         3. **交易统计**：\n\
                            - 交易次数和胜率\n\
                            - 平均盈亏比\n\
                            - 持仓时间分布\n\n\
                         4. **策略稳定性**：\n\
                            - 不同市场环境下的表现差异\n\
                            - 参数敏感性分析\n\
                            - 是否存在过拟合风险\n\n\
                         5. **改进建议**：\n\
                            - 当前策略的主要问题\n\
                            - 优化方向和参数调整建议\n\
                            - 风险控制改进方案",
                        params.strategy_id, params.period
                    ),
                ),
            ]))
        }
        "backtest_analysis" => {
            let params = parse_args::<BacktestAnalysisParams>(arguments)?;
            let metrics_info = match &params.metrics {
                Some(m) => format!("请重点关注以下指标：{}", m),
                None => "请对所有核心指标进行全面分析".to_string(),
            };
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对回测 {} 的运行结果进行深度分析。{}\n\n\
                         1. **收益分析**：\n\
                            - 总收益率和年化收益率\n\
                            - 基准对比和超额收益\n\
                            - 收益曲线特征（平稳/波动/阶梯式）\n\n\
                         2. **风险指标**：\n\
                            - 最大回撤及其持续时间\n\
                            - 夏普比率和索提诺比率\n\
                            - Calmar 比率\n\
                            - 日/周/月度波动率\n\n\
                         3. **交易质量**：\n\
                            - 总交易次数和胜率\n\
                            - 平均盈利交易 vs 平均亏损交易\n\
                            - 盈亏比和期望值\n\
                            - 连续亏损最大次数\n\n\
                         4. **回测可靠性**：\n\
                            - 样本量是否充足\n\
                            - 是否存在存活偏差\n\
                            - 滑点和手续费假设是否合理\n\
                            - 参数过拟合风险评估\n\n\
                         5. **优化建议**：\n\
                            - 策略改进方向\n\
                            - 参数调整建议\n\
                            - 风险管理优化方案",
                        params.backtest_id, metrics_info
                    ),
                ),
            ]))
        }
        "parameter_optimization" => {
            let params = parse_args::<ParameterOptimizationParams>(arguments)?;
            let params_info = match &params.current_params {
                Some(p) => format!("当前参数：{}", p),
                None => "未提供当前参数，请先获取策略参数后再进行分析".to_string(),
            };
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请为策略 {} 提供参数优化建议。{}\n\n\
                         1. **当前参数评估**：\n\
                            - 各参数的当前设置是否合理\n\
                            - 参数是否处于最优区间\n\
                            - 参数之间的交互影响\n\n\
                         2. **优化方向**：\n\
                            - 基于历史表现建议的参数调整\n\
                            - 不同市场环境下的参数适配\n\
                            - 参数步进方向和步长建议\n\n\
                         3. **鲁棒性分析**：\n\
                            - 参数在小幅变动时的表现稳定性\n\
                            - 避免过拟合的参数约束\n\
                            - 参数安全边界建议\n\n\
                         4. **推荐参数组合**：\n\
                            - 保守型参数配置\n\
                            - 均衡型参数配置\n\
                            - 激进型参数配置\n\n\
                         5. **风险评估**：\n\
                            - 参数调整后的预期风险变化\n\
                            - 极端行情下的表现预期\n\
                            - 建议的风险控制配套措施",
                        params.strategy_id, params_info
                    ),
                ),
            ]))
        }
        "portfolio_risk" => {
            let params = parse_args::<PortfolioRiskParams>(arguments)?;
            let symbols_list: Vec<&str> = params.symbols.split(',').map(|s| s.trim()).collect();
            let symbols_display = symbols_list.join("、");
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "请对以下持仓组合进行整体风险评估：{}\n\n\
                         1. **组合风险概览**：\n\
                            - 组合总敞口和净敞口\n\
                            - 杠杆使用情况\n\
                            - 组合整体风险等级\n\n\
                         2. **相关性分析**：\n\
                            - 标的之间的相关系数矩阵\n\
                            - 高相关性标的的重叠风险\n\
                            - 分散化效果评估\n\n\
                         3. **VaR 和压力测试**：\n\
                            - 95%/99% VaR 估计\n\
                            - 历史最大亏损情景\n\
                            - 极端市场条件下的损失预估\n\n\
                         4. **集中度风险**：\n\
                            - 单一标的集中度\n\
                            - 方向集中度（多/空）\n\
                            - 板块/行业集中度\n\n\
                         5. **流动性风险**：\n\
                            - 组合整体流动性评估\n\
                            - 清仓时间预估\n\
                            - 滑点成本预估\n\n\
                         6. **风险缓解建议**：\n\
                            - 对冲建议\n\
                            - 仓位调整建议\n\
                            - 止损/风控策略建议",
                        symbols_display
                    ),
                ),
            ]))
        }
        "margin_check" => {
            let _params = parse_args::<MarginCheckParams>(arguments)?;
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    "请检查当前账户的保证金充足率：\n\n\
                     1. **保证金概览**：\n\
                        - 总保证金和可用保证金\n\
                        - 已用保证金和保证金使用率\n\
                        - 维持保证金要求\n\n\
                     2. **强平风险**：\n\
                        - 各持仓的强平价格\n\
                        - 距离强平的安全距离\n\
                        - 强平风险等级（安全/注意/危险）\n\n\
                     3. **杠杆分析**：\n\
                        - 各持仓的实际杠杆\n\
                        - 有效杠杆和风险杠杆\n\
                        - 杠杆合理性评估\n\n\
                     4. **压力情景**：\n\
                        - 价格下跌 10%/20%/30% 时的保证金状态\n\
                        - 极端波动下的保证金需求\n\
                        - 需要追加保证金的触发条件\n\n\
                     5. **建议**：\n\
                        - 是否需要降低杠杆\n\
                        - 是否需要追加保证金\n\
                        - 仓位调整建议".to_string(),
                ),
            ]))
        }
        "exposure_analysis" => {
            let params = parse_args::<ExposureAnalysisParams>(arguments)?;
            let symbol_info = match &params.symbol {
                Some(s) => format!("请重点分析标的 {} 的风险敞口，并兼顾全仓情况。", s),
                None => "请分析所有持仓的整体风险敞口。".to_string(),
            };
            Ok(GetPromptResult::new(vec![
                PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "{}\n\n\
                         1. **敞口概览**：\n\
                            - 多空敞口分布\n\
                            - 净敞口和总敞口\n\
                            - 敞口占总资金比例\n\n\
                         2. **方向风险**：\n\
                            - 多头 vs 空头敞口对比\n\
                            - 净方向敞口风险\n\
                            - 方向集中度评估\n\n\
                         3. **敏感性分析**：\n\
                            - 价格变动 1% 对组合的影响\n\
                            - 各标的的 Delta 敞口\n\
                            - 敞口对波动率的敏感度\n\n\
                         4. **跨市场敞口**：\n\
                            - 现货 vs 合约敞口\n\
                            - 不同交易所的敞口分布\n\
                            - 资金费率对敞口的影响\n\n\
                         5. **敞口优化建议**：\n\
                            - 敞口再平衡建议\n\
                            - 对冲策略建议\n\
                            - 风险限额建议",
                        symbol_info
                    ),
                ),
            ]))
        }
        _ => Err(McpError::invalid_params(
            format!("未知的 prompt: '{}'", name),
            Some(json!({
                "available_prompts": list_prompts().iter().map(|p| &p.name).collect::<Vec<_>>()
            })),
        )),
    }
}

/// 从 JSON 参数映射中解析为指定类型
fn parse_args<T: serde::de::DeserializeOwned>(
    arguments: Option<Map<String, Value>>,
) -> Result<T, McpError> {
    let params = if let Some(args_map) = arguments {
        let args_value = Value::Object(args_map);
        serde_json::from_value::<T>(args_value).map_err(|e| {
            McpError::invalid_params(
                format!("参数解析失败: {}", e),
                None,
            )
        })?
    } else {
        // 尝试从空对象反序列化（用于所有字段都有默认值的情况）
        serde_json::from_value::<T>(Value::Object(Map::new())).map_err(|e| {
            McpError::invalid_params(
                format!("缺少必需参数: {}", e),
                None,
            )
        })?
    };
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_prompts_returns_all() {
        let prompts = list_prompts();
        assert_eq!(prompts.len(), 10);

        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"pre_trade_check"));
        assert!(names.contains(&"risk_assessment"));
        assert!(names.contains(&"position_analysis"));
        assert!(names.contains(&"market_overview"));
        assert!(names.contains(&"strategy_review"));
        assert!(names.contains(&"backtest_analysis"));
        assert!(names.contains(&"parameter_optimization"));
        assert!(names.contains(&"portfolio_risk"));
        assert!(names.contains(&"margin_check"));
        assert!(names.contains(&"exposure_analysis"));
    }

    #[test]
    fn test_list_prompts_has_descriptions() {
        let prompts = list_prompts();
        for prompt in &prompts {
            assert!(
                prompt.description.is_some(),
                "Prompt '{}' 缺少描述",
                prompt.name
            );
        }
    }

    #[test]
    fn test_list_prompts_has_arguments() {
        let prompts = list_prompts();
        for prompt in &prompts {
            // Arguments list should exist (but may be empty for no-arg prompts like margin_check)
            assert!(
                prompt.arguments.is_some(),
                "Prompt '{}' 缺少参数定义",
                prompt.name
            );
        }
    }

    #[test]
    fn test_get_prompt_pre_trade_check() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("BTCUSDT.BINANCE"));
        args.insert("timeframe".to_string(), json!("4h"));

        let result = get_prompt("pre_trade_check", Some(args)).unwrap();
        assert!(!result.messages.is_empty());

        let msg = &result.messages[0];
        if let PromptMessageContent::Text { text } = &msg.content {
            assert!(text.contains("BTCUSDT.BINANCE"));
            assert!(text.contains("4h"));
            assert!(text.contains("市场趋势判断"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_pre_trade_check_default_timeframe() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("ETHUSDT.BINANCE"));

        let result = get_prompt("pre_trade_check", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("ETHUSDT.BINANCE"));
            assert!(text.contains("1h")); // 默认值
        }
    }

    #[test]
    fn test_get_prompt_risk_assessment_with_position() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("BTCUSDT.BINANCE"));
        args.insert("position_size".to_string(), json!(1.5));

        let result = get_prompt("risk_assessment", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT.BINANCE"));
            assert!(text.contains("1.5"));
            assert!(text.contains("风险评估"));
        }
    }

    #[test]
    fn test_get_prompt_risk_assessment_without_position() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("BTCUSDT.BINANCE"));

        let result = get_prompt("risk_assessment", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("通用风险评估"));
        }
    }

    #[test]
    fn test_get_prompt_position_analysis() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("ETHUSDT.BINANCE"));

        let result = get_prompt("position_analysis", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("ETHUSDT.BINANCE"));
            assert!(text.contains("持仓概览"));
        }
    }

    #[test]
    fn test_get_prompt_market_overview() {
        let mut args = Map::new();
        args.insert("symbols".to_string(), json!("BTCUSDT,ETHUSDT,SOLUSDT"));
        args.insert("timeframe".to_string(), json!("1d"));

        let result = get_prompt("market_overview", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT"));
            assert!(text.contains("ETHUSDT"));
            assert!(text.contains("SOLUSDT"));
            assert!(text.contains("1d"));
        }
    }

    #[test]
    fn test_get_prompt_market_overview_defaults() {
        // 无参数时应使用默认值
        let result = get_prompt("market_overview", None).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT") || text.contains("ETHUSDT"));
        }
    }

    #[test]
    fn test_get_prompt_strategy_review() {
        let mut args = Map::new();
        args.insert("strategy_id".to_string(), json!("dual_ma_v2"));
        args.insert("period".to_string(), json!("7d"));

        let result = get_prompt("strategy_review", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("dual_ma_v2"));
            assert!(text.contains("7d"));
            assert!(text.contains("收益表现"));
        }
    }

    #[test]
    fn test_get_prompt_unknown_returns_error() {
        let result = get_prompt("nonexistent_prompt", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_prompt_missing_required_param() {
        // pre_trade_check 需要 symbol 参数
        let result = get_prompt("pre_trade_check", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_prompt_arguments_have_required_flags() {
        let prompts = list_prompts();
        let pre_trade = prompts.iter().find(|p| p.name == "pre_trade_check").unwrap();
        let args = pre_trade.arguments.as_ref().unwrap();

        let symbol_arg = args.iter().find(|a| a.name == "symbol").unwrap();
        assert_eq!(symbol_arg.required, Some(true));

        let timeframe_arg = args.iter().find(|a| a.name == "timeframe").unwrap();
        assert_eq!(timeframe_arg.required, Some(false));
    }

    #[test]
    fn test_get_prompt_backtest_analysis() {
        let mut args = Map::new();
        args.insert("backtest_id".to_string(), json!("bt_001"));
        args.insert("metrics".to_string(), json!("sharpe_ratio,max_drawdown"));

        let result = get_prompt("backtest_analysis", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("bt_001"));
            assert!(text.contains("sharpe_ratio,max_drawdown"));
            assert!(text.contains("收益分析"));
            assert!(text.contains("回测可靠性"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_backtest_analysis_no_metrics() {
        let mut args = Map::new();
        args.insert("backtest_id".to_string(), json!("bt_002"));

        let result = get_prompt("backtest_analysis", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("bt_002"));
            assert!(text.contains("核心指标"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_parameter_optimization() {
        let mut args = Map::new();
        args.insert("strategy_id".to_string(), json!("dual_ma_v3"));
        args.insert("current_params".to_string(), json!(r#"{"fast_period":10,"slow_period":30}"#));

        let result = get_prompt("parameter_optimization", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("dual_ma_v3"));
            assert!(text.contains("fast_period"));
            assert!(text.contains("参数评估"));
            assert!(text.contains("鲁棒性分析"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_parameter_optimization_no_params() {
        let mut args = Map::new();
        args.insert("strategy_id".to_string(), json!("rsi_strategy"));

        let result = get_prompt("parameter_optimization", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("rsi_strategy"));
            assert!(text.contains("未提供当前参数"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_portfolio_risk() {
        let mut args = Map::new();
        args.insert("symbols".to_string(), json!("BTCUSDT,ETHUSDT,SOLUSDT"));

        let result = get_prompt("portfolio_risk", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT"));
            assert!(text.contains("ETHUSDT"));
            assert!(text.contains("SOLUSDT"));
            assert!(text.contains("相关性分析"));
            assert!(text.contains("VaR"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_portfolio_risk_defaults() {
        let result = get_prompt("portfolio_risk", None).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT") || text.contains("ETHUSDT"));
            assert!(text.contains("组合风险"));
        }
    }

    #[test]
    fn test_get_prompt_margin_check() {
        let result = get_prompt("margin_check", None).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("保证金"));
            assert!(text.contains("强平"));
            assert!(text.contains("杠杆"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_exposure_analysis_with_symbol() {
        let mut args = Map::new();
        args.insert("symbol".to_string(), json!("BTCUSDT.BINANCE"));

        let result = get_prompt("exposure_analysis", Some(args)).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("BTCUSDT.BINANCE"));
            assert!(text.contains("敞口概览"));
            assert!(text.contains("敏感性分析"));
        } else {
            panic!("期望文本内容");
        }
    }

    #[test]
    fn test_get_prompt_exposure_analysis_no_symbol() {
        let result = get_prompt("exposure_analysis", None).unwrap();
        if let PromptMessageContent::Text { text } = &result.messages[0].content {
            assert!(text.contains("所有持仓"));
            assert!(text.contains("敞口"));
        } else {
            panic!("期望文本内容");
        }
    }
}
