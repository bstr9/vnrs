---
id: REQ-018
title: "Python load_bar() 返回空数据，策略预热不可用"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-20T22:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-026]
  related_to: [REQ-020]
  cluster: Bug-Fix
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "代码审查确认 src/python/backtesting_bindings.rs:500-524 PyBacktestingEngine.load_bar() 返回空 Vec<PyBarData>。Python 策略在 on_init 中调用 load_bar 加载历史数据用于指标计算预热，但始终拿到空数据。"
    reason: "Python 策略无法在初始化时加载历史数据，指标计算缺少预热数据"
    snapshot: "Python load_bar() 实现：从回测引擎的历史数据缓存中返回指定品种和天数的 Bar 数据"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "Rust 层已实现 BacktestingEngine::get_history_bars() 和 get_history_ticks()，但 Python 绑定 load_bar() 仍返回空 Vec，未调用新方法"
    reason: "部分修复：Rust 层完成，Python 绑定未接线"
    snapshot: "Rust 层 get_history_bars/get_history_ticks 已实现，Python load_bar() 仍需接线"
  - version: 3
    date: "2026-04-20T22:00:00"
    author: ai
    context: "修复完成：load_bar() 调用 BacktestingEngine::get_history_bars() 并转换为 PyBarData，load_tick() 同样实现，添加 PyBarData::from_rust() 构造器"
    reason: "Bug 修复完成"
    snapshot: "Python load_bar/load_tick 完整实现，策略可在 on_init 中加载历史数据用于预热"
---

# Python load_bar() 返回空数据，策略预热不可用

## 描述

`backtesting_bindings.rs:500-524` 的 `PyBacktestingEngine.load_bar()` 总是返回空的 `Vec<PyBarData>`。Python 策略在 `on_init` 中调用 `load_bar` 来加载历史数据用于指标计算预热（如计算 MA 需要前 N 根 Bar），但始终拿到空列表。

回测引擎内部已有完整的历史数据（用于驱动 on_bar 回调），只需将已缓存的数据暴露给 Python 端即可。

## 验收标准

- [x] `load_bar()` 从回测引擎的 bar 缓存中返回数据（Rust 层 get_history_bars() 已实现，Python 绑定已接线）
- [x] 支持按品种 (vt_symbol) 和天数筛选
- [x] 返回的 `PyBarData` 包含完整的 OHLCV + 时间信息
- [x] 同理 `load_tick()` 也已实现（Rust 层 get_history_ticks() 已实现，Python 绑定已接线）
- [x] 回测策略 on_init 中调用 load_bar 能正确拿到历史 Bar
- [x] 测试：Python 策略 on_init 中 load_bar(5) 返回 5 天的 Bar 数据

## 影响范围

- `src/python/backtesting_bindings.rs` — load_bar() / load_tick() 实现
- `src/backtesting/engine.rs` — 可能需要暴露历史数据访问接口
