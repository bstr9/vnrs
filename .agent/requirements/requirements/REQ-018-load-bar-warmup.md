---
id: REQ-018
title: "Python load_bar() 返回空数据，策略预热不可用"
status: active
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
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
---

# Python load_bar() 返回空数据，策略预热不可用

## 描述

`backtesting_bindings.rs:500-524` 的 `PyBacktestingEngine.load_bar()` 总是返回空的 `Vec<PyBarData>`。Python 策略在 `on_init` 中调用 `load_bar` 来加载历史数据用于指标计算预热（如计算 MA 需要前 N 根 Bar），但始终拿到空列表。

回测引擎内部已有完整的历史数据（用于驱动 on_bar 回调），只需将已缓存的数据暴露给 Python 端即可。

## 验收标准

- [ ] `load_bar()` 从回测引擎的 bar 缓存中返回数据
- [ ] 支持按品种 (vt_symbol) 和天数筛选
- [ ] 返回的 `PyBarData` 包含完整的 OHLCV + 时间信息
- [ ] 同理 `load_tick()` 也应实现（如 tick 数据可用）
- [ ] 回测策略 on_init 中调用 load_bar 能正确拿到历史 Bar
- [ ] 测试：Python 策略 on_init 中 load_bar(5) 返回 5 天的 Bar 数据

## 影响范围

- `src/python/backtesting_bindings.rs` — load_bar() / load_tick() 实现
- `src/backtesting/engine.rs` — 可能需要暴露历史数据访问接口
