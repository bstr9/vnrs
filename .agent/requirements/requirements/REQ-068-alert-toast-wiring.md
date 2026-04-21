---
id: REQ-068
title: "AlertEngine → ToastManager 告警通知集成"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T22:00:00"
priority: P1
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
    context: "集成审计发现 AlertEngine 已实现风控告警，但 GUI 的 ToastManager 从未接收告警事件。"
    reason: "录入告警通知集成需求"
    snapshot: "AlertEngine 告警事件通过事件总线转发到 GUI ToastManager"
  - version: 2
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认 GUI 是主要使用入口。风控拒绝/连接断开等关键事件用户必须在界面上看到，优先级从 P2 升到 P1。"
    reason: "GUI 优先级提升——关键告警必须界面可见"
    snapshot: "AlertEngine → 事件总线 → ToastManager 弹窗 + 告警历史面板"
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

### 告警历史面板
- [ ] 新增"告警"标签页，展示告警历史记录
- [ ] 告警列表显示：时间、级别、类型、合约、详情
- [ ] 支持按级别筛选（Error/Warning/Info）
- [ ] 支持按时间范围筛选
- [ ] 告警记录持久化（数据库或文件）

## 影响范围

- `src/trader/alert.rs` — AlertEngine 添加事件总线发送
- `src/trader/engine.rs` — 注册告警事件处理器
- `src/trader/ui/toast.rs` — ToastManager 接收告警事件
- `src/trader/ui/` — 新增告警历史面板
