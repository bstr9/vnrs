---
id: REQ-071
title: "回测引擎填充模型选择 API"
status: active
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P2
level: story
cluster: Backtesting
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-057]
  merged_from: []
  refined_by: []
  related_to: [REQ-063]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现回测引擎（src/backtesting/fill_model.rs）实现了 5 种填充模型（BestPrice/Ideal/TwoTier/SizeAware/Probabilistic），但无 API 让用户选择使用哪种模型。默认使用 BestPrice（最不真实），且 Python 和 GUI 均无选择接口。"
    reason: "录入回测填充模型选择 API 需求"
    snapshot: "Python 和 GUI 提供回测填充模型选择接口，用户可选 BestPrice/Ideal/TwoTier/SizeAware/Probabilistic"
---

# 回测引擎填充模型选择 API

## 描述

回测引擎（`src/backtesting/fill_model.rs`）实现了 5 种填充模型，从最简单到最真实：
1. **BestPrice** — 总是以最优价成交（最不真实，当前默认）
2. **Ideal** — 理想成交，无滑点
3. **TwoTier** — 两级成交概率（有利/不利）
4. **SizeAware** — 考虑委托量的成交模拟
5. **Probabilistic** — 概率成交模型（最真实）

但当前无 API 让用户选择填充模型。Python 和 GUI 均使用硬编码的默认模型。

## 验收标准

### Python API
- [ ] `PyBacktestingEngine.set_fill_model(model_name)` 方法
- [ ] 支持字符串选择："best_price"、"ideal"、"two_tier"、"size_aware"、"probabilistic"
- [ ] 默认仍为 "best_price"（向后兼容）

### GUI
- [ ] 回测设置面板添加填充模型下拉框
- [ ] 显示每种模型的简短描述

### Rust API
- [ ] BacktestingEngine 提供构造时或运行前设置填充模型的方法
- [ ] 填充模型选择影响回测统计结果

## 影响范围

- `src/backtesting/engine.rs` — 添加填充模型设置接口
- `src/backtesting/fill_model.rs` — 模型已就绪，无需改动
- `src/python/` — Python 绑定添加 set_fill_model
- `src/trader/ui/` — GUI 回测设置面板
