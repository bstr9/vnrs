---
id: REQ-074
title: "RPC 服务启动 + 远程监控 GUI 面板"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T21:00:00"
priority: P3
level: story
cluster: RPC
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-060]
  merged_from: []
  refined_by: []
  related_to: []
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 RPC 服务已实现但从未启动。"
    reason: "录入 RPC 服务启动集成需求"
    snapshot: "main.rs 启动 RPC 服务，事件广播到远程客户端"
  - version: 2
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认全覆盖——RPC 远程监控也需要 GUI 面板。优先级 P3（分布式场景才需要）。"
    reason: "补充远程监控 GUI 面板需求"
    snapshot: "RPC 服务启动 + GUI 远程监控面板（连接状态、客户端列表、事件广播）"
---

# RPC 服务启动 + 事件广播——远程监控集成

## 描述

RPC 模块（`src/rpc/`）已实现：
- ZeroMQ 事件广播（tick/bar/order/trade）
- 远程查询接口（持仓/委托/账户）
- 远程控制接口（启动/停止策略）

但 main.rs 中**从未启动 RPC 服务**。所有远程监控功能完全不可用。

## 验收标准

### 启动集成
- [x] main.rs 添加 RPC 服务启动选项
- [x] RPC 服务端口可配置
- [x] RPC 事件广播连接到 MainEngine 事件总线

### 事件广播
- [ ] tick/bar/order/trade 事件广播到远程客户端
- [ ] 远程客户端可订阅特定事件类型

### 远程查询
- [x] 远程查询持仓
- [x] 远程查询活跃委托
- [x] 远程查询账户余额

### 远程控制
- [ ] 远程启动/停止策略
- [x] 远程发送/撤销委托

### GUI 远程监控面板
- [ ] 新增"远程监控"标签页
- [ ] RPC 服务状态显示（运行中/已停止、端口、连接数）
- [ ] RPC 服务启动/停止按钮
- [ ] 已连接客户端列表（IP、订阅事件类型、连接时长）
- [ ] 事件广播统计（已发送事件数、各类型事件数）
- [ ] RPC 端口配置输入框

## 影响范围

- `src/main.rs` — 添加 RPC 服务启动
- `src/rpc/` — 可能需要添加启动配置
- `src/trader/engine.rs` — 事件广播到 RPC
- `src/trader/ui/` — 新增远程监控 GUI 面板
