---
id: REQ-007
title: "FeatureStore 特征存储"
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
  related_to: [REQ-008, REQ-009, REQ-055, REQ-058]
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md AI-Native 架构设计的核心组件。当�?ArrayManager 内联计算 40+ 指标，无特征版本管理、无特征共享、无训练/服务一致性保证。src/feature/ 目录不存在�?
    reason: "ML/AI 特征管理基础，解�?ArrayManager 的三大问题：无版本、无共享、无一致�?
    snapshot: "实现 FeatureStore，含 Online Store (DashMap, <1us �?、Offline Store (Parquet)、Feature Registry (定义/版本/血�?、Time-Travel 快照"
---

# FeatureStore 特征存储

## 描述

解决当前 `ArrayManager` 的三大问题：
1. **无特征版本管�?* �?无法追踪特征定义变更
2. **无特征共�?* �?两个策略使用相同特征会独立计�?3. **无训�?服务一致性保�?* �?回测时和实盘时的特征计算可能不同

FeatureStore �?AI-Native 架构的基础层，为模型推理提供统一的特征访问接口�?
## 验收标准

- [x] 新建 `src/feature/` 目录结构：mod.rs, store.rs, online.rs, offline.rs, registry.rs, snapshot.rs, types.rs
- [x] `FeatureId` 类型（如 `"btcusdt_close_price_1m"`�?- [x] `FeatureVector`：entity + timestamp + HashMap<FeatureId, f64>
- [x] `FeatureDefinition`：id + expression + version + dependencies + dtype
- [x] `OnlineStore`：DashMap<String, FeatureVector>�?1us �?- [x] `OfflineStore`：Parquet 存储，回测用
- [x] `FeatureRegistry`：定义、版本、血缘追�?- [x] `get_online()`：实时特征获�?- [x] `get_at()`：时间旅行（回测用）
- [x] `materialize()`：从 Bar/Tick 计算并存储特�?- [x] `snapshot()`：创建快�?- [x] 可�?feature flag `feature-store`

## 依赖

- `dashmap` �?已在项目中使用，无新增依�?
## 工作�?
3-5 �?
## 设计参�?
详见 `.sisyphus/plans/development-guide.md` 第五�?5.2 "AI-1 FeatureStore 设计"
