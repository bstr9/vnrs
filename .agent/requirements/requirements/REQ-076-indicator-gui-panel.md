---
id: REQ-076
title: "技术指标 GUI 面板——指标选择器 + 参数配置 + 图表叠加"
status: completed
created_at: "2026-04-22T21:00:00"
updated_at: "2026-04-22T21:00:00"
priority: P2
level: story
cluster: GUI
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-055, REQ-061]
  merged_from: []
  refined_by: []
  related_to: [REQ-069]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T21:00:00"
    author: user
    context: "用户确认全覆盖——技术指标需要 GUI 面板。ArrayManager 已有 25+ 指标（REQ-055），图表系统已有基础（REQ-061），但指标选择器、参数配置、图表叠加功能缺失。"
    reason: "技术指标 GUI 面板独立需求"
    snapshot: "指标选择器面板——选择指标、配置参数、叠加到 K 线图"
---

# 技术指标 GUI 面板——指标选择器 + 参数配置 + 图表叠加

## 描述

ArrayManager（`src/trader/utility.rs`）已实现 25+ 技术指标，图表系统（`src/chart/`）已有 K 线图基础。但 GUI 缺少：

1. **指标选择器**——用户无法选择要显示的指标
2. **参数配置**——用户无法调整指标参数（如 SMA 周期、MACD 快慢线）
3. **图表叠加**——指标无法叠加到 K 线图上

## 验收标准

### 指标选择器面板
- [x] 新增"指标"面板，位于图表区域侧边或顶部
- [x] 指标列表分类显示：趋势（SMA/EMA/MACD/ADX）、震荡（RSI/KDJ/CCI）、波动（ATR/布林带）、成交量（OBV/VWAP）
- [x] 勾选指标即叠加到当前图表
- [x] 支持同时显示多个指标

### 参数配置
- [x] 点击指标名称弹出参数配置对话框
- [x] SMA：周期 n（默认 20）
- [x] EMA：周期 n（默认 20）
- [x] MACD：fast（12）、slow（26）、signal（9）
- [x] RSI：周期 n（默认 14）
- [x] 布林带：周期 n（20）、标准差 dev（2.0）
- [x] ATR：周期 n（默认 14）
- [x] 参数修改后实时重绘

### 图表叠加
- [x] 趋势指标（SMA/EMA/布林带）叠加到主图 K 线
- [x] 震荡指标（RSI/MACD/KDJ/CCI）显示在副图
- [x] 指标线颜色可配置
- [x] 指标数值实时显示在图例区域
- [x] 右键指标可移除

### Python 联动
- [x] REQ-069 的 PyArrayManager 指标计算与 GUI 指标面板共享数据
- [x] Python 策略注册的指标自动出现在 GUI 指标面板

## 影响范围

- `src/trader/ui/` — 新增指标选择器面板、参数配置对话框
- `src/chart/` — 指标叠加渲染
- `src/trader/utility.rs` — ArrayManager（已就绪，无需改动）
