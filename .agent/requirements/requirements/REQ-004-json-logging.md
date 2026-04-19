---
id: REQ-004
title: "结构化 JSON 日志"
status: active
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-003]
  cluster: Infrastructure
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发现 tesser 已实现 JSON 结构化日志，vnrs 当前 logger.rs 使用纯文本 tracing_subscriber::fmt::layer()，无 .json() 层。日志分析和告警系统需要结构化格式。"
    reason: "日志分析和告警需要结构化格式"
    snapshot: "支持 JSON 结构化日志格式，通过环境变量或配置切换"
---

# 结构化 JSON 日志

## 描述

当前 `logger.rs` 使用 `tracing_subscriber::fmt::layer()` 纯文本格式。添加 JSON 格式选项，便于日志分析系统（ELK、Grafana Loki 等）消费。

## 验收标准

- [ ] `init_logger_with_json(json_format: bool)` 或等效函数
- [ ] 通过环境变量 `VNRS_LOG_FORMAT=json` 控制
- [ ] JSON 格式包含：timestamp, level, target, message, spans 等标准字段
- [ ] 默认仍为纯文本格式（向后兼容）
- [ ] 无新依赖（tracing-subscriber 已支持 `.json()`）

## 工作量

约 0.5 天

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第四节 4.3 "P1-5 JSON 结构化日志"
