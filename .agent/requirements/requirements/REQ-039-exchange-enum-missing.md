---
id: REQ-039
title: "Exchange 枚举缺失 OKX/Bybit 等主流交易所变体"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P1
cluster: Python-API
relations:
  depends_on: []
  related_to: [REQ-020]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 backtesting_bindings.rs 中 OKX/Bybit 映射到 Exchange::Global，缺少专用变体"
    reason: "初始发现"
    snapshot: "Exchange 枚举缺少 OKX、Bybit 等主流交易所变体，回测时订单和行情静默映射到 Exchange::Global"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "新增 Exchange::Okx 和 Exchange::Bybit 变体，更新所有 10+ 文件的字符串到枚举映射"
    reason: "修复完成"
    snapshot: "Exchange 枚举新增 Okx/Bybit 变体，backtesting/strategy/ui/database/mcp 层全部更新映射"
---

# Exchange 枚举缺失 OKX/Bybit 等主流交易所变体

## 描述
`src/trader/constant.rs` 中的 `Exchange` 枚举缺少 OKX、Bybit 等主流加密货币交易所变体。在 `src/python/backtesting_bindings.rs:650-651` 中，OKX 和 Bybit 被映射到 `Exchange::Global`，这会导致订单路由失败或数据查找不匹配。

## 验收标准
- [x] `Exchange` 枚举新增 `Okx`、`Bybit` 变体
- [x] 回测引擎的 exchange 字符串到枚举的映射覆盖 OKX/Bybit
- [x] Display trait 为新变体输出正确的交易所名称字符串
- [x] 数据库查询可按新交易所变体过滤
