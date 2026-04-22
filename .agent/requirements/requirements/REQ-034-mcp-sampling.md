---
id: REQ-034
title: "MCP Sampling 支持"
status: completed
created_at: "2026-04-20T00:00:00"
updated_at: "2026-04-20T00:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-036, REQ-037]
  related_to: [REQ-033]
  cluster: MCP
versions:
  - version: 1
    date: "2026-04-20T00:00:00"
    author: ai
    context: "MCP 分析后发�?Sampling 未实现。这�?MCP 最强大的特性，允许 Server 反向请求 LLM 进行推理。用户确认需�?Sampling�?
    reason: "实现 Agentic 行为，Server 可请�?LLM 辅助决策"
    snapshot: "实现 MCP Sampling 能力，Server 可通过 ctx.session.create_message() 请求 LLM 推理"
---

# MCP Sampling 支持

## 描述

**Sampling �?MCP 最强大的特�?* �?允许 Server 反向请求 LLM 进行推理，实现真正的 Agentic 行为�?
### MCP 五大原语对比

| 原语 | 方向 | vnrs 状�?| 用�?|
|------|------|----------|------|
| Tools | LLM→Server | �?13�?| 交易执行/查询 |
| Resources | Server→Client | �?10�?| 实时数据暴露 |
| Prompts | 用户控制 | �?| 标准化分析模�?|
| **Sampling** | **Server→LLM** | �?| **Server请求LLM推理** |
| Roots/Elicitation | Client能力 | �?| 文件边界/用户输入 |

### Sampling 工作流程

```
1. Tool 被调用时，需�?LLM 辅助决策
2. Tool 通过 ctx.session.create_message() 发送请�?3. Client 转发�?LLM（Claude/GPT等）
4. LLM 返回分析结果
5. Human-in-the-loop 可审�?拒绝
```

### 典型用例

#### 1. 异常事件处理
```rust
#[tool]
async fn handle_market_anomaly(&self, symbol: String, anomaly: String, ctx: Context) -> McpResult<String> {
    let prompt = format!("检测到 {} 异常: {}。当前持�? {}。建议操作？", 
        symbol, anomaly, self.get_position_summary(&symbol));
    let response = ctx.session.create_message(
        messages![user!(prompt)],
        SamplingParams { max_tokens: 500, ..default() },
    ).await?;
    self.record_llm_suggestion(&symbol, response.content);
    Ok(response.content)
}
```

#### 2. 新闻情绪分析
```rust
#[tool]
async fn analyze_news_impact(&self, news: String, symbols: Vec<String>, ctx: Context) -> McpResult<String> {
    let prompt = format!("分析新闻对以下品种的影响:\n品种: {:?}\n新闻: {}", symbols, news);
    let response = ctx.session.create_message(...).await?;
    Ok(response.content)
}
```

#### 3. 策略参数建议
```rust
#[tool]
async fn suggest_strategy_params(&self, strategy_id: String, metrics: BacktestMetrics, ctx: Context) -> McpResult<String> {
    // 基于 backtest 结果请求 LLM 给出优化建议
}
```

## 验收标准

- [x] 实现 `ctx.session.create_message()` Sampling API
- [x] 添加 Sampling 参数配置（max_tokens, temperature, model preference�?- [x] 实现 human-in-the-loop 审批机制
- [x] 添加 Sampling 调用日志和审�?- [x] 示例 Tool: `analyze_sentiment` 使用 Sampling
- [x] 示例 Tool: `suggest_strategy_params` 使用 Sampling
- [x] 错误处理：LLM 不可用时�?fallback

## 安全考虑

1. **Human-in-the-loop**: 所�?Sampling 请求需用户审批
2. **速率限制**: 防止过度调用 LLM
3. **审计日志**: 记录所�?Sampling 请求和响�?4. **敏感信息**: 不在 prompt 中暴�?API key、密码等

## 工作�?
1-2 �?
## 设计参�?
- MCP Sampling 规范：https://modelcontextprotocol.io/specification/2025-03-26/client/sampling
- OKX Agent Trade Kit Sampling 示例
