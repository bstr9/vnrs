---
id: REQ-040
title: "DataFrame 转 BarData 硬编码 Exchange::Binance 和 interval=None"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P1
cluster: Python-API
relations:
  depends_on: [REQ-039]
  related_to: [REQ-020]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 data_converter.rs:176-178 中 convert_polars_to_bars() 硬编码 exchange 和忽略 interval"
    reason: "初始发现"
    snapshot: "convert_polars_to_bars() 硬编码 Exchange::Binance 且 interval=None，多交易所或带周期回测不可用"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "新增 parse_exchange_str() 和 parse_interval_str() 辅助函数，修复 arrow_to_bars() 和 py_to_bar() 硬编码"
    reason: "修复完成"
    snapshot: "data_converter.rs 不再硬编码 exchange/interval，支持 BINANCE/OKX/BYBIT 等交易所和 1s~1d 周期"
---

# DataFrame 转 BarData 硬编码 Exchange::Binance 和 interval=None

## 描述
`src/python/data_converter.rs` 中 `convert_polars_to_bars()` 函数（lines 176-178）将 exchange 硬编码为 `Exchange::Binance`，interval 设为 `None`。这导致：
1. 从 Polars DataFrame 加载的数据如果来自其他交易所，exchange 信息丢失
2. K线周期信息缺失，无法按周期过滤或显示

## 验收标准
- [x] `convert_polars_to_bars()` 接受 exchange 参数（或从 DataFrame 列自动推断）
- [x] `convert_polars_to_bars()` 接受 interval 参数（或从 DataFrame 列自动推断）
- [x] 转换后的 BarData 包含正确的 exchange 和 interval 值
