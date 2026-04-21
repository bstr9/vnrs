---
id: REQ-067
title: "GUI 交易面板支持止损单/止损限价单下单"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P1
level: story
cluster: GUI
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-061]
  merged_from: []
  refined_by: []
  related_to: [REQ-064, REQ-042]
  depends_on: [REQ-064]
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 TradingWidget 下单面板仅支持 Limit/Market/FAK/FOK 四种订单类型，缺少 Stop 和 StopLimit。Rust 侧 StopOrderEngine 已完整实现，但 GUI 用户无法通过界面提交止损单。"
    reason: "录入 GUI 止损单下单功能缺失需求"
    snapshot: "TradingWidget 订单类型下拉框添加 Stop/StopLimit 选项，添加触发价格输入框"
---

# GUI 交易面板支持止损单/止损限价单下单

## 描述

当前 GUI 的 `TradingWidget`（`src/trader/ui/trading.rs`）下单面板支持四种订单类型：
- Limit（限价单）
- Market（市价单）
- FAK（Fill-or-Kill 变体）
- FOK（Fill-or-Kill）

但缺少：
- **Stop**（止损单/停损单）—— 价格触及时转为市价单
- **StopLimit**（止损限价单）—— 价格触及时转为限价单

Rust 侧 StopOrderEngine 已完整实现，但 GUI 用户无法通过界面提交这两种订单。

### 当前代码

TradingWidget 的订单类型下拉框（`trading.rs`）：
```rust
// 当前仅 4 种
combo_box.show_index(&mut self.order_type_index, &ORDER_TYPES, ...);
```
需要添加 Stop/StopLimit 到 ORDER_TYPES 列表，并在 StopLimit 时显示 limit_price 输入框。

## 验收标准

### 订单类型扩展
- [ ] ORDER_TYPES 列表添加 "Stop" 和 "StopLimit"
- [ ] 选择 Stop 时，显示 stop_price 输入框（触发价）
- [ ] 选择 StopLimit 时，显示 stop_price + limit_price 两个输入框
- [ ] Stop 订单提交通过 StopOrderEngine
- [ ] StopLimit 订单提交通过 StopOrderEngine

### UI 交互
- [ ] 止损单下单后，在活跃委托列表显示（带 STOP_ 前缀标识）
- [ ] 止损单可在活跃委托列表中撤销
- [ ] 止损单触发状态变更时，UI 列表实时更新

### 依赖
- [ ] 依赖 REQ-064 完成回调绑定后，GUI 提交的止损单才能实际触发下单

## 影响范围

- `src/trader/ui/trading.rs` — TradingWidget 下单面板（主要改动）
- `src/trader/ui/` — 可能需要止损单列表组件
