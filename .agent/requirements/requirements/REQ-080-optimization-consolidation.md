---
id: REQ-080
title: "优化模块整合与Python绑定完善"
status: completed
level: story
priority: P1
cluster: strategy-unification
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-26T10:00:00"
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-077]
  merged_from: []
  depends_on: []
  related_to: [REQ-057, REQ-069]
  refined_by: []
versions:
  - version: 1
    date: "2026-04-26T10:00:00"
    author: ai
    context: "分析发现trader/optimize.rs是死代码，与backtesting/optimization.rs功能重叠"
    reason: "清理冗余代码，统一优化接口"
    snapshot: "整合两个优化模块，废弃trader/optimize.rs，增强backtesting/optimization.rs"
---

# 优化模块整合与Python绑定完善

## 描述
当前存在两个优化模块：
- `src/trader/optimize.rs` — 通用优化器，闭包API，与BacktestingEngine无关（死代码）
- `src/backtesting/optimization.rs` — 专用优化器，工厂模式，正确连接BacktestingEngine

需要：
1. 废弃 trader/optimize.rs，将有用逻辑合并到 backtesting/optimization.rs
2. 为 OptimizationEngine 添加 Python 绑定
3. 添加优化结果 → 策略参数应用的桥接方法

## 验收标准
- [x] trader/optimize.rs 被标记为 deprecated 或删除
- [x] OptimizationEngine 有 Python API
- [x] 优化结果可转换为 StrategySetting
- [x] 优化示例（Rust + Python）
- [x] cargo check/clippy/test 零错误零警告
