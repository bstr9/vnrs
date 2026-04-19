---
id: REQ-039
title: "Exchange 枚举缺失 OKX/Bybit 等主流交易所变体"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
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
---

# Exchange 枚举缺失 OKX/Bybit 等主流交易所变体

## 描述
`src/trader/constant.rs` 中的 `Exchange` 枚举缺少 OKX、Bybit 等主流加密货币交易所变体。在 `src/python/backtesting_bindings.rs:650-651` 中，OKX 和 Bybit 被映射到 `Exchange::Global`，这会导致订单路由失败或数据查找不匹配。

## 验收标准
- [ ] `Exchange` 枚举新增 `Okx`、`Bybit` 变体
- [ ] 回测引擎的 exchange 字符串到枚举的映射覆盖 OKX/Bybit
- [ ] Display trait 为新变体输出正确的交易所名称字符串
- [ ] 数据库查询可按新交易所变体过滤
