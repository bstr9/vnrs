---
id: REQ-045
title: "Gateway .expect() 可能导致运行时崩溃"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T19:25:00"
priority: P1
cluster: Bug-Fix
relations:
  depends_on: [REQ-046]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 gateway/binance/rest_client.rs 6 处 .expect()、spot_gateway.rs 3 处 .expect()、backtesting/engine.rs 5 处 .expect() 在生产代码中"
    reason: "初始发现"
    snapshot: "Gateway 和回测引擎中共 14 处 .expect() 可能在异常情况下 panic 导致进程崩溃"
  - version: 2
    date: "2026-04-20T19:25:00"
    author: ai
    context: "修复完成：rest_client.rs new() 返回 Result、所有 .expect() 替换为安全错误处理"
    reason: "Bug 修复完成"
    snapshot: "Gateway 和回测引擎中所有生产代码 .expect() 已替换为安全错误处理"
---

# Gateway .expect() 可能导致运行时崩溃

## 描述
多处生产代码使用 `.expect()` 在可恢复错误上，可能导致交易引擎在运行时崩溃：

1. **`rest_client.rs`** 6 处 `.expect()`：4 处 "Failed to create HTTP client"、1 处 "Time went backwards"、1 处 "HMAC can take key of any size"
2. **`spot_gateway.rs`** 3 处 `.expect("order_type verified non-None above")`：API 响应异常时崩溃
3. **`backtesting/engine.rs`** 5 处 `.expect("Position apply_fill failed")`：成交数据异常时崩溃

网络环境、API 响应格式变化、数据异常都是可预期的情况，应返回 `Result` 而非 panic。

## 验收标准
- [x] `rest_client.rs` HTTP 客户端构造返回 `Result`，失败由调用方处理
- [x] `spot_gateway.rs` 订单类型解析使用安全降级（如 `OrderType::Unknown`）
- [x] `backtesting/engine.rs` position fill 失败返回错误而非 panic
