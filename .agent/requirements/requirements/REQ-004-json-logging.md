---
id: REQ-004
title: "结构化 JSON 日志"
status: completed
completed_at: "2026-04-22T18:00:00"
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
  - version: 2
    date: "2026-04-22T12:00:00"
    author: ai
    context: "需求审查发现 status=completed 但 0/5 验收标准已勾选。init_logger_with_json() 函数已存在，但无 VNRS_LOG_FORMAT 环境变量支持，默认仍为纯文本。状态回退为 active。"
    reason: "JSON 日志函数存在但缺 VNRS_LOG_FORMAT 环境变量支持和默认纯文本兼容，回退为 active"
    snapshot: "init_logger_with_json() 存在但缺 VNRS_LOG_FORMAT 环境变量支持和默认纯文本兼容"
  - version: 3
    date: "2026-04-22T18:00:00"
    author: ai
    context: "代码验证发现：is_json_format() 读取 VNRS_LOG_FORMAT 环境变量（第43-45行），init_logger() 调用 init_logger_inner(false) 检查环境变量（第175-177行），init_logger_with_json() 强制 JSON 格式（第183-185行），init_logger_inner() 根据 json 标志选择 fmt::layer().json() 或纯文本格式（第51-168行），支持 console+file 双输出。所有验收标准已满足。状态恢复为 completed。"
    reason: "代码验证确认 VNRS_LOG_FORMAT 环境变量支持和双模式日志均已完成"
    snapshot: "完整 JSON 日志支持：VNRS_LOG_FORMAT 环境变量控制、init_logger_with_json() 强制模式、console+file 双输出"
---

# 结构化 JSON 日志

## 描述

当前 `logger.rs` 使用 `tracing_subscriber::fmt::layer()` 纯文本格式。添加 JSON 格式选项，便于日志分析系统（ELK、Grafana Loki 等）消费。

## 验收标准

- [x] `init_logger_with_json(json_format: bool)` 或等效函数
- [x] 通过环境变量 `VNRS_LOG_FORMAT=json` 控制
- [x] JSON 格式包含：timestamp, level, target, message, spans 等标准字段
- [x] 默认仍为纯文本格式（向后兼容）
- [x] 无新依赖（tracing-subscriber 已支持 `.json()`）

## 工作量

约 0.5 天

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第四节 4.3 "P1-5 JSON 结构化日志"
