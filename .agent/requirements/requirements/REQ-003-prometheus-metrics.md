---
id: REQ-003
title: "Prometheus 指标监控"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-004]
  cluster: Infrastructure
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发现 tesser 项目已实现 Prometheus 指标，vnrs 当前无任何 metrics 端点。生产环境可观测性是基本需求。"
    reason: "生产可观测性基础设施缺失"
    snapshot: "添加可选 Prometheus 指标，暴露 /metrics 端点，监控订单/成交/tick/仓位/PnL/策略活跃数"
---

# Prometheus 指标监控

## 描述

添加可选的 Prometheus 指标支持，通过 feature flag `prometheus` 控制。当前项目无任何 metrics 端点或监控基础设施，仅依赖 tracing 日志。

参考 tesser 项目的实现：tick/candle 吞吐、equity、order error 等指标。

## 验收标准

- [ ] 添加 `prometheus` 可选 feature 到 Cargo.toml
- [ ] 实现核心指标：
  - `vnrs_orders_total` (Counter, label: gateway, direction)
  - `vnrs_trades_total` (Counter, label: gateway, direction)
  - `vnrs_order_latency_seconds` (Histogram)
  - `vnrs_tick_count` (Counter)
  - `vnrs_position_value` (Gauge, label: symbol)
  - `vnrs_pnl_total` (Gauge)
  - `vnrs_strategy_active` (Gauge)
- [ ] MainEngine 添加 `start_metrics_server(addr)` 方法
- [ ] 指标在订单/成交/tick/仓位更新时自动采集
- [ ] 不启用 feature 时零开销

## 依赖

- `prometheus` crate (可选 feature，可避免——手写简单 HTTP `/metrics` 端点替代)

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第四节 4.2 "P1-4 Prometheus 指标"
