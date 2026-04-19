---
id: REQ-005
title: "自成交防范 (STP)"
status: active
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-002, REQ-006]
  cluster: Order-Types
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发现 OrderBook-rs 已实现 STP（CancelTaker/CancelMaker/CancelBoth 三种模式），vnrs 无任何 self_trade/stp 相关代码。高频/多策略场景下自成交是严重合规问题。"
    reason: "多策略运行时自成交防范是合规必需"
    snapshot: "实现自成交防范 (STP)，支持 CancelTaker/CancelMaker/CancelBoth 三种模式"
---

# 自成交防范 (STP)

## 描述

当多个策略同时运行时，可能出现同一账户的买单和卖单互相成交（自成交），这在大多数交易所是违规行为。需要实现自成交防范机制。

参考 OrderBook-rs 的实现：CancelTaker/CancelMaker/CancelBoth 三种模式。

## 验收标准

- [ ] 定义 `StpMode` 枚举：CancelTaker, CancelMaker, CancelBoth
- [ ] 在订单提交前检查是否存在对手方自成交可能
- [ ] 根据 StpMode 决定取消哪一方
- [ ] 可在策略级或全局级配置 StpMode
- [ ] 自成交被阻止时产生告警事件

## 工作量

约 1 天
