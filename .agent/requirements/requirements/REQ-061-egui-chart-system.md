---
id: REQ-061
title: "egui 交互式图表系统（K线/指标/交易叠加）"
status: completed
source: code-analysis
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
completed_at: "2026-04-22T00:00:00"
priority: P2
level: story
cluster: GUI
related_to: [REQ-055]
versions:
  - version: 1
    author: ai
    context: "代码审查发现 egui 图表系统已实现K线、成交量、指标叠加、交易标记，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "egui 图表系统实现交互式K线图、指标叠加、交易标记"
---

# REQ-061: egui 交互式图表系统（K线/指标/交易叠加）

## 需求描述

基于 egui 实现交互式K线图表系统，支持蜡烛图渲染、成交量柱状图、技术指标叠加、交易标记覆盖层，以及鼠标/键盘交互操作（缩放、平移、十字光标等）。

## 验证清单

### K线蜡烛图渲染

- [x] 蜡烛图实体渲染（阳线空心/阴线实心）
- [x] 上下影线绘制
- [x] 十字星（Doji）处理（开收相等时绘制横线）
- [x] 中式配色方案：红色上涨、绿色下跌
- [x] 价格到Y坐标映射（对数/线性）

### 成交量柱状图

- [x] 成交量柱状图渲染
- [x] 成交量柱按价格涨跌着色（红涨绿跌）
- [x] 成交量范围自动计算

### 内置技术指标

- [x] MA（简单移动平均线）
- [x] EMA（指数移动平均线，支持可组合 update_raw）
- [x] WMA（加权移动平均线）
- [x] BOLL（布林带，Welford 增量方差算法，3条线：上轨/中轨/下轨）
- [x] VWAP（成交量加权平均价）
- [x] SAR（抛物线指标，含翻转检测）
- [x] AVL（均价线）
- [x] TRIX（三重指数平滑，含信号线，使用 EmaState 组合）
- [x] SUPER（SuperTrend 指标，含 ATR 计算）

### 自定义指标

- [x] CustomIndicator 表达式解析器
- [x] 词法分析器（Tokenizer）支持变量 open/high/low/close/volume
- [x] 递归下降解析器支持运算符 +、-、*、/ 及括号
- [x] 表达式验证函数 validate_expression
- [x] IndicatorType 枚举用于 UI 选择

### 指标渲染与配置

- [x] 指标叠加渲染（段式绘制，None 值间隙处理）
- [x] 线型样式：实线（Solid）、虚线（Dashed）、点线（Dotted）
- [x] 指标图例覆盖层
- [x] 主图指标（IndicatorLocation::Main）叠加在K线上
- [x] 副图指标（IndicatorLocation::Sub）独立子图显示
- [x] 副图零线绘制（振荡器类指标如 TRIX）
- [x] 指标配置对话框（周期、乘数、信号周期、颜色、线宽、位置）
- [x] 指标管理面板（添加/删除指标）

### 交易标记覆盖层

- [x] TradeMarker（日期时间、价格、方向、数量）
- [x] TradeDirection 枚举（Buy、Sell、Short、Cover）
- [x] 交易方向配色（买入黄色、卖出绿色、做空/平仓品红色）
- [x] TradePair 开仓/平仓匹配及盈亏连线
- [x] TradeOverlay 从交易列表转换并绘制
- [x] 三角形标记（买入朝上、卖出朝下）
- [x] 盈亏连接线绘制

### 交互操作

- [x] 鼠标滚轮缩放
- [x] 键盘上下箭头缩放
- [x] 鼠标拖拽平移（bar-pixel 精确增量计算）
- [x] 键盘左右箭头平移
- [x] Home/End 键跳转至最旧/最新数据
- [x] 十字光标线（水平 + 垂直）
- [x] Y 轴价格标签
- [x] X 轴日期时间标签
- [x] 信息框显示 OHLCV 数据（定位在光标对侧）
- [x] 迷你滚动条及拇指拖拽
- [x] 自动缩放 Y 轴开关
- [x] 拖拽加载更多历史数据检测（need_more_history 事件）

### 布局与网格

- [x] 主图区域 60% + 成交量区域 15% + 副图区域 25%（有副图指标时）
- [x] 主图区域 75% + 成交量区域 25%（无副图指标时）
- [x] 水平网格线（Y 轴刻度位置）
- [x] 垂直网格线（X 轴刻度位置）
- [x] 左/右边距、Y 轴宽度、滚动条高度、最小K线数量等布局常量

### 工具栏

- [x] 时间周期选择器（1m/5m/15m/1h/4h/1d 等）
- [x] 指标选择器
- [x] 时间范围选择器
- [x] 自动缩放开关按钮
- [x] 保存/加载配置按钮

### 数据管理

- [x] BarManager 日期时间索引的K线数据管理
- [x] HashMap 索引（日期时间 → 序号）
- [x] 有序K线存储
- [x] 缓存价格/成交量范围
- [x] update_history 批量更新
- [x] update_bar 单根更新
- [x] get_index 日期时间查找
- [x] update_history_prepend 前置插入保留滚动位置

### 配置持久化

- [x] ChartConfig 序列化/反序列化
- [x] SerializableIndicatorConfig 指标配置序列化
- [x] 保存配置至 ~/.rstrade/chart_configs/
- [x] 加载配置从 ~/.rstrade/chart_configs/

### 主窗口集成

- [x] ChartWidget 通过 HashMap<String, ChartWidget> 管理
- [x] TickBarAggregator 行情聚合为K线
- [x] 历史数据初始加载 vs 前置插入处理
- [x] 从行情监控双击或仪表盘操作打开图表
- [x] 加载中覆盖层

### 工具函数

- [x] format_price 价格格式化
- [x] format_volume 成交量格式化（K/M/B 单位）
- [x] calculate_axis_ticks 计算美观刻度值
