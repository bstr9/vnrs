---
id: REQ-037
title: "Agent Layer 模块"
status: active
created_at: "2026-04-20T00:00:00"
updated_at: "2026-04-20T00:00:00"
priority: P3
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-011]
  merged_from: []
  depends_on: [REQ-033, REQ-034]
  cluster: MCP
versions:
  - version: 1
    date: "2026-04-20T00:00:00"
    author: ai
    context: "MCP 分析后明确 Agent Layer 定位：内部 Agent 通过 McpBridge 调用外部 LLM，与 TradingMcpServer（对外暴露）形成双轨架构。细化 REQ-011。"
    reason: "实现内部 Agent 模块，让 vnrs 内部的 AI Agent 能够调用外部 LLM 进行决策"
    snapshot: "实现 Agent trait、McpBridge 客户端、RiskAgent 和 SentimentAgent 示例"
---

# Agent Layer 模块

## 描述

Agent Layer 是 vnrs 的内部 AI Agent 模块，与 TradingMcpServer 形成双轨架构：

```
┌─────────────────────────────────────────────────────────────┐
│                     外部 LLM 客户端                          │
│            (Claude Desktop / Web / 自定义 Client)            │
└─────────────────────────┬───────────────────────────────────┘
                          │ MCP Protocol
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              TradingMcpServer (对外暴露)                     │
│  Tools / Resources / Prompts / Sampling                     │
└─────────────────────────┬───────────────────────────────────┘
                          │ 直接调用
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   vnrs Core Engine                          │
└─────────────────────────┬───────────────────────────────────┘
                          │ 调用
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                 Agent Layer (本需求)                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ RiskAgent    │  │ SentimentAgent│  │ ExecutionAgent│      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │
│         │                 │                 │                │
│         └─────────────────┼─────────────────┘                │
│                           ▼                                  │
│              McpBridge (LLM 调用客户端)                      │
│         通过 Sampling 请求外部 LLM 进行推理                   │
└─────────────────────────────────────────────────────────────┘
```

### 核心洞察（来自 REQ-011）

**Architecture > Model**（TradingAgents 研究发现）：
- 改变 agent 架构对收益的影响 20-40%
- LLM backbone (GPT-4 vs Claude vs Llama) 差异 <5%
- 架构设计（信息流、决策链、反馈机制）是关键

### 与 TradingMcpServer 的区别

| 组件 | 方向 | 用途 |
|------|------|------|
| TradingMcpServer | Server ← 外部 LLM | 外部 LLM 操作 vnrs |
| McpBridge | Agent → 外部 LLM | 内部 Agent 请求 LLM 推理 |

## 目录结构

```
src/agent/
├── mod.rs              # 模块入口
├── traits.rs           # Agent trait 定义
├── types.rs            # AgentType, AgentConfig, AgentResult
├── mcp_bridge.rs       # McpBridge 客户端
├── risk_agent.rs       # RiskAgent 实现
├── sentiment_agent.rs  # SentimentAgent 实现
└── execution_agent.rs  # ExecutionAgent 实现（可选）
```

## 核心组件

### 1. Agent Trait

```rust
pub trait Agent: Send + Sync {
    fn agent_name(&self) -> &str;
    fn agent_type(&self) -> AgentType;
    
    /// 观察市场状态，收集信息
    async fn observe(&mut self, context: &AgentContext) -> Result<Observation>;
    
    /// 基于观察做出决策
    async fn decide(&mut self, observation: &Observation) -> Result<Decision>;
    
    /// 接收决策反馈，更新内部状态
    async fn feedback(&mut self, result: &DecisionResult) -> Result<()>;
}

pub enum AgentType {
    SentimentAnalyst,
    TechnicalAnalyst,
    RiskAssessor,
    RLTrader,
    ExecutionOptimizer,
}
```

### 2. McpBridge（LLM 客户端）

```rust
pub struct McpBridge {
    client: McpClient,  // MCP 客户端
    config: McpBridgeConfig,
}

impl McpBridge {
    /// 通过 MCP Sampling 请求 LLM 推理
    pub async fn request_reasoning(&self, prompt: &str, params: SamplingParams) -> Result<String>;
    
    /// 调用外部 MCP Server 的工具
    pub async fn call_tool(&self, tool: &str, args: Value) -> Result<Value>;
}
```

### 3. RiskAgent 示例

```rust
pub struct RiskAgent {
    bridge: McpBridge,
    risk_threshold: f64,
}

impl Agent for RiskAgent {
    async fn observe(&mut self, context: &AgentContext) -> Result<Observation> {
        // 收集组合风险数据
        let positions = context.engine.get_positions();
        let margin = context.engine.get_margin_info();
        Ok(Observation { positions, margin, ... })
    }
    
    async fn decide(&mut self, observation: &Observation) -> Result<Decision> {
        // 通过 McpBridge 请求 LLM 风险评估
        let prompt = format!("风险评估请求: {:?}", observation);
        let analysis = self.bridge.request_reasoning(&prompt, Default::default()).await?;
        
        // 解析 LLM 建议并生成决策
        Ok(Decision::RiskAdjustment { analysis, ... })
    }
    
    async fn feedback(&mut self, result: &DecisionResult) -> Result<()> {
        // 记录决策结果，用于学习
        Ok(())
    }
}
```

### 4. SentimentAgent 示例

```rust
pub struct SentimentAgent {
    bridge: McpBridge,
    news_sources: Vec<NewsSource>,
}

impl Agent for SentimentAgent {
    async fn observe(&mut self, context: &AgentContext) -> Result<Observation> {
        // 拉取新闻数据
        let news = self.fetch_news().await?;
        Ok(Observation { news, ... })
    }
    
    async fn decide(&mut self, observation: &Observation) -> Result<Decision> {
        // 通过 McpBridge 进行情绪分析
        let prompt = format!("分析以下新闻的市场情绪: {:?}", observation.news);
        let sentiment = self.bridge.request_reasoning(&prompt, Default::default()).await?;
        
        // 将情绪写入 FeatureStore
        Ok(Decision::SentimentSignal { sentiment, ... })
    }
}
```

## 验收标准

- [ ] 新建 `src/agent/` 目录结构
- [ ] `Agent` trait：agent_name, agent_type, async observe/decide/feedback
- [ ] `AgentType` 枚举：SentimentAnalyst, TechnicalAnalyst, RiskAssessor, RLTrader, ExecutionOptimizer
- [ ] `McpBridge`：MCP 客户端，支持 Sampling 和 Tool 调用
- [ ] `RiskAgent` 实现：组合风险分析
- [ ] `SentimentAgent` 实现：新闻情绪分析
- [ ] 与 MainEngine 集成：Agent 可访问交易引擎状态
- [ ] 可选 feature flag `agent`
- [ ] Agent 配置文件：`config/agents.toml`

## 与现有需求的关系

- **refines REQ-011**: 细化 AI Agent Layer 设计
- **依赖 REQ-033**: 需要 MCP HTTP/SSE Transport 支持 McpBridge 连接
- **依赖 REQ-034**: 需要 MCP Sampling 支持请求 LLM 推理

## 工作量

3-5 天

## 设计参考

- REQ-011 AI Agent 层（原始需求）
- `.sisyphus/plans/development-guide.md` 第五节 5.6 "AI-5 Agent Layer 设计"
- TradingAgents 多智能体辩论框架
