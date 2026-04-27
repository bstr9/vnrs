---
id: REQ-087
title: "策略回测到实盘无缝切换"
status: active
level: story
priority: P1
cluster: strategy-lifecycle
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-081, REQ-083, REQ-084, REQ-085]
  refines: []
  related_to: [REQ-078, REQ-077]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "量化策略必须先回测验证再实盘。当前回测引擎和实盘引擎是两套独立系统，策略在两者间切换需要修改代码（如买卖调用方式、数据获取方式不同）。"
    reason: "回测到实盘的无缝切换是量化系统的核心竞争力，切换成本高会导致策略验证不足就上实盘"
    snapshot: "同一份策略代码不做修改即可在回测和实盘两种模式运行"
---

# 策略回测到实盘无缝切换

## 描述
量化策略的开发流程是：回测验证 → 模拟盘验证 → 实盘。当前回测引擎（`backtesting::Engine`）和实盘引擎（`StrategyEngine`）虽然共享 `StrategyTemplate` trait，但在使用中存在差异：

1. `StrategyContext` 在回测和实盘中的行为不同（数据来源、下单方式）
2. 回测中的 `buy/sell/short/cover` 直接成交，实盘需要经过风控和交易所
3. 回测中策略可以直接访问历史数据，实盘需要异步加载
4. 缺少模拟盘模式（paper trading）— 连接真实行情但虚拟成交

需要确保：
1. 同一份策略代码（实现 `StrategyTemplate`）可在回测和实盘中无修改运行
2. `on_init`/`on_tick`/`on_bar`/`on_trade`/`on_order` 回调在两种模式下行为一致
3. `StrategyContext` 抽象数据来源（回测=历史数据，实盘=数据库+实时流）
4. 添加模拟盘模式：使用真实行情，虚拟撮合成交

## 验收标准
- [ ] 实现 `StrategyTemplate` 的策略可在回测引擎和实盘引擎中无修改运行
- [ ] `StrategyContext.load_bar()` 在回测模式返回回测数据，实盘模式返回数据库数据
- [ ] `buy/sell/short/cover` 在回测中虚拟成交，实盘中真实发单
- [ ] `get_position()` 在两种模式下语义一致
- [ ] 模拟盘模式：连接真实Binance行情，订单不实际提交，使用本地撮合
- [ ] GUI 支持选择运行模式：回测/模拟/实盘
