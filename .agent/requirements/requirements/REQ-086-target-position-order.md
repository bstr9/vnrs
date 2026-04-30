---
id: REQ-086
title: "目标仓位下单模式（Target Position）"
status: completed
level: story
priority: P1
cluster: strategy-execution
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-083]
  refines: []
  related_to: [REQ-063, REQ-064]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "很多量化策略的输出是'目标仓位'而非'交易动作'（如信号强度0.8→目标仓位80%）。当前策略必须手动计算差额下单，容易出错。"
    reason: "目标仓位模式可避免重复开仓、方向错误等常见bug，是量化策略最常用的下单方式"
    snapshot: "策略设置目标仓位后引擎自动计算差额并拆单执行，支持限价和市价两种执行方式"
---

# 目标仓位下单模式（Target Position）

## 描述
许多量化策略的信号输出是"目标仓位"（如信号强度0.8 → 目标持仓80%），而不是具体的买卖动作。当前策略必须手动计算：当前持仓 - 目标持仓 = 需要交易的数量，然后判断方向（买/卖）并下单。

需要实现：
1. `set_target_position(vt_symbol, target_volume)` — 设置目标仓位
2. 引擎自动计算 `delta = target - current_position`
3. 如果 `delta > 0`：买入 `delta` 量；如果 `delta < 0`：卖出 `|delta|` 量
4. 支持限价执行（指定价格或使用当前买一/卖一价）和市价执行
5. 大额订单自动拆分（Iceberg/Time-sliced），避免冲击市场
6. 目标仓位变更时，自动取消之前的未成交差额单，重新计算

## 验收标准
- [x] 策略调用 `set_target_position(vt_symbol, 10.0)` 后，引擎自动计算差额并下单
- [x] 当前持仓5、目标10 → 自动买入5
- [x] 当前持仓10、目标3 → 自动卖出7
- [x] 当前持仓0、目标0 → 不发单
- [x] 支持限价执行：使用买一价买入、卖一价卖出
- [x] 支持大单拆分：超过阈值时拆成多笔小单
- [x] 目标仓位变更时自动取消旧的未成交单，重新计算差额
