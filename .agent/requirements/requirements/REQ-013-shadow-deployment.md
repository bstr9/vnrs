---
id: REQ-013
title: "Shadow Deployment 影子部署"
status: active
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-2
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-009]
  merged_from: []
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md Phase 2 设计中包含 Shadow Deployment（P2-4）。新模型 shadow 模式：记录预测但不交易，是模型安全上线的核心机制。"
    reason: "模型安全上线需要影子模式验证"
    snapshot: "实现 Shadow Deployment：新模型在 shadow 模式下记录预测但不交易，对比实际结果评估模型质量"
---

# Shadow Deployment 影子部署

## 描述

新模型上线前需要验证阶段。Shadow 模式下模型接收实盘数据并产生预测，但预测不转化为实际交易，仅记录以便与实际市场结果对比。

这是 ModelStage 状态机中的关键阶段（Development → Staging → Shadow → Canary → Production）。

## 验收标准

- [ ] ModelStage::Shadow 阶段定义
- [ ] Shadow 模式下：模型接收数据 → 产生预测 → 记录到 DecisionAudit → **不产生交易**
- [ ] 预测 vs 实际结果的对比分析
- [ ] 影子模型性能指标：准确率、夏普、最大回撤（虚拟）
- [ ] 从 Shadow → Canary 的晋升条件（性能达标自动建议）
- [ ] 与 ModelRegistry (REQ-009) 和 DecisionAudit 集成

## 依赖

- REQ-009 ModelRegistry（ModelStage 状态机）
- REQ-008 AsyncStrategy（DecisionRecord 审计）

## 工作量

1-2 天
