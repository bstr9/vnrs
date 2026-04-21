---
id: REQ-035
title: "MCP Prompts 模板"
status: completed
created_at: "2026-04-20T00:00:00"
updated_at: "2026-04-20T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-033]
  cluster: MCP
versions:
  - version: 1
    date: "2026-04-20T00:00:00"
    author: ai
    context: "MCP 分析后发�?Prompts 未实现。Prompts 提供可复用的标准化交互模板�?
    reason: "提供标准化分析模板，提升 LLM 交互效率"
    snapshot: "实现 MCP Prompts 能力，提供预定义的交易分析模�?
---

# MCP Prompts 模板

## 描述

**Prompts** �?MCP 的五大原语之一，提供可复用的标准化交互模板。用�?应用可以通过预定义的 prompt 模板快速发起分析请求�?
### MCP Prompts 特�?
- **方向**: 用户/应用控制
- **用�?*: 定义标准化交互模�?- **参数�?*: 支持模板变量 `{{symbol}}`, `{{timeframe}}` �?- **可组�?*: 多个 prompt 可以组合使用

### 参考实现（MetaTrader MCP Server�?
```python
@mcp.prompt(title="Pre-Trade Check")
def pre_trade_check(symbol: str, timeframe: str = "1H") -> str:
    return f"""
    请对 {symbol} 进行交易前检�?
    1. 当前价格�?20 日均线的关系
    2. RSI 指标状�?    3. 成交量变�?    4. 建议的止�?止盈�?    """
```

## 预定�?Prompts

### 交易相关

| Prompt | 描述 | 参数 |
|--------|------|------|
| `pre_trade_check` | 交易前检查清�?| symbol, timeframe |
| `risk_assessment` | 风险评估模板 | symbol, position_size |
| `position_analysis` | 持仓分析模板 | symbol |
| `market_overview` | 市场概览 | symbols[], timeframe |

### 策略相关

| Prompt | 描述 | 参数 |
|--------|------|------|
| `strategy_review` | 策略绩效回顾 | strategy_id, period |
| `backtest_analysis` | 回测结果分析 | backtest_id |
| `parameter_optimization` | 参数优化建议 | strategy_id, current_params |

### 风控相关

| Prompt | 描述 | 参数 |
|--------|------|------|
| `portfolio_risk` | 组合风险评估 | - |
| `margin_check` | 保证金检�?| - |
| `exposure_analysis` | 风险敞口分析 | - |

## 验收标准

- [ ] 实现 `#[prompt]` 宏或等效 API
- [ ] 添加 `list_prompts` MCP 方法
- [ ] 添加 `get_prompt` MCP 方法
- [ ] 实现 5+ 个交易相�?prompt 模板
- [ ] 实现 3+ 个策略相�?prompt 模板
- [ ] 实现 3+ 个风控相�?prompt 模板
- [x] 支持 prompt 参数化和默认�?- [ ] Prompt 列表可从 Claude Desktop 访问

## 示例实现

```rust
#[derive(Clone)]
struct TradingPrompts;

impl TradingPrompts {
    #[prompt(title = "Pre-Trade Check")]
    fn pre_trade_check(&self, symbol: String, timeframe: Option<String>) -> String {
        let tf = timeframe.unwrap_or_else(|| "1H".to_string());
        format!(
            r#"请对 {} 进行交易前检查（{} 周期）：

1. 当前价格�?20 日均线的关系
2. RSI 指标状态（是否超买/超卖�?3. MACD 信号线状�?4. 成交量变化趋�?5. 支撑位和阻力�?6. 建议的止�?止盈�?#,
            symbol, tf
        )
    }

    #[prompt(title = "Risk Assessment")]
    fn risk_assessment(&self, symbol: String, position_size: f64) -> String {
        format!(
            r#"风险评估报告请求�?
品种: {}
持仓�? {}

请分析：
1. 单品种风险敞口占�?2. 波动率风险评�?3. 相关性风险（与其他持仓）
4. 流动性风�?5. 建议的风险调整措�?#,
            symbol, position_size
        )
    }
}
```

## 工作�?
0.5-1 �?
## 设计参�?
- MCP Prompts 规范：https://modelcontextprotocol.io/specification/2025-03-26/server/prompts
- MetaTrader MCP Server prompts 示例
