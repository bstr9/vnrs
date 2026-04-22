---
id: REQ-036
title: "MCP 工具集扩�?
status: completed
created_at: "2026-04-20T00:00:00"
updated_at: "2026-04-20T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-034]
  related_to: [REQ-020]
  cluster: MCP
versions:
  - version: 1
    date: "2026-04-20T00:00:00"
    author: ai
    context: "MCP 分析后发现当前仅 13 个工具。参�?OKX Agent Trade Kit (140工具)、MetaTrader MCP Server (32工具)，建议扩展到 40+ 个工具。用户确认需要参考项目工具集增强�?
    reason: "扩展 MCP 工具集，覆盖更多交易场景"
    snapshot: "�?MCP 工具�?13 个扩展到 40+ 个，参�?OKX/MetaTrader 分类"
---

# MCP 工具集扩�?
## 描述

当前 MCP Server 仅提�?13 个工具（6 �?Trading + 7 �?UI）。参考成熟的交易 MCP 项目，扩展到 40+ 个工具，覆盖更完整的交易场景�?
### 当前工具�?3个）

| 类别 | 工具 |
|------|------|
| Trading (6) | connect, subscribe, send_order, cancel_order, query_history, list_contracts |
| UI (7) | switch_symbol, switch_interval, add_indicator, remove_indicator, clear_indicators, navigate_to, show_notification |

### 参考：OKX Agent Trade Kit (140工具)

| 模块 | 工具�?| 功能 |
|------|--------|------|
| market | 19 | Ticker, K�? 技术指�?70+), 市场筛�?|
| spot | 13 | 现货下单/撤单/批量 |
| swap | 17 | 永续合约交易/杠杆/追踪止损 |
| futures | 18 | 交割合约/条件�?OCO |
| option | 10 | 期权交易/Greeks |
| account | 14 | 余额/持仓/费率 |
| earn | 23 | 理财/质押/双币�?|
| bot | 10 | 网格/DCA 策略机器�?|
| news | 7 | 加密新闻/情绪分析 |

## 目标工具集（40+�?
### Market 数据�?0个）

| 工具 | 描述 |
|------|------|
| `get_ticker` | 获取实时行情 |
| `get_orderbook` | 获取盘口深度 |
| `get_candles` | 获取K线数�?|
| `get_trades` | 获取最近成�?|
| `get_funding_rate` | 获取资金费率 |
| `get_mark_price` | 获取标记价格 |
| `get_index_price` | 获取指数价格 |
| `get_liquidations` | 获取强平数据 |
| `get_open_interest` | 获取持仓�?|
| `get_ticker_24h` | 获取24小时统计 |

### Trading 交易�?2个）

| 工具 | 描述 | 状�?|
|------|------|------|
| `connect` | 连接交易所 | �?已有 |
| `disconnect` | 断开交易所 | 新增 |
| `subscribe` | 订阅行情 | �?已有 |
| `unsubscribe` | 取消订阅 | 新增 |
| `send_order` | 下单 | �?已有 |
| `cancel_order` | 撤单 | �?已有 |
| `modify_order` | 改单 | 新增 |
| `batch_orders` | 批量下单 | 新增 |
| `close_position` | 平仓 | 新增 |
| `set_leverage` | 设置杠杆 | 新增 |
| `get_order_status` | 查询订单状�?| 新增 |
| `list_contracts` | 列出合约 | �?已有 |

### Account 账户�?个）

| 工具 | 描述 |
|------|------|
| `get_balance` | 获取账户余额 |
| `get_positions` | 获取持仓列表 |
| `get_position` | 获取单个持仓 |
| `get_trade_history` | 获取成交历史 |
| `get_fee_rate` | 获取手续费率 |
| `get_account_summary` | 获取账户汇�?|

### Strategy 策略�?个）

| 工具 | 描述 |
|------|------|
| `list_strategies` | 列出策略实例 |
| `get_strategy_status` | 获取策略状�?|
| `start_strategy` | 启动策略 |
| `stop_strategy` | 停止策略 |
| `pause_strategy` | 暂停策略 |
| `get_strategy_params` | 获取策略参数 |
| `set_strategy_params` | 设置策略参数 |
| `get_strategy_performance` | 获取策略绩效 |

### Risk 风控�?个）

| 工具 | 描述 |
|------|------|
| `get_risk_metrics` | 获取风控指标 |
| `set_stop_loss` | 设置止损 |
| `set_take_profit` | 设置止盈 |
| `check_margin` | 检查保证金 |
| `get_exposure` | 获取风险敞口 |

### Backtest 回测�?个）

| 工具 | 描述 |
|------|------|
| `run_backtest` | 运行回测 |
| `get_backtest_result` | 获取回测结果 |
| `list_backtests` | 列出回测记录 |
| `compare_strategies` | 对比策略绩效 |

### News/Sentiment 分析�?个）

| 工具 | 描述 |
|------|------|
| `get_news` | 获取市场新闻 |
| `analyze_sentiment` | 分析情绪（使�?Sampling�?|
| `get_economic_calendar` | 获取经济日历 |
| `get_market_events` | 获取市场事件 |

## 验收标准

- [x] Market 工具�?0个全部实�?- [x] Trading 工具：从 4 个扩展到 12 �?- [x] Account 工具�?个全部实�?- [x] Strategy 工具�?个全部实�?- [x] Risk 工具�?个全部实�?- [x] Backtest 工具�?个全部实�?- [x] News/Sentiment 工具�?个全部实�?- [x] 工具文档：每个工具有清晰的描述和参数说明
- [x] 错误处理：所有工具有统一的错误响应格�?- [x] 权限控制：敏感操作需要确�?
## 安全设计

参�?OKX Agent Trade Kit �?`--read-only` 模式�?
```rust
pub struct McpConfig {
    pub read_only: bool,  // 只暴露查询工�?    pub allowed_modules: HashSet<String>,  // 模块过滤
    pub rate_limit: RateLimitConfig,  // 速率限制
}
```

## 工作�?
3-5 �?
## 设计参�?
- OKX Agent Trade Kit: https://github.com/okx/agent-trade-kit
- MetaTrader MCP Server: https://github.com/ariadng/metatrader-mcp-server
- CCXT MCP: https://github.com/lazy-dinosaur/ccxt-mcp
