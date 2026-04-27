---
id: REQ-088
title: "策略参数优化与最优参数选择"
status: active
level: story
priority: P1
cluster: strategy-optimization
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-087]
  refines: []
  related_to: [REQ-080, REQ-019]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "量化策略参数需要通过历史数据优化。当前backtesting/optimization.rs存在优化框架，但缺少与实盘策略参数的对接——优化完的参数如何应用到实盘策略？"
    reason: "参数优化是量化策略开发的核心环节，优化结果需要方便地应用到实盘"
    snapshot: "策略参数可通过回测优化自动调优，最优参数一键应用到实盘策略"
---

# 策略参数优化与最优参数选择

## 描述
量化策略的参数（如MA周期、ATR倍数、止盈止损距离等）需要通过历史数据回测来优化选择。当前 `backtesting/optimization.rs` 提供了暴力搜索和遗传算法框架，但：

1. 优化结果缺少可视化（参数热力图、收益曲线对比）
2. 最优参数没有一键应用到实盘策略的机制
3. 缺少样本外检验（out-of-sample test）— 用训练集优化、测试集验证
4. 缺少Walk-Forward分析 — 滚动窗口优化+验证
5. 缺少参数稳定性分析 — 最优参数附近小范围扰动的收益变化

## 验收标准
- [ ] GUI 上可配置参数搜索范围（如 `atr_length: [10, 20, 30]`）
- [ ] 优化完成后显示参数-收益热力图
- [ ] 一键将最优参数应用到实盘策略（调用 `set_parameters()`）
- [ ] 支持样本外检验：70%数据训练+30%数据验证
- [ ] 支持Walk-Forward分析：滚动窗口优化
- [ ] 参数稳定性报告：最优参数±1的收益变化率
