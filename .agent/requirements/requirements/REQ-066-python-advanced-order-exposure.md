---
id: REQ-066
title: "高级订单类型暴露——Python API + GUI 面板（模拟单/止损单引擎管理）"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T21:00:00"
priority: P2
level: story
cluster: Core-Trading
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-025]
  merged_from: []
  refined_by: []
  related_to: [REQ-064, REQ-063, REQ-062, REQ-067, REQ-075]
  depends_on: [REQ-064]
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现三大引擎在 Python 绑定中完全无暴露。"
    reason: "录入 Python 高级订单类型暴露需求"
    snapshot: "Python API 暴露 StopOrderEngine/OrderEmulator/BracketOrderEngine 的完整管理接口"
  - version: 2
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认 GUI 是主要使用入口，所有功能都需要 GUI 集成。模拟单和止损单引擎管理面板需要加入 GUI。优先级调整为 P2（专业交易员功能），组合单独立为 REQ-075。"
    reason: "补充 GUI 面板需求，调整优先级"
    snapshot: "Python API + GUI 面板暴露 OrderEmulator 和 StopOrderEngine 管理接口"
---

# Python 高级订单类型暴露——止损单/模拟单/组合单 Python API

## 描述

Python 绑定（`src/python/bindings.rs`）当前暴露了基本的交易功能（buy/sell/short/cover、send_order、cancel_order），但三大高级订单引擎完全无 Python 接口：

1. **StopOrderEngine**：Python 策略可通过 `send_stop_order()` 发送止损单，但无法直接管理 StopOrderEngine（查询、批量撤销、设置追踪止损参数等）
2. **OrderEmulator**：追踪止损/止损限价/冰山单/MIT/LIT——Python 完全无法使用
3. **BracketOrderEngine**：Bracket/OCO/OTO 组合单——Python 完全无法使用

### 已有 vs 缺失

| 功能 | Python 现状 | 需要 |
|------|------------|------|
| send_stop_order() | ✅ BaseStrategy 有 | - |
| cancel_stop_order() | ✅ BaseStrategy 有 | - |
| on_stop_order 回调 | ✅ Python 可覆盖 | - |
| StopOrderEngine 直接管理 | ❌ | 查询/批量撤销 |
| TrailingStop 追踪止损 | ❌ | OrderEmulator API |
| StopLimit 止损限价 | ❌ | OrderEmulator API |
| Iceberg 冰山单 | ❌ | OrderEmulator API |
| MIT 触价单 | ❌ | OrderEmulator API |
| LIT 触价限价单 | ❌ | OrderEmulator API |
| Bracket 组合单 | ❌ | BracketOrderEngine API |
| OCO 组合单 | ❌ | BracketOrderEngine API |
| OTO 组合单 | ❌ | BracketOrderEngine API |

## 验收标准

### OrderEmulator Python 接口
- [ ] `PyOrderEmulator` 类暴露 `add_trailing_stop_pct()` 方法
- [ ] `PyOrderEmulator` 类暴露 `add_trailing_stop_abs()` 方法
- [ ] `PyOrderEmulator` 类暴露 `add_stop_limit()` 方法
- [ ] `PyOrderEmulator` 类暴露 `add_iceberg()` 方法
- [ ] `PyOrderEmulator` 类暴露 `add_mit()` 方法
- [ ] `PyOrderEmulator` 类暴露 `add_lit()` 方法
- [ ] `PyOrderEmulator` 类暴露 `cancel_order()` 方法
- [ ] `PyOrderEmulator` 类暴露 `get_active_orders()` 查询方法
- [ ] Python 通过 `engine.order_emulator` 访问

### BracketOrderEngine Python 接口
- [ ] `PyBracketOrderEngine` 类暴露 `add_bracket_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `add_oco_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `add_oto_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `cancel_group()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `get_active_groups()` 查询方法
- [ ] `PyBracketOrderEngine` 类暴露 `set_state_change_callback()` 回调设置
- [ ] Python 通过 `engine.bracket_order_engine` 访问

### StopOrderEngine Python 接口（补充）
- [ ] `PyStopOrderEngine` 类暴露 `get_active_stop_orders()` 查询
- [ ] `PyStopOrderEngine` 类暴露 `cancel_stop_order()` 直接管理
- [ ] `PyStopOrderEngine` 类暴露 `cancel_orders_for_symbol()` 批量撤销
- [ ] Python 通过 `engine.stop_order_engine` 访问

### 集成要求
- [ ] 依赖 REQ-064 完成回调绑定后，Python API 才能正常工作
- [ ] Python 示例：添加使用 OrderEmulator 和 BracketOrderEngine 的示例

## 影响范围

- `src/python/bindings.rs` — PyO3 模块定义，添加新类
- `src/python/` — 可能需要新增 `order_emulator.rs`、`bracket_order.rs`、`stop_order_engine.rs` 绑定文件
- `examples/` — Python 使用高级订单的示例
