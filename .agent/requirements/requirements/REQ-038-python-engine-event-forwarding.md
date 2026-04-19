---
id: REQ-038
title: "PythonEngineWrapper 事件转发断裂"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P0
cluster: Bug-Fix
relations:
  depends_on: []
  related_to: [REQ-022, REQ-026]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 PythonEngineWrapper 的 on_tick/on_bar/on_trade/on_order 方法仅提取 symbol 后立即返回 Ok(())，不转发事件到任何策略"
    reason: "初始发现"
    snapshot: "PythonEngineWrapper 事件处理器不转发事件到策略，实盘路径下 Python 策略收不到行情和成交回调"
---

# PythonEngineWrapper 事件转发断裂

## 描述
`src/python/bindings.rs` 中 `PythonEngineWrapper` 的 `on_tick`、`on_bar`、`on_trade`、`on_order` 四个事件处理方法（lines 241-272）仅提取 `symbol` 字段后立即返回 `Ok(())`，不将事件路由到任何已注册的 Python 策略。这意味着通过 `PythonEngineWrapper`（实盘交易路径）的 Python 策略无法收到行情推送和订单回报。

`PythonEngineBridge`（`src/python/engine.rs`）中有正确的事件转发逻辑，但 PyO3 暴露的 Wrapper 没有使用。

## 验收标准
- [ ] `PythonEngineWrapper.on_tick()` 将 tick 事件转发到已注册策略的 `on_tick` 回调
- [ ] `PythonEngineWrapper.on_bar()` 将 bar 事件转发到已注册策略的 `on_bar` 回调
- [ ] `PythonEngineWrapper.on_trade()` 将 trade 事件转发到已注册策略的 `on_trade` 回调
- [ ] `PythonEngineWrapper.on_order()` 将 order 事件转发到已注册策略的 `on_order` 回调
- [ ] 事件转发与 `PythonEngineBridge` 的转发逻辑保持一致
