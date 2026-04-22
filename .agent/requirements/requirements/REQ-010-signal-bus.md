---
id: REQ-010
title: "SignalBus 信号总线"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-011]
  related_to: [REQ-007, REQ-008]
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md AI-Native 架构设计组件。当�?AI 信号（如情绪分析、RL 策略输出）无法与现有策略框架集成。src/signal/ 目录不存在�?
    reason: "AI 信号与传统策略解耦，支持多信号源订阅"
    snapshot: "实现 SignalBus，类型化 Signal + pub/sub 模式，策略可订阅 AI 信号并与传统指标结合"
---

# SignalBus 信号总线

## 描述

SignalBus �?AI 信号与传统策略之间的桥梁。策略可以订�?AI 信号（如情绪分析、RL 策略输出）并与传统指标结合，无需直接依赖模型推理�?
## 验收标准

- [x] 新建 `src/signal/` 目录：mod.rs, bus.rs, types.rs, subscriber.rs
- [x] `Signal` 类型：signal_id, source, symbol, direction, strength, confidence, features, model_version, timestamp
- [x] `SignalDirection` 枚举：Long, Short, Neutral
- [x] `SignalBus`：DashMap<String, Vec<mpsc::Sender<Signal>>> 实现 topic-based pub/sub
- [x] `subscribe(topic, sender)`：策略订阅特定信号源
- [x] `publish(topic, signal)`：信号源发布信号
- [x] �?StrategyEngine 集成：策略可�?on_bar 中读取缓存的信号

## 工作�?
1-2 �?
## 设计参�?
详见 `.sisyphus/plans/development-guide.md` 第五�?5.5 "AI-4 SignalBus 设计"
