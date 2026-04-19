---
id: REQ-043
title: "PyBarData 缺少 turnover/open_interest 字段"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P2
cluster: Python-API
relations:
  depends_on: [REQ-020]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 backtesting_bindings.rs:621 中 PyBarData.__getitem__ 对 turnover/open_interest 返回硬编码 0.0"
    reason: "初始发现"
    snapshot: "PyBarData 构造函数和字段缺少 turnover/open_interest，__getitem__ 返回硬编码 0.0"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "PyBarData 新增 turnover/open_interest 字段，更新构造函数/__getitem__/to_rust()，strategy_adapter 传递实际值"
    reason: "修复完成"
    snapshot: "PyBarData 支持 turnover 和 open_interest 字段，Python 端 BarData 类可访问这两个属性"
---

# PyBarData 缺少 turnover/open_interest 字段

## 描述
`src/python/backtesting_bindings.rs` 中 `PyBarData` 的构造函数（lines 534-588）没有 `turnover`（成交额）和 `open_interest`（持仓量）参数。`__getitem__` 方法（line 621）对这两个键返回硬编码的 `0.0`。对于期货策略，持仓量是关键指标；成交额对成交量加权分析也很重要。

## 验收标准
- [x] `PyBarData::new()` 接受 `turnover` 和 `open_interest` 参数
- [x] `__getitem__` 返回实际值而非硬编码 `0.0`
- [x] Python 端 `BarData` 类包含 `turnover` 和 `open_interest` 属性
