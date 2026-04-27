---
id: REQ-077
title: "策略体系统一：AlphaStrategy/AsyncStrategy → StrategyTemplate 适配"
status: active
level: epic
priority: P0
cluster: strategy-unification
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-26T10:00:00"
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
- [ ] AsyncStrategy 可以通过适配器在 BacktestingEngine 中回测
- [ ] AlphaStrategy 可以通过适配器在 BacktestingEngine 中回测
- [ ] Alpha 的独立 BacktestingEngine 被废弃，统一使用标准引擎
- [ ] 所有策略类型共享相同的填充模型和统计模块
- [ ] cargo check/clippy/test 零错误零警告
