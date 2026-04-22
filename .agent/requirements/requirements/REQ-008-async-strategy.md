---
id: REQ-008
title: "AsyncStrategy 异步策略接口"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-012]
  related_to: [REQ-007]
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 分析发现 StrategyTemplate 是同步回调，ML 推理 (ONNX, gRPC, LLM API) 本质上是 async，可能耗时 10-500ms，会阻塞整个事件循环。strategy/engine.rs process_tick_event �?strategy.on_tick(tick, context) 是阻塞调用�?
    reason: "ML 推理需�?async 接口，当前同步回调会阻塞事件循环"
    snapshot: "实现 AsyncStrategy trait，支�?async on_bar/on_tick，含 Weight-centric 接口�?DecisionRecord 审计追踪"
---

# AsyncStrategy 异步策略接口

## 描述

当前 `StrategyTemplate` (`template.rs:241`) 是同�?trait，`on_tick`/`on_bar` �?`&mut self` 同步回调。当策略需要调�?ML 推理（ONNX、gRPC、LLM API）时，这些操作本质上�?async 的，可能耗时 10-500ms，会阻塞事件循环�?
### 6.1 同步回调陷阱

```rust
// 当前（strategy/engine.rs:178-191�?fn process_tick_event(&self, tick: &TickData) {
    let mut strategies = self.strategies.blocking_write();
    strategy.on_tick(tick, context);  // 阻塞在这�?}
```

### Weight-centric 接口 (FinRL-X)

策略-执行合约应为组合权重向量，而非离散动作�?```rust
fn target_weights(&self) -> HashMap<String, f64> {
    // {"BTCUSDT": 0.3, "ETHUSDT": 0.2, "USDT": 0.5}
}
```

## 验收标准

- [x] 定义 `AsyncStrategy` trait：async on_init/on_bar/on_tick
- [x] `target_weights()` 方法：返回组合权重向量（统一 RL/LLM/传统策略接口�?- [x] `drain_decisions()` 方法：返�?`Vec<DecisionRecord>`
- [x] `DecisionRecord` 类型：timestamp, strategy, signal, confidence, features_used, model_version, inference_latency_us, orders_generated
- [x] StrategyEngine 改造：支持 async 回调（tokio::spawn�?- [x] 保留现有同步 StrategyTemplate 不变（向后兼容）
- [x] 同步/异步策略可共�?
## 工作�?
2-3 �?
## 设计参�?
详见 `.sisyphus/plans/development-guide.md` 第五�?5.3 "AI-2 AsyncStrategy 设计"
