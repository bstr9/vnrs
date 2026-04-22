---
id: REQ-075
title: "组合单 GUI 面板——Bracket/OCO/OTO 可视化管理"
status: completed
created_at: "2026-04-22T21:00:00"
updated_at: "2026-04-22T21:00:00"
priority: P1
level: story
cluster: GUI
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-063]
  merged_from: []
  refined_by: []
  related_to: [REQ-064, REQ-066, REQ-067]
  depends_on: [REQ-064]
versions:
  - version: 1
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认全覆盖——组合单（Bracket/OCO/OTO）是交易员常用功能，需要 GUI 面板。从 REQ-066 拆出，因为组合单比模拟单更常用，优先级更高。"
    reason: "组合单 GUI 面板独立需求，P1 优先级"
    snapshot: "组合单 GUI 面板——新建组合单对话框、活跃组合单列表、状态跟踪"
---

# 组合单 GUI 面板——Bracket/OCO/OTO 可视化管理

## 描述

BracketOrderEngine 已实现三种组合单类型（REQ-063），但 GUI 完全无界面。交易员常用操作：

1. **Bracket（括号单）**：入场 + 止盈 + 止损三单联动——最常见的止损止盈设置方式
2. **OCO（一取消全）**：两个挂单，一个成交另一个自动撤销——突破交易常用
3. **OTO（一触发全）**：主单成交后自动提交次单——条件单链

这些是交易员的核心工作流，必须有 GUI 支持。

## 验收标准

### 组合单列表面板
- [ ] 新增"组合单"标签页，展示所有活跃组合单组
- [ ] 列表显示：组ID、类型（Bracket/OCO/OTO）、合约、状态、入场价、止盈价、止损价
- [x] 状态颜色标识：Pending=灰、EntryActive=蓝、SecondaryActive=绿、Completed=绿、Cancelled=红
- [x] 点击组展开子委托详情（角色、委托ID、状态、已成交量）
- [x] 支持撤销整个委托组

### 新建 Bracket 单对话框
- [x] 合约选择器（vt_symbol）
- [x] 方向选择（Long/Short）
- [x] 入场价格/数量/类型（Limit/Market）
- [x] 止盈价格
- [x] 止损价格/止损类型（Stop/StopLimit）
- [x] 提交按钮

### 新建 OCO 单对话框
- [x] 合约选择器
- [x] A 单价格/类型
- [x] B 单价格/类型
- [x] 方向/数量
- [x] 提交按钮

### 新建 OTO 单对话框
- [x] 合约选择器
- [x] 主单方向/价格/数量/类型
- [x] 次单方向/价格/数量/类型
- [x] 提交按钮

### 实时更新
- [x] 委托组状态变更时列表实时更新
- [x] 子委托成交通知（Toast 弹窗）
- [x] 组完成/取消时移出活跃列表

### Python API
- [ ] `PyBracketOrderEngine` 类暴露 `add_bracket_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `add_oco_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `add_oto_order()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `cancel_group()` 方法
- [ ] `PyBracketOrderEngine` 类暴露 `get_active_groups()` 查询方法
- [ ] Python 通过 `engine.bracket_order_engine` 访问

## 影响范围

- `src/trader/ui/` — 新增组合单管理面板、新建对话框
- `src/python/bindings.rs` — 添加 PyBracketOrderEngine 类
- `src/python/` — 可能需要新增 `bracket_order.rs` 绑定文件
