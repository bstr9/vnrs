---
id: REQ-073
title: "Python OffsetConverter 暴露——期货今昨仓拆单接口"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P3
level: story
cluster: Python-API
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-056]
  merged_from: []
  refined_by: []
  related_to: [REQ-021, REQ-062]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 OffsetConverter 在 Rust 侧已完整实现并集成到 MainEngine.send_order() 流程，但 Python 端无法直接使用。Python 期货策略需要手动处理今昨仓拆单逻辑，或依赖 Rust 侧自动转换。"
    reason: "录入 Python OffsetConverter 暴露需求"
    snapshot: "Python 通过 PyOffsetConverter 查询和手动触发今昨仓拆单"
---

# Python OffsetConverter 暴露——期货今昨仓拆单接口

## 描述

OffsetConverter（`src/trader/converter.rs`）在 Rust 侧已完整实现并集成到 MainEngine.send_order() 流程，自动处理 SHFE/INE 今昨仓拆单。但 Python 端无法直接使用 OffsetConverter，Python 期货策略需要手动处理今昨仓拆单逻辑。

## 验收标准

### Python 接口
- [x] PyOffsetConverter 类暴露 convert_order_request() 方法
- [x] Python 可查询某合约是否需要今昨仓拆分
- [x] Python 可查询当前今仓/昨仓持仓量
- [x] Python 通过 engine.offset_converter 访问

### 集成
- [x] MainEngine.send_order() 自动转换对 Python 透明
- [x] Python 策略可手动调用 OffsetConverter 预览拆单结果

## 影响范围

- `src/python/bindings.rs` — 添加 PyOffsetConverter 类
- `src/trader/converter.rs` — 无需改动
