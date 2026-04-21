---
id: REQ-068
title: "AlertEngine → ToastManager 告警通知集成"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P2
level: story
cluster: GUI
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-061]
  merged_from: []
  refined_by: []
  related_to: [REQ-065]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 AlertEngine 已实现风控告警（risk_reject、connection_loss、large_order 等），但 GUI 的 ToastManager 从未接收 AlertEngine 的告警事件。风控拒绝等关键告警只在日志中出现，用户界面无感知。"
    reason: "录入告警通知集成需求"
    snapshot: "AlertEngine 告警事件通过事件总线转发到 GUI ToastManager，实现界面弹窗通知"
---

# AlertEngine → ToastManager 告警通知集成

## 描述

AlertEngine（`src/trader/alert.rs`）实现了多种告警类型：
- `alert_risk_reject()` — 风控拒绝
- `alert_connection_loss()` — 连接断开
- `alert_large_order()` — 大额委托
- `alert_high_frequency()` — 高频交易

但 AlertEngine 的告警事件**未连接到 GUI 的 ToastManager**。风控拒绝、连接断开等关键告警只在日志中出现，GUI 用户无法及时感知。

## 验收标准

### 事件转发
- [ ] AlertEngine 告警事件通过 MainEngine 事件总线发出
- [ ] 定义 EVENT_ALERT 事件类型
- [ ] ToastManager 注册为 EVENT_ALERT 事件处理器

### UI 展示
- [ ] 风控拒绝告警：Toast 弹窗显示"风控拒绝"红色通知
- [ ] 连接断开告警：Toast 弹窗显示"连接断开"红色通知
- [ ] 大额委托告警：Toast 弹窗显示"大额委托"黄色通知
- [ ] 高频交易告警：Toast 弹窗显示"高频交易"黄色通知
- [ ] 告警按级别显示不同颜色（Error=红、Warning=黄、Info=蓝）

## 影响范围

- `src/trader/alert.rs` — AlertEngine 添加事件总线发送
- `src/trader/engine.rs` — 注册告警事件处理器
- `src/trader/ui/toast.rs` — ToastManager 接收告警事件
