---
id: REQ-011
title: "AI Agent 层"
status: active
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-3
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-009, REQ-010]
  merged_from: []
  refined_by: [REQ-037]
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md AI-Native 架构设计组件。参考 TradingAgents (51K stars) 多智能体辩论框架、AI-Trader 的 SKILL.md agent 接入模式。Architecture > Model：改变 agent 架构对收益的影响 (20-40%) 远大于改变 LLM 骨干 (<5%)。src/agent/ 目录不存在。"
    reason: "AI Agent 是 LLM 时代量化交易的核心交互模式"
    snapshot: "实现 Agent trait（observe/decide/feedback），MCP Bridge LLM 工具调用，SentimentAgent 和 RiskAssessorAgent 示例"
---

# AI Agent 层

## 描述

Agent 层是 AI-Native 架构的最上层，提供标准化的 Agent 接口和 MCP 协议桥接。

### 核心洞察

**Architecture > Model**（TradingAgents 研究发现）：
- 改变 agent 架构对收益的影响 20-40%
- LLM backbone (GPT-4 vs Claude vs Llama) 差异 <5%
- 架构设计（信息流、决策链、反馈机制）是关键

### LLM 集成模式（按实用性排序）

| 模式 | 延迟 | 适用性 |
|------|------|--------|
| Sentiment → Feature | 秒-分钟 | **最佳起点** |
| LLM as Code Generator | 分钟 | 研究用 |
| LLM as Risk Assessor | 分钟 | 低频 |
| LLM as Decision Maker | 秒 | 高延迟、高成本 |
| LLM as Debate Agent | 30-60s | 研究/教育 |

## 验收标准

- [ ] 新建 `src/agent/` 目录：mod.rs, agent.rs, mcp_bridge.rs, sentiment.rs, risk.rs, types.rs
- [ ] `Agent` trait：agent_name, agent_type, async observe/decide/feedback
- [ ] `AgentType` 枚举：SentimentAnalyst, TechnicalAnalyst, RiskAssessor, RLTrader, DebateParticipant
- [ ] `McpBridge`：集成 MCP protocol，LLM 工具调用
- [ ] `SentimentAgent` 示例：异步拉取新闻 → 调用 LLM → 写入 FeatureStore
- [ ] `RiskAssessorAgent` 示例：定期组合风险分析 → 生成风险报告
- [ ] 可选 feature flag `agent`

## 工作量

3-5 天

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第五节 5.6 "AI-5 Agent Layer 设计"
