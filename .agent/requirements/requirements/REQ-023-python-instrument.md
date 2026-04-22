---
id: REQ-023
title: "Instrument 元数据类（Python 端）"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-020]
  merged_from: []
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现 nautilus_trader 有完整的 Instrument 类（含 tick_size, lot_size, min_notional 等），vnrs Python 端无 Instrument 概念。策略需要合约元信息进行正确的下单量计算和价格取整。"
    reason: "策略需要合约元信息（tick_size, lot_size 等）进行正确的下单量计算"
    snapshot: "Python Instrument 类，包含 symbol, exchange, tick_size, lot_size, min_notional, price_tick, multiplier 等合约元数据"
---

# Instrument 元数据类（Python 端）

## 描述

Python 策略当前无法获取合约元信息（如最小价格变动 tick_size、最小交易量 lot_size、最小名义价值 min_notional 等）。这些信息对于：
- 价格取整（下单价格必须是 tick_size 的整数倍）
- 数量取整（下单数量必须是 lot_size 的整数倍）
- 最低名义价值检查（Binance 等交易所有 min_notional 限制）

至关重要。nautilus_trader 的 Instrument 类提供完整的合约元信息。

## 验收标准

- [x] `PyInstrument` 类：symbol, exchange, name, tick_size, lot_size, min_notional, price_tick, multiplier, margin_rate
- [x] 策略可通过 `self.get_instrument(symbol)` 获取 Instrument 信息
- [x] Spot 网关：从 `/api/v3/exchangeInfo` 获取并缓存 Instrument
- [x] Futures 网关：从 `/fapi/v1/exchangeInfo` 获取并缓存 Instrument
- [x] Instrument 信息在网关连接时自动加载
- [x] 下单量自动取整辅助方法：`round_price(price)`, `round_volume(volume)`

## 影响范围

- 新增 `src/python/instrument.rs` — PyInstrument 定义
- `src/gateway/binance/spot_gateway.rs` — 加载 exchangeInfo
- `src/gateway/binance/usdt_gateway.rs` — 加载 exchangeInfo
- `src/python/strategy.rs` — 添加 get_instrument 方法
