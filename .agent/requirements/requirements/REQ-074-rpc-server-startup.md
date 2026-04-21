---
id: REQ-074
title: "RPC 服务启动 + 事件广播——远程监控集成"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
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
    context: "集成审计发现 RPC 服务（src/rpc/）已实现 ZeroMQ 事件广播和远程调用接口，但 main.rs 中从未启动 RPC 服务。远程监控、分布式系统功能完全不可用。"
    reason: "录入 RPC 服务启动集成需求"
    snapshot: "main.rs 启动 RPC 服务，事件广播到远程客户端，支持远程查询和策略管理"
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
- [ ] main.rs 添加 RPC 服务启动选项
- [ ] RPC 服务端口可配置
- [ ] RPC 事件广播连接到 MainEngine 事件总线

### 事件广播
- [ ] tick/bar/order/trade 事件广播到远程客户端
- [ ] 远程客户端可订阅特定事件类型

### 远程查询
- [ ] 远程查询持仓
- [ ] 远程查询活跃委托
- [ ] 远程查询账户余额

### 远程控制
- [ ] 远程启动/停止策略
- [ ] 远程发送/撤销委托

## 影响范围

- `src/main.rs` — 添加 RPC 服务启动
- `src/rpc/` — 可能需要添加启动配置
- `src/trader/engine.rs` — 事件广播到 RPC
