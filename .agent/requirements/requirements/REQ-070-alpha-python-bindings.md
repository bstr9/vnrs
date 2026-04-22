---
id: REQ-070
title: "Alpha 模块暴露——Python API + GUI 量化研究面板"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T21:00:00"
priority: P3
level: story
cluster: Alpha
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-058]
  merged_from: []
  refined_by: []
  related_to: [REQ-055, REQ-069]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 Alpha 模块 Python 绑定完全无暴露。"
    reason: "录入 Alpha 模块 Python 暴露需求"
    snapshot: "Python 通过 PyAlphaModule 访问 ML 模型训练、因子分析和回测集成"
  - version: 2
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认全覆盖——Alpha 研究也需要 GUI 面板。优先级 P3（专业量化研究员功能，可后置）。"
    reason: "补充 GUI 量化研究面板需求"
    snapshot: "Python API + GUI 量化研究面板（模型训练、因子分析、Alpha 组合）"
---

# Alpha 模块 Python 绑定——量化研究平台 Python API

## 描述

Alpha 模块（`src/alpha/`）是 vnrs 的量化研究平台，包含：
- ML 模型：LinearRegression、Ridge、Lasso、RandomForest、XGBoost
- 因子分析：因子计算、截面分析、Alpha 组合
- 数据管道：Polars 高性能数据处理

但 Python 绑定**完全无暴露**。Python 量化研究员无法使用这些功能。

## 验收标准

### ML 模型 Python 接口
- [x] `PyAlphaModel` 基类暴露 `fit(X, y)` 训练方法
- [x] `PyAlphaModel` 基类暴露 `predict(X)` 预测方法
- [x] 暴露 LinearRegression / Ridge / Lasso 模型类
- [x] 暴露 RandomForest 模型类
- [x] 暴露 XGBoost 模型类（如已实现）

### 因子分析 Python 接口
- [x] 暴露因子计算函数
- [x] 暴露截面分析函数
- [x] 暴露 Alpha 组合权重计算

### 集成
- [x] Python 可从 Alpha 模型输出直接创建策略信号
- [x] Python 可将因子分析结果传递给回测引擎

### GUI 量化研究面板
- [x] 新增"Alpha 研究"标签页
- [x] 模型训练面板：选择模型类型、配置超参数、开始训练、显示训练进度
- [x] 因子分析面板：选择因子、运行截面分析、显示因子分布图
- [x] Alpha 组合面板：查看组合权重、回测 Alpha 信号
- [x] 模型管理：保存/加载/删除已训练模型

## 影响范围

- `src/python/bindings.rs` — 添加 PyAlphaModule 类
- `src/python/` — 可能需要新增 `alpha.rs` 绑定文件
- `src/alpha/` — 可能需要添加 Python 友好的接口
- `src/trader/ui/` — 新增 Alpha 研究 GUI 面板
