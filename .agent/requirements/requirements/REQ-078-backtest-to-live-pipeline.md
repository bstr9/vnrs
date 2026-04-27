---
id: REQ-078
title: "回测→实盘部署管道"
status: active
level: epic
priority: P0
cluster: deployment-pipeline
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-26T10:00:00"
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-077]
  related_to: [REQ-062, REQ-057]
  refined_by: [REQ-079]
versions:
  - version: 1
    date: "2026-04-26T10:00:00"
    author: user
    context: "用户反馈功能没有串联起来，回测完的策略无法直接部署到实盘"
    reason: "初始提出"
    snapshot: "建立回测→参数优化→实盘部署的完整管道"
---

# 回测→实盘部署管道

## 描述
当前回测和实盘是两个完全割裂的世界：
- `get_engine_type()` 硬编码 "LIVE"（已部分修复）
- 优化后的参数无法应用到运行中的策略
- 没有 paper trading 模式
- 没有 LiveDeploymentConfig 从回测结果生成
- 没有一键从回测切换到实盘的工具

需要建立完整的管道：回测 → 参数优化 → 模拟交易 → 实盘部署

## 验收标准
- [ ] StrategyTemplate 支持 set_parameters() 动态修改参数
- [ ] BacktestingEngine 能生成 LiveDeploymentConfig
- [ ] StrategyEngine 支持 paper trading 模式（拦截订单提交）
- [ ] OptimizationEngine 的最优结果能直接应用到新策略实例
- [ ] 端到端示例：策略 → 回测 → 优化 → paper trading → 实盘
- [ ] cargo check/clippy/test 零错误零警告
