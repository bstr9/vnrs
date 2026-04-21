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

# 高级订单类型暴露——Python API + GUI 面板（模拟单/止损单引擎管理）

## 描述

Python 绑定（`src/python/bindings.rs`）当前暴露了基本的交易功能（buy/sell/short/cover、send_order、cancel_order），但高级订单引擎完全无接口：

1. **StopOrderEngine**：Python 策略可通过 `send_stop_order()` 发送止损单，但无法直接管理 StopOrderEngine（查询、批量撤销、设置追踪止损参数等）
2. **OrderEmulator**：追踪止损/止损限价/冰山单/MIT/LIT——Python 完全无法使用
3. **BracketOrderEngine**：已拆分到 REQ-075

GUI 是用户主要入口，因此除了 Python API，还需要 GUI 管理面板。

### 已有 vs 缺失

| 功能 | Python 现状 | GUI 现状 | 需要 |
|------|------------|---------|------|
| send_stop_order() | ✅ BaseStrategy 有 | ✅ REQ-067 覆盖 | - |
| StopOrderEngine 直接管理 | ❌ | ❌ | 查询/批量撤销面板 |
| TrailingStop 追踪止损 | ❌ | ❌ | OrderEmulator API + GUI |
| StopLimit 止损限价 | ❌ | ❌ | OrderEmulator API + GUI |
| Iceberg 冰山单 | ❌ | ❌ | OrderEmulator API + GUI |
| MIT 触价单 | ❌ | ❌ | OrderEmulator API + GUI |
| LIT 触价限价单 | ❌ | ❌ | OrderEmulator API + GUI |

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

### StopOrderEngine Python 接口（补充）
- [ ] `PyStopOrderEngine` 类暴露 `get_active_stop_orders()` 查询
- [ ] `PyStopOrderEngine` 类暴露 `cancel_stop_order()` 直接管理
- [ ] `PyStopOrderEngine` 类暴露 `cancel_orders_for_symbol()` 批量撤销
- [ ] Python 通过 `engine.stop_order_engine` 访问

### GUI 模拟单管理面板
- [ ] 新增"模拟订单"标签页，展示所有活跃模拟单
- [ ] 模拟单列表显示：ID、类型、合约、方向、触发价/限价、状态
- [ ] 支持撤销单个模拟单
- [ ] 支持按合约批量撤销
- [ ] 新建模拟单对话框：选择类型（追踪止损/止损限价/冰山单/MIT/LIT），输入参数
- [ ] 模拟单触发状态变更时列表实时更新

### GUI 止损单管理面板
- [ ] 新增"止损单"标签页，展示所有活跃止损单
- [ ] 止损单列表显示：ID、合约、方向、止损类型、触发价、状态
- [ ] 支持撤销单个止损单
- [ ] 支持按合约批量撤销
- [ ] 止损单触发/取消时列表实时更新

### 集成要求
- [ ] 依赖 REQ-064 完成回调绑定后，Python API 和 GUI 才能正常工作
- [ ] Python 示例：添加使用 OrderEmulator 的示例

## 影响范围

- `src/python/bindings.rs` — PyO3 模块定义，添加新类
- `src/python/` — 可能需要新增 `order_emulator.rs`、`stop_order_engine.rs` 绑定文件
- `src/trader/ui/` — 新增模拟单管理面板、止损单管理面板
- `examples/` — Python 使用高级订单的示例
