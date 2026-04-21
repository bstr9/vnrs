---
id: REQ-009
title: "ModelRegistry 模型注册与服�?
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: AI-2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-011, REQ-013]
  related_to: [REQ-007]
  cluster: AI-Native
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 分析发现 alpha 模块提供 AlphaModel trait 但无模型版本/注册表、无 train/validate/deploy 流程、无 shadow deployment / A/B testing、无 drift detection、无 checkpoint/rollback。src/model/ 目录不存在�?
    reason: "模型生命周期管理�?AI 生产部署的核心需�?
    snapshot: "实现 ModelRegistry（SQLite 后端）和 ModelServer trait，支�?Development→Staging→Shadow→Canary→Production 阶段流转"
---

# ModelRegistry 模型注册与服�?
## 描述

当前 `AlphaModel` trait 缺少完整的模型生命周期管理。ModelRegistry 提供模型版本管理、阶段流转和推理服务�?
### Rust ML 栈决策矩�?
| 方式 | 延迟 | 依赖 | 用�?|
|------|------|------|------|
| `ort` (ONNX Runtime) | 0.5-15ms | C++ shared lib | 生产模型 (XGBoost, sklearn, PyTorch 导出) |
| `candle` | 10-20ms CPU | �?Rust | 本地 LLM/embedding，GGUF 量化 |
| `tract` | 1-5ms | �?Rust | 小模型，嵌入式部�?|
| gRPC to Triton | 5-50ms | 外部服务�?| GPU 推理，大�?transformer |
| ZMQ to Python | 2-20ms | Python 进程 | 研究迭代，利�?Python ML 生�?|

**推荐**: 默认 `ort` 用于结构化模型，`candle` 用于文本/情绪，ZMQ 桥接 Python 研究�?
## 验收标准

- [ ] 新建 `src/model/` 目录：mod.rs, registry.rs, server.rs, onnx.rs, candle.rs, grpc.rs, types.rs
- [ ] `ModelStage` 枚举：Development, Staging, Shadow, Canary, Production, Archived
- [ ] `ModelEntry`：model_id, version, stage, artifact_path, metrics, feature_ids
- [ ] `ModelRegistry`：SQLite 后端存储模型元数据，阶段状态机
- [ ] `ModelServer` trait：async predict, model_info, health
- [ ] `OnnxModelServer` 实现（需 `ort` crate，可�?feature `ml-inference`�?- [ ] `ZmqModelServer` 实现（利用现�?RPC 基础设施�?- [ ] 可�?feature flags：`ml-inference`, `ml-local`, `ml-tract`, `ml-grpc`, `ml-full`

## 依赖

- `ort` crate（可�?feature `ml-inference`�?- `candle-*` crates（可�?feature `ml-local`�?- `tonic` + `prost`（可�?feature `ml-grpc`�?- **替代方案**：通过现有 ZMQ RPC 调用外部 Python 推理服务（零新依赖）

## 工作�?
2-3 �?
## 设计参�?
详见 `.sisyphus/plans/development-guide.md` 第五�?5.4 "AI-3 ModelRegistry 设计"
