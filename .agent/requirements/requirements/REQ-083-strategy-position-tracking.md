---
id: REQ-083
title: "策略仓位与盈亏实时跟踪"
status: completed
level: story
priority: P0
cluster: strategy-execution
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: []
  refines: []
  related_to: [REQ-056, REQ-065]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "策略需要实时知道自己的持仓和盈亏来做决策。当前StrategyEngine有get_strategy_position/get_strategy_pnl，但策略内部通过BaseStrategy.pos dict跟踪，与实际成交回报可能不同步。"
    reason: "仓位是策略决策的核心输入，如果仓位不准会导致重复开仓或反向错误"
    snapshot: "策略能可靠获取自己的实时持仓、成交均价、浮动盈亏和已实现盈亏"
---

# 策略仓位与盈亏实时跟踪

## 描述
量化策略需要准确跟踪自己的持仓状态来做决策（加仓、减仓、止盈止损）。当前问题：

1. `BaseStrategy.pos` 是策略自己维护的字典，与交易所实际成交回报可能不同步
2. 策略的 `on_trade` 回调需要正确更新持仓
3. 缺少成交均价（average entry price）的计算
4. 缺少浮动盈亏（unrealized PnL）的实时计算
5. 缺少已实现盈亏（realized PnL）的累计跟踪

需要实现：
- 每个 `vt_symbol` 的持仓方向、数量、成交均价
- 实时浮动盈亏 = (当前价 - 成交均价) × 持仓量 × 合约乘数
- 已实现盈亏 = 每次平仓的 (平仓价 - 开仓均价) × 平仓量
- 仓位变化时触发 `on_position_update` 回调

## 验收标准
- [x] 策略 `on_trade` 回调后，`get_position(vt_symbol)` 返回准确的持仓数量
- [x] 做多：买入增加持仓，卖出减少持仓；做空：卖出增加持仓，买入减少持仓
- [x] 支持计算成交均价（加权平均）
- [x] 支持实时浮动盈亏计算 `get_unrealized_pnl(vt_symbol)`
- [x] 支持已实现盈亏累计 `get_realized_pnl()`
- [x] 仓位从非零变零时（平仓完毕），自动重置成交均价
- [x] 与 OmsEngine 中的实际持仓数据定期对账，发现偏差时告警
