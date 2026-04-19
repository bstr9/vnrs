---
id: REQ-033
title: "MCP HTTP/SSE Transport"
status: active
created_at: "2026-04-20T00:00:00"
updated_at: "2026-04-20T00:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-035, REQ-037]
  related_to: [REQ-034]
  cluster: MCP
versions:
  - version: 1
    date: "2026-04-20T00:00:00"
    author: ai
    context: "MCP 分析后发现当前仅支持 STDIO transport，无法支持 Web 客户端和远程访问。用户确认需要远程和 Web 客户端支持。"
    reason: "支持 Web 客户端和远程访问"
    snapshot: "为 TradingMcpServer 添加 HTTP/SSE transport 支持，实现 Streamable HTTP 规范"
---

# MCP HTTP/SSE Transport

## 描述

当前 MCP Server 仅支持 STDIO transport（用于 Claude Desktop 本地集成）。需要添加 HTTP/SSE transport 以支持 Web 客户端和远程访问。

### 背景

MCP 协议支持多种 transport：
- **STDIO**: 本地进程通信（当前已实现）
- **Streamable HTTP**: 生产级远程部署（2025-03-26+ 规范推荐）
- **SSE (Server-Sent Events)**: 向后兼容，被 Streamable HTTP 替代
- **WebSocket**: 双向实时通信

### 当前实现

```rust
// src/main.rs - 仅 STDIO 模式
if std::env::var("MCP_MODE").is_ok() {
    let service = TradingMcpServer::new(/* ... */);
    service.serve_stdio().await;  // 仅 STDIO
}
```

### 目标

支持以下场景：
1. Web 前端通过 HTTP/SSE 连接 MCP Server
2. 远程客户端通过 Streamable HTTP 访问
3. 保持 STDIO 模式兼容（Claude Desktop）

## 验收标准

- [ ] 添加 HTTP/SSE transport 支持（使用 rmcp 或升级到 rust-mcp-sdk）
- [ ] 实现 Streamable HTTP 规范（POST /mcp + SSE response）
- [ ] 添加配置选项：transport type (stdio/http/sse)
- [ ] 添加端口配置：HTTP/SSE 监听端口
- [ ] 保持 STDIO 模式向后兼容
- [ ] Web 客户端可连接并调用 Tools
- [ ] 资源订阅通过 SSE 推送实时数据

## 技术选项

| SDK | 优势 | 劣势 |
|-----|------|------|
| **rmcp** (当前) | 已集成 | HTTP transport 支持待确认 |
| **rust-mcp-sdk** ⭐170 | Streamable HTTP, SSE, OAuth | 需要迁移 |
| **turbomcp** ⭐77 | 零样板宏，高性能 | Edition 2024 |

## 工作量

1-2 天

## 设计参考

- MCP 规范：https://modelcontextprotocol.io/specification/2025-03-26/basic/transports
- rust-mcp-sdk HTTP 示例
