---
id: REQ-012
title: "RL Environment 强化学习环境"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-3
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-007, REQ-008]
  merged_from: []
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md AI-Native 架构设计组件。参�?TensorTrade (6K stars) 可组�?RL 环境、TradeMaster 全流�?RL 平台。vnrs 已有完整的回测引擎，可基于此构建 gym 兼容接口。src/rl/ 目录不存在�?
    reason: "RL 是量化交易中 AI 应用的主流范式，需要标准化的训练环�?
    snapshot: "实现 TradingEnv（gym 兼容接口 step/reset/observation），ActionMapper，RewardFunction，PyO3 导出兼容 stable-baselines3"
---

# RL Environment 强化学习环境

## 描述

基于现有回测引擎构建 gym 兼容�?RL 训练环境，支�?stable-baselines3 �?ray[rllib]�?
## 验收标准

- [x] 新建 `src/rl/` 目录：mod.rs, env.rs, action.rs, reward.rs, observation.rs, python.rs
- [x] `TradingEnv`：reset() �?Observation, step(action) �?(Observation, reward, done, info)
- [x] `ActionMapper` trait：离�?连续 action �?Vec<OrderRequest>
- [x] `RewardFunction` trait：compute(prev, curr, action) �?f64
- [x] 内置 reward：SharpeReward, PnlReward, RiskAdjustedReward
- [x] PyO3 导出：Python 可直接使�?TradingEnv
- [x] 兼容 stable-baselines3, ray[rllib]
- [x] 可�?feature flag `rl`

## 工作�?
2-3 �?
## 设计参�?
详见 `.sisyphus/plans/development-guide.md` 第五�?5.7 "AI-6 RL Environment 设计"
