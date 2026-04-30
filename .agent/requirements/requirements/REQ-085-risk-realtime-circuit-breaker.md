---
id: REQ-085
title: "风控规则实时生效与策略熔断"
status: completed
level: story
priority: P0
cluster: risk
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-065]
  refines: []
  related_to: [REQ-083, REQ-084]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "RiskManager已有丰富规则（max_order_count, max_daily_loss, max_position等）且已接入send_order流程，但缺少：1)运行时动态修改风控参数 2)策略级独立风控 3)自动熔断（暂停策略或全部交易）"
    reason: "实盘交易必须能实时调整风控，日亏超限必须自动停止，否则可能导致灾难性亏损"
    snapshot: "风控规则可运行时调整，日亏损超限自动熔断，策略级风控独立互不影响"
---

# 风控规则实时生效与策略熔断

## 描述
RiskManager 已有完善的风控检查逻辑（max_order_count, max_daily_loss, max_position_per_symbol 等），且在 `send_order` 时已接入。但实际量化交易还需要：

1. **运行时动态修改风控参数** — 当前 `update_config()` 存在但 GUI 上没有入口
2. **策略级独立风控** — 每个策略应有独立的风控限额，互不影响
3. **自动熔断机制** — 当触发风控限制时，不是拒绝单笔订单，而是自动暂停策略或全部交易
4. **每日重置** — `DailyStats` 需要在交易日切换时自动重置
5. **通知机制** — 风控触发时通知策略（`on_risk_alert`）和用户（GUI告警）

## 验收标准
- [x] GUI 上可实时修改风控参数（最大日亏损、最大持仓、最大单量等）
- [x] 修改后立即生效，无需重启
- [x] 每个策略有独立的风控配置（通过 `set_strategy_risk_config` 已存在，需完善）
- [x] 日亏损超过 `max_daily_loss` 时自动暂停策略（熔断），不再发单
- [x] 熔断后 GUI 显示告警，用户可手动恢复
- [x] 每日零点自动重置 `DailyStats`（交易次数、日亏损等计数器）
- [x] 风控触发时调用策略的 `on_risk_alert(reason)` 回调
