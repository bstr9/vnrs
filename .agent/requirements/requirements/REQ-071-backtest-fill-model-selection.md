---
id: REQ-071
title: "回测配置面板——填充模型选择 + 参数配置 GUI"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T21:00:00"
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
    context: "集成审计发现回测引擎实现了 5 种填充模型，但无 API 选择。"
    reason: "录入回测填充模型选择 API 需求"
    snapshot: "Python 和 GUI 提供回测填充模型选择接口"
  - version: 2
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认全覆盖——回测是核心工作流，需要完整 GUI 配置面板。扩展为回测配置面板，不仅限于填充模型选择。"
    reason: "扩展为完整回测配置 GUI 面板"
    snapshot: "回测配置 GUI 面板——填充模型、手续费率、滑点、初始资金等参数配置"
---

# 回测配置面板——填充模型选择 + 参数配置 GUI

## 描述

回测引擎（`src/backtesting/fill_model.rs`）实现了 5 种填充模型，从最简单到最真实：
1. **BestPrice** — 总是以最优价成交（最不真实，当前默认）
2. **Ideal** — 理想成交，无滑点
3. **TwoTier** — 两级成交概率（有利/不利）
4. **SizeAware** — 考虑委托量的成交模拟
5. **Probabilistic** — 概率成交模型（最真实）

但当前无 API 让用户选择填充模型，且回测其他参数（手续费率、滑点、初始资金等）也缺乏统一的 GUI 配置入口。

## 验收标准

### Python API
- [ ] `PyBacktestingEngine.set_fill_model(model_name)` 方法
- [ ] 支持字符串选择："best_price"、"ideal"、"two_tier"、"size_aware"、"probabilistic"
- [ ] 默认仍为 "best_price"（向后兼容）

### GUI 回测配置面板
- [ ] 新增"回测配置"面板或对话框
- [ ] 填充模型下拉框：5 种模型可选，每种附带简短描述
- [ ] 手续费率输入框（rate，默认 0.0003）
- [ ] 滑点输入框（slippage，默认 0.0）
- [ ] 初始资金输入框（capital，默认 1,000,000）
- [ ] 合约选择器（vt_symbol）
- [ ] 时间范围选择器（开始/结束日期）
- [ ] K线周期选择器（1m/5m/15m/1h/1d）
- [ ] "开始回测"按钮，运行后显示统计结果
- [ ] 回测统计结果面板：总收益、年化收益、夏普比率、最大回撤、胜率

### Rust API
- [ ] BacktestingEngine 提供构造时或运行前设置填充模型的方法
- [ ] 填充模型选择影响回测统计结果

## 影响范围

- `src/backtesting/engine.rs` — 添加填充模型设置接口
- `src/backtesting/fill_model.rs` — 模型已就绪，无需改动
- `src/python/` — Python 绑定添加 set_fill_model
- `src/trader/ui/` — 新增回测配置 GUI 面板
