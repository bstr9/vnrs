---
id: REQ-048
title: "Gateway 模块零测试覆盖（7 个文件）"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P1
cluster: Infrastructure
relations:
  depends_on: []
  related_to: [REQ-047]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 gateway/binance/ 下全部 7 个文件（含生产资金处理代码）零测试覆盖"
    reason: "初始发现"
    snapshot: "Binance Gateway（REST 客户端、现货/合约网关、WebSocket 客户端）零测试覆盖"
---

# Gateway 模块零测试覆盖

## 描述
`src/gateway/binance/` 下全部 7 个文件（rest_client.rs、spot_gateway.rs、usdt_gateway.rs、websocket_client.rs、config.rs、constants.rs、mod.rs）没有任何单元测试。这些是处理真实资金的代码，包含 REST API 签名、订单解析、WebSocket 消息处理等关键逻辑。

## 验收标准
- [ ] `rest_client.rs` 添加签名逻辑单元测试（HMAC 签名验证）
- [ ] `spot_gateway.rs` 添加订单解析测试（API 响应 → OrderData）
- [ ] `usdt_gateway.rs` 添加合约订单解析测试
- [ ] `websocket_client.rs` 添加消息路由测试
- [ ] 使用 mock HTTP 响应，不依赖真实交易所连接
