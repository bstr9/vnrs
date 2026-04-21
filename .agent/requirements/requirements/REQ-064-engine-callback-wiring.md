---
id: REQ-064
title: "三大引擎回调绑定——止损单/模拟单/组合单功能断头路修复"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P0
level: story
cluster: Core-Trading
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-063]
  merged_from: []
  refined_by: []
  related_to: [REQ-025, REQ-065, REQ-066, REQ-067]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 StopOrderEngine、OrderEmulator、BracketOrderEngine 三引擎已实现并注册到 MainEngine，但回调函数从未绑定——触发后无法提交真实委托，功能完全断头。这是 P0 级别的集成缺陷。"
    reason: "录入关键集成缺陷需求，三大引擎回调未绑定导致功能完全无效"
    snapshot: "在 MainEngine 构造中绑定三引擎回调：StopOrderEngine.register_callback、OrderEmulator.set_send_order_callback/set_cancel_order_callback、BracketOrderEngine.set_send_order_callback/set_cancel_order_callback/set_state_change_callback"
---

# 三大引擎回调绑定——止损单/模拟单/组合单功能断头路修复

## 描述

**严重集成缺陷**：StopOrderEngine、OrderEmulator、BracketOrderEngine 三套引擎已在 `MainEngine::new_internal()` 中创建并注册到 `engines` HashMap，通过 `process_event()` 接收行情和委托事件，但**回调函数从未绑定**。

这意味着：
- 止损单触发后，无法提交真实委托到交易所——止损/止盈/追踪止损形同虚设
- 模拟订单触发后，无法提交真实委托——追踪止损/止损限价/冰山单/MIT/LIT 全部断头
- 组合单无法提交入场/出场委托——Bracket/OCO/OTO 组合完全无效

三套引擎共 3000 行代码，实现完整但**从未被接通**，是典型的"断头路"问题。

### 回调签名详情

| 引擎 | 方法 | 回调类型 | 作用 |
|------|------|---------|------|
| StopOrderEngine | `register_callback(callback: StopOrderCallback)` | `Box<dyn Fn(&StopOrder, OrderRequest) + Send + Sync>` | 止损单触发时，将 StopOrder 转 OrderRequest 提交 |
| OrderEmulator | `set_send_order_callback(callback: EmulatorSendOrderCallback)` | `Box<dyn Fn(&OrderRequest) -> Result<String, String> + Send + Sync>` | 模拟单触发时提交真实委托 |
| OrderEmulator | `set_cancel_order_callback(callback: EmulatorCancelOrderCallback)` | `Box<dyn Fn(&CancelRequest) -> Result<(), String> + Send + Sync>` | 模拟单需要撤销真实委托 |
| BracketOrderEngine | `set_send_order_callback(callback: SendOrderCallback)` | `Box<dyn Fn(&OrderRequest) -> Result<String, String> + Send + Sync>` | 组合单提交入场/出场委托 |
| BracketOrderEngine | `set_cancel_order_callback(callback: CancelOrderCallback)` | `Box<dyn Fn(&CancelRequest) -> Result<(), String> + Send + Sync>` | 组合单撤销子委托 |
| BracketOrderEngine | `set_state_change_callback(callback: StateChangeCallback)` | `Box<dyn Fn(&OrderGroup) + Send + Sync>` | 组合单状态变更通知 |

### 当前代码状态

`src/trader/engine.rs` 中 `new_internal()` 构造函数（约 L685-L761）：
- 创建三引擎并存储为 MainEngine 字段 ✅
- 注册到 `engines` HashMap ✅
- **未调用任何回调绑定方法** ❌

回调绑定的正确位置：构造完成后、返回 `engine` 之前，或者在 `start()` 方法中。

## 验收标准

### StopOrderEngine 回调绑定
- [ ] 在 MainEngine 中调用 `stop_order_engine.register_callback()` 绑定触发回调
- [ ] 回调逻辑：将 StopOrder 转换为 OrderRequest，通过 MainEngine.send_order() 提交到对应 gateway
- [ ] 回调中正确使用 stop_order.gateway_name 定位 gateway
- [ ] 止损单触发后日志输出：包含 stop_orderid、vt_symbol、direction、trigger_price

### OrderEmulator 回调绑定
- [ ] 在 MainEngine 中调用 `order_emulator.set_send_order_callback()` 绑定下单回调
- [ ] 在 MainEngine 中调用 `order_emulator.set_cancel_order_callback()` 绑定撤单回调
- [ ] send_callback 逻辑：通过 MainEngine.send_order() 提交到对应 gateway
- [ ] cancel_callback 逻辑：通过 MainEngine.cancel_order() 撤销对应委托
- [ ] 回调中正确使用 emulated_order.gateway_name 定位 gateway

### BracketOrderEngine 回调绑定
- [ ] 在 MainEngine 中调用 `bracket_order_engine.set_send_order_callback()` 绑定下单回调
- [ ] 在 MainEngine 中调用 `bracket_order_engine.set_cancel_order_callback()` 绑定撤单回调
- [ ] 在 MainEngine 中调用 `bracket_order_engine.set_state_change_callback()` 绑定状态变更回调
- [ ] send_callback 逻辑：通过 MainEngine.send_order() 提交到对应 gateway
- [ ] cancel_callback 逻辑：通过 MainEngine.cancel_order() 撤销对应委托
- [ ] state_change_callback 逻辑：写入 MainEngine 日志（write_log），通知组状态变更

### 集成验证
- [ ] MainEngine::new_internal() 编译通过
- [ ] 现有单元测试不受影响
- [ ] 回调闭包中正确处理 Arc<MainEngine> 引用（避免循环引用或生命周期问题）

### 注意事项
- [ ] MainEngine.send_order() 是 async 方法，回调签名是同步的——需要用 tokio::spawn 或 spawn_blocking 处理
- [ ] 回调绑定位置选择：构造函数末尾 vs start() 方法。构造函数末尾更安全（回调立即可用），但需要处理 Arc<Self> 获取
- [ ] 不能在回调中直接 await async 方法，需要通过 event_tx 发送事件或 tokio::spawn

## 影响范围

- `src/trader/engine.rs` — MainEngine::new_internal() 添加回调绑定代码（主要改动）
- `src/trader/stop_order.rs` — StopOrderEngine（无需改动，回调接口已就绪）
- `src/trader/order_emulator.rs` — OrderEmulator（无需改动，回调接口已就绪）
- `src/trader/bracket_order.rs` — BracketOrderEngine（无需改动，回调接口已就绪）
