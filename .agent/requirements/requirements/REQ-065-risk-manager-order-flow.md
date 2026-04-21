---
id: REQ-065
title: "RiskManager 订单流拦截——风控引擎接入 send_order 流程"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P0
level: story
cluster: Core-Trading
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: [REQ-064, REQ-063]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 RiskManager 在 MainEngine.send_order() 中已执行 check_order_with_gateway() 风控检查（engine.rs:1121-1133），但三引擎（StopOrder/OrderEmulator/BracketOrderEngine）的回调提交委托时绕过了风控。实盘交易中，止损/模拟/组合单触发后的委托应同样经过风控审核。"
    reason: "录入风控引擎集成缺陷——三大引擎回调绕过风控直接下单"
    snapshot: "确保止损单/模拟单/组合单触发的真实委托也经过 RiskManager 风控检查"
---

# RiskManager 订单流拦截——风控引擎接入 send_order 流程

## 描述

**风控漏洞**：MainEngine.send_order() 已实现完整的风控检查流程（engine.rs:1121-1133）：
1. `risk_manager.check_order_with_gateway()` 审核委托
2. 审核拒绝 → 写日志 + 触发 AlertEngine + 返回错误
3. 审核通过 → OffsetConverter 偏移转换 → gateway.send_order()

但三引擎回调提交委托时，**必须通过 MainEngine.send_order() 而非直接调用 gateway.send_order()**，否则风控形同虚设。

### 当前状态

- `MainEngine.send_order()` ✅ 已包含风控检查
- REQ-064 绑定回调后，回调内部应调用 `MainEngine.send_order()` 而非跳过风控
- 需要确认回调绑定代码中正确通过 MainEngine 的 send_order 路径

### 风险分析

如果 REQ-064 的回调绑定直接调用 `gateway.send_order()` 绕过 MainEngine：
- 止损单触发后，大额委托无风控限制
- 组合单入场，可能超出持仓限制
- 冰山单切片，单笔小额但累计可能超限

## 验收标准

### 风控覆盖验证
- [ ] StopOrderEngine 回调提交委托通过 MainEngine.send_order() 走风控
- [ ] OrderEmulator 回调提交委托通过 MainEngine.send_order() 走风控
- [ ] BracketOrderEngine 回调提交委托通过 MainEngine.send_order() 走风控
- [ ] 风控拒绝时，StopOrderEngine 止损单状态正确更新（不应为 Triggered）
- [ ] 风控拒绝时，OrderEmulator 模拟单状态正确更新（不应为 Triggered）
- [ ] 风控拒绝时，BracketOrderEngine 组合单状态正确更新（不应进入 Active）

### 异常处理
- [ ] 风控拒绝的委托有日志记录
- [ ] 风控拒绝后，引擎内部状态一致（不应残留"已触发但未提交"的脏状态）
- [ ] 风控拒绝后，策略的 on_stop_order / on_order 回调能收到拒绝通知

## 影响范围

- `src/trader/engine.rs` — REQ-064 回调绑定代码需确保通过 MainEngine.send_order()
- `src/trader/risk.rs` — RiskManager（无需改动）
- `src/trader/stop_order.rs` — 触发失败时状态回滚逻辑（可能需要补充）
- `src/trader/order_emulator.rs` — 触发失败时状态回滚逻辑（可能需要补充）
- `src/trader/bracket_order.rs` — 入场委托被拒处理（已有 handle_rejection，但需确认风控拒绝也走此路径）
