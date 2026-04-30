---
id: REQ-077
title: "策略体系统一：AlphaStrategy/AsyncStrategy → StrategyTemplate 适配"
status: completed
level: epic
priority: P0
cluster: strategy-unification
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-30T10:00:00"
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: []
  related_to: [REQ-058, REQ-062]
  refined_by: [REQ-078, REQ-079]
versions:
  - version: 1
    date: "2026-04-26T10:00:00"
    author: user
    context: "用户反馈功能没有串联起来，Alpha模块和Async策略都无法通过标准回测引擎运行"
    reason: "初始提出"
    snapshot: "统一策略体系，使AlphaStrategy和AsyncStrategy都能通过标准BacktestingEngine运行"
  - version: 2
    date: "2026-04-29T12:00:00"
    author: ai
    context: "AsyncStrategyAdapter和AlphaStrategyAdapter已实现，set_parameters()已添加到trait"
    reason: "更新验收标准进度"
    snapshot: "策略适配器完成，set_parameters()已添加，Alpha独立引擎待废弃"
  - version: 3
    date: "2026-04-30T10:00:00"
    author: ai
    context: "AlphaStrategyAdapter registered in mod.rs, re-exported from alpha/mod.rs and lib.rs. Alpha BacktestingEngine deprecated with #[deprecated] annotation and module docs recommending adapter path. alpha_demo.rs updated to use adapter."
    reason: "Complete adapter registration and deprecation"
    snapshot: "策略适配器完成并注册，Alpha独立引擎已废弃，验收标准全部达成"
---

# 策略体系统一：AlphaStrategy/AsyncStrategy → StrategyTemplate 适配

## 描述
当前vnrs的策略体系存在三个不兼容的trait：StrategyTemplate（同步）、AsyncStrategy（异步）、AlphaStrategy（量化）。这导致：
- Alpha策略无法使用标准回测引擎（没有填充模型、前视偏差防护）
- Async策略无法回测
- 三个体系各自维护独立的引擎代码，重复且不一致

需要建立统一的策略适配层，使所有策略类型都能：
1. 通过标准 BacktestingEngine 回测（含填充模型、无前视偏差）
2. 通过标准 StrategyEngine 实盘运行
3. 支持参数优化和部署

## 验收标准
- [x] AsyncStrategy 可以通过适配器在 BacktestingEngine 中回测
- [x] AlphaStrategy 可以通过适配器在 BacktestingEngine 中回测
- [x] Alpha 的独立 BacktestingEngine 被废弃，统一使用标准引擎
- [x] 所有策略类型共享相同的填充模型和统计模块
- [x] cargo check/clippy/test 零错误零警告
