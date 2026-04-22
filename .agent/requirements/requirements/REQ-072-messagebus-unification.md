---
id: REQ-072
title: "Python/Rust MessageBus 统一——消除双消息系统"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P3
level: story
cluster: Core-Trading
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: []
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 Rust 端 MessageBus（src/trader/message_bus.rs）实现了 BaseEngine trait 但从未注册到 MainEngine，是孤儿模块；Python 端有独立的 python::MessageBus 实现。两套消息系统互不兼容，造成功能冗余和混淆。"
    reason: "录入双消息系统统一需求"
    snapshot: "统一 Rust/Python MessageBus 为一套消息系统，消除冗余和混淆"
---

# Python/Rust MessageBus 统一——消除双消息系统

## 描述

当前存在两套 MessageBus 实现：

1. **Rust MessageBus**（`src/trader/message_bus.rs:197`）：
   - 实现 BaseEngine trait
   - 但从未注册到 MainEngine，是孤儿模块
   - 提供 publish/subscribe 模式

2. **Python MessageBus**（`src/python/`）：
   - Python 端独立实现
   - 不连接 Rust 端 MessageBus
   - 与 MainEngine 事件总线无关

这造成：
- 两个系统功能重叠但互不通信
- Rust 策略和 Python 策略无法通过消息总线互相通信
- 维护成本翻倍

## 验收标准

### 统一方案
- [x] Rust MessageBus 注册到 MainEngine 作为子引擎
- [x] Python MessageBus 改为 Rust MessageBus 的薄封装
- [x] Rust 和 Python 订阅者能收到同一消息
- [x] 迁移 Python 端现有 MessageBus 用户代码

### 向后兼容
- [x] Python MessageBus API 保持不变（内部委托给 Rust）
- [x] 现有 Python 代码无需修改

## 影响范围

- `src/trader/message_bus.rs` — 注册到 MainEngine
- `src/trader/engine.rs` — 注册 MessageBus
- `src/python/` — Python MessageBus 改为 Rust 封装
