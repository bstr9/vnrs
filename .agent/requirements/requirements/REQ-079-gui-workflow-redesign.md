---
id: REQ-079
title: "GUI工作流重设计：端到端交易体验"
status: completed
level: epic
priority: P1
cluster: gui-workflow
created_at: "2026-04-26T10:00:00"
updated_at: "2026-04-30T18:00:00"
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
  - version: 2
    date: "2026-04-30T18:00:00"
    author: sisyphus
    context: "REQ-079全部6项验收标准已实现并验证通过"
    reason: "完成实现"
    snapshot: "所有6项验收标准已实现：工作流引导栏、面板联动(SharedWorkflowState)、错误反馈(toast+验证)、键盘快捷键(F5/F6/Ctrl+Shift+*)、状态持久化(UiState→.rstrader/ui_state.json)、cargo check/clippy/test全部通过"
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
- [x] 工作流引导：新用户能在30分钟内完成首次策略回测
- [x] 面板联动：回测结果自动传递给优化面板，优化结果可一键部署
- [x] 错误反馈：所有操作失败有清晰的错误提示
- [x] 键盘快捷键：常用操作支持快捷键
- [x] 状态持久化：面板状态、窗口布局可保存/恢复
- [x] cargo check/clippy/test 零错误零警告

## 实现详情

### 工作流引导
- 新增 `WorkflowProgress` 结构体，7步工作流进度自动检测
- 顶部工作流引导栏：①连接网关→②订阅行情→③选择策略→④回测验证→⑤优化参数→⑥部署交易→⑦监控管理
- 每步颜色编码：绿色(已完成)、蓝色(当前)、灰色(待完成)
- 点击步骤按钮可跳转到对应面板
- 视图菜单中可切换显示/隐藏，引导栏右上角有关闭按钮

### 面板联动
- 激活 `workflow_state` 模块（之前为死代码），声明并重新导出
- `MainWindow` 添加 `SharedWorkflowState` 字段，共享给 `BacktestingPanel` 和 `AlphaPanel`
- 回测面板添加 "🚀 一键部署到模拟交易" 按钮
- Alpha研究面板添加 "📤 发送到回测" 按钮
- `MainWindow::process_workflow_actions()` 处理所有 `WorkflowAction` 变体
- `StrategyPanel` 添加 `set_deploy_config()` 接收部署配置并显示高亮部署横幅

### 错误反馈
- 策略操作失败时添加 toast 通知（之前只有 tracing::error!）
- 回测面板添加 `backtest_error_flag`，后台线程失败时设置错误标志
- 回测面板添加输入验证：vt_symbol 非空、capital > 0、日期范围有效
- Alpha训练失败时通过 `error_flag` → `take_error()` → toast 通知
- TradingWidget 限价单价格验证 (> 0) 和数量验证 (> 0)

### 键盘快捷键
- 新增 Ctrl+Shift+B(回测)、Ctrl+Shift+S(策略)、Ctrl+Shift+O(高级委托)、Ctrl+Shift+R(远程监控)、Ctrl+Shift+A(告警)、Ctrl+Shift+G(组合单)
- 新增 F5(运行回测)、F6(部署到模拟交易)、Ctrl+S(保存UI状态)
- 保留原有 Ctrl+0~9、Ctrl+L/B/P/N、Escape 快捷键
- 帮助菜单"快捷键"子菜单更新显示所有快捷键

### 状态持久化
- `PanelState`、`CentralTab`、`BottomTab` 添加 `serde::Serialize/Deserialize`
- 新增 `UiState` 结构体：panels + central_tab + bottom_tab + dark_mode + 面板尺寸
- `UiState::save()` / `UiState::load()` 持久化到 `.rstrader/ui_state.json`
- 启动时自动加载，每 ~300 帧自动保存（带变更检测）
