---
id: REQ-021
title: "FuturesStrategy Python 基类（含开平模式）"
status: active
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-020, REQ-025]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现当前 Python Strategy 基类的 short/cover 方法仅有 warning 提示，不实际发单。期货交易需要开平模式（Open/Close/CloseYesterday/CloseToday），这是期货策略的基本需求。"
    reason: "期货策略需要开平模式支持，当前 short/cover 在期货场景不可用"
    snapshot: "FuturesStrategy Python 基类，提供 buy/sell/short/cover + Offset 开平模式，自动处理期货开仓/平仓逻辑"
---

# FuturesStrategy Python 基类（含开平模式）

## 描述

当前 Python `Strategy` 基类的 `short()` 和 `cover()` 方法仅有 warning 提示，不实际发送订单。期货交易需要 Offset 模式（开仓 Open / 平仓 Close / 平昨 CloseYesterday / 平今 CloseToday），这是期货策略的基本需求。

vnpy 的 CtaTemplate 已有完整的开平模式支持。vnrs 应提供 `FuturesStrategy` 基类，自动处理期货开仓/平仓逻辑。

## 验收标准

- [ ] `FuturesStrategy` Python 基类继承自 `Strategy`
- [ ] `Offset` 枚举：Open, Close, CloseYesterday, CloseToday
- [ ] `buy()` / `sell()` / `short()` / `cover()` 支持指定 offset 参数
- [ ] `short()` 发送 Direction::Short + Offset::Open 的订单
- [ ] `cover()` 发送 Direction::Short + Offset::Close 的订单
- [ ] Position 查询支持期货方向（Long/Short 双向持仓）
- [ ] 与现有 `SpotStrategy` 并列，共享 OrderFactory 基础设施

## 影响范围

- 新增 `src/python/futures_strategy.py`
- `src/python/strategy.rs` — 可能需要调整 short/cover 逻辑
- `src/trader/constant.rs` — Offset 枚举（如果尚未存在）
