---
id: REQ-053
title: "on_indicator() 回调无事件生产者"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P2
cluster: Python-API
relations:
  depends_on: [REQ-022]
  related_to: [REQ-026]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 template.rs:332 中 on_indicator() 空方法存在但全代码库无任何代码发送 indicator 事件"
    reason: "初始发现"
    snapshot: "StrategyTemplate::on_indicator() 回调已定义但整个代码库无事件生产者，指标事件永远不会触发"
---

# on_indicator() 回调无事件生产者

## 描述
`src/strategy/template.rs:332` 中 `StrategyTemplate::on_indicator()` 方法已定义，但整个代码库中没有任何代码发送 indicator 事件。`StrategyEngine` 不 dispatch indicator 事件，`ArrayManager` 或其他指标计算模块也不产生此类事件。这个回调是死代码。

## 验收标准
- [ ] 在 `StrategyEngine` 中添加 indicator 事件 dispatch 逻辑
- [ ] `ArrayManager` 或指标计算模块在更新指标时发出 indicator 事件
- [ ] Python 策略可通过 `on_indicator(name, value)` 接收指标更新
