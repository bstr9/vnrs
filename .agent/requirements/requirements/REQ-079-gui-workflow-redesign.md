---
id: REQ-079
title: "GUI工作流重设计：端到端交易体验"
status: active
level: epic
priority: P1
cluster: gui-workflow
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-26T10:00:00"
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-077, REQ-078]
  related_to: [REQ-061, REQ-070, REQ-071]
  refined_by: []
versions:
  - version: 1
    date: "2026-04-26T10:00:00"
    author: user
    context: "用户反馈GUI非常不好用，14个面板没有串联，没有工作流"
    reason: "初始提出"
    snapshot: "重设计GUI工作流，实现端到端交易体验：连接→策略→回测→部署→监控"
---

# GUI工作流重设计：端到端交易体验

## 描述
当前GUI有14个独立面板，但它们之间没有工作流串联。用户需要：
1. 手动在面板间切换
2. 在一个面板的操作结果无法自动驱动另一个面板
3. 没有引导式的端到端流程

需要重设计GUI工作流，实现：
- 连接网关 → 订阅行情 → 选择/编写策略 → 回测 → 优化 → 部署 → 监控
- 面板之间自动联动（回测完成自动跳转结果，优化完成提示部署）
- 关键操作有引导和确认

## 验收标准
- [ ] 工作流引导：新用户能在30分钟内完成首次策略回测
- [ ] 面板联动：回测结果自动传递给优化面板，优化结果可一键部署
- [ ] 错误反馈：所有操作失败有清晰的错误提示
- [ ] 键盘快捷键：常用操作支持快捷键
- [ ] 状态持久化：面板状态、窗口布局可保存/恢复
- [ ] cargo check/clippy/test 零错误零警告
