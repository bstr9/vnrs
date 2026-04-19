---
id: REQ-030
title: "Python 策略参数管理（parameters/variables）"
status: active
created_at: "2026-04-19T14:00:00"
updated_at: "2026-04-19T14:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-020]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T14:00:00"
    author: ai
    context: "代码分析发现 Rust StrategyTemplate 有 parameters() 和 variables() 方法（template.rs:254-258），用于策略参数管理。vnpy 的 CtaTemplate 也有 parameters/variables 概念（用于 UI 显示策略参数）。Python 策略当前通过 setting dict 设置参数，但无标准化的参数/变量接口。"
    reason: "策略需要标准化的参数管理接口，支持 UI 展示、持久化、动态调整"
    snapshot: "Python Strategy 添加 parameters/variables 属性和 get_parameter/set_parameter 方法，支持策略参数的标准化管理"
---

# Python 策略参数管理（parameters/variables）

## 描述

当前 Python 策略的参数管理方式：
- `CtaStrategy.__init__` 通过 `setting` dict 设置属性（setattr），无类型信息
- `SpotStrategy.__init__` 同样通过 `setting` dict
- 无标准化的参数定义、验证、UI 展示接口

vnpy 的 CtaTemplate 有：
- `parameters` — 策略可配置参数列表（如 `["window", "multiplier"]`），用于 UI 展示输入框
- `variables` — 策略运行时变量列表（如 `["pos", "cross_count"]`），用于 UI 实时展示

nautilus_trader 使用 `Config` dataclass 做参数定义。

## 验收标准

- [ ] Python `Strategy` 添加 `parameters` 类属性（子类覆盖，声明可配置参数）
- [ ] Python `Strategy` 添加 `variables` 类属性（子类覆盖，声明运行时变量）
- [ ] `get_parameter(name)` / `set_parameter(name, value)` 方法
- [ ] 参数类型验证（如 int/float/str/bool）
- [ ] 引擎可通过 `strategy.parameters` 获取参数列表用于 UI 展示
- [ ] 向后兼容现有 setting dict 方式

## 影响范围

- `src/python/strategy.rs` — parameters/variables 属性
- `src/python/cta_strategy.py` — CtaStrategy parameters/variables 实现
- `src/python/spot_strategy.py` — SpotStrategy parameters/variables 实现
