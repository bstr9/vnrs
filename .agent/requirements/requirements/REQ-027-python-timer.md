---
id: REQ-027
title: "Python 策略定时器/调度功能"
status: completed
created_at: "2026-04-19T14:00:00"
updated_at: "2026-04-22T22:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-026]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T14:00:00"
    author: ai
    context: "代码分析发现 Rust 端和 Python 端均无定时器功能。nautilus_trader 提供 LiveClock/TestClock + schedule/time_alert 方法，支持策略在特定时间执行操作（如定时调仓、定时报告）。定时调仓是量化交易常见需求。"
    reason: "定时调仓、定时报告、时间条件触发等场景需要定时器支持"
    snapshot: "Python Strategy 添加 schedule_timer/cancel_timer 方法，支持定时回调 on_timer()"
---

# Python 策略定时器/调度功能

## 描述

当前策略只能通过 on_tick/on_bar 事件驱动，无法设置定时回调。常见的定时需求包括：
- 定时调仓（如每日收盘前 5 分钟调整仓位）
- 定时报告（如每小时输出持仓快照）
- 时间条件触发（如开盘后 30 分钟开始交易）
- 超时处理（如挂单后 60 秒未成交自动撤单）

nautilus_trader 提供完整的时钟系统：`LiveClock`/`TestClock` + `schedule`/`cancel_timer`/`time_alert`。

## 验收标准

- [ ] Python `Strategy` 添加 `on_timer(self, timer_id)` 回调
- [ ] Python `Strategy` 添加 `schedule_timer(timer_id, interval_seconds, repeat=True)` 方法
- [ ] Python `Strategy` 添加 `cancel_timer(timer_id)` 方法
- [ ] 回测模式：定时器由回测引擎的虚拟时钟驱动
- [ ] 实盘模式：定时器由 tokio 定时器驱动
- [ ] 支持一次性定时（time_alert）和重复定时（schedule）
- [ ] 定时器到期时触发 `on_timer(timer_id)` 回调

## 影响范围

- `src/python/strategy.rs` — on_timer 回调 + schedule/cancel 方法
- `src/strategy/engine.rs` — 定时器管理
- `src/backtesting/engine.rs` — 虚拟时钟驱动定时器
