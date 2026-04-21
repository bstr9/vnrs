---
id: REQ-062
title: "策略模板系统（StrategyTemplate + 生命周期管理）"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P0
level: epic
cluster: Strategy
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-020, REQ-025, REQ-030]
  related_to: [REQ-021, REQ-056]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现策略模板系统已实现 CtaTemplate 基础模板、期货策略模板、网格策略模板，以及完整的策略生命周期管理，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "策略模板系统包含 StrategyTemplate trait、4种策略类型、完整生命周期管理、策略引擎、风控配置"
---

# 策略模板系统（StrategyTemplate + 生命周期管理）

## 描述

实现 vnpy 风格的策略模板系统，提供统一的策略开发框架。包含 `StrategyTemplate` trait、多种策略模板实现（Base、Futures、Grid、Volatility）、策略引擎（`StrategyEngine`）以及完整的生命周期管理。

核心设计灵感来源于 vnpy 的 `CtaTemplate`，支持 Rust 原生策略和 Python 策略（通过 PyO3 绑定）。

## 验收标准

### StrategyTemplate Trait（核心接口）

- [x] `strategy_name()` 获取策略名称
- [x] `vt_symbols()` 获取订阅合约列表
- [x] `strategy_type()` 获取策略类型
- [x] `state()` 获取当前状态
- [x] `parameters()` 获取策略参数字典
- [x] `variables()` 获取策略变量字典

### 生命周期回调

- [x] `on_init()` 策略初始化回调
- [x] `on_start()` 策略启动回调
- [x] `on_stop()` 策略停止回调
- [x] `on_tick()` Tick 数据回调
- [x] `on_bar()` Bar 数据回调
- [x] `on_depth()` 深度/订单簿数据回调（可选，默认空实现）
- [x] `on_bars()` 多合约 Bar 回调（可选，默认调用 on_bar）
- [x] `on_order()` 委托状态回调
- [x] `on_trade()` 成交回调
- [x] `on_stop_order()` 止损单触发回调
- [x] `on_indicator()` 指标更新回调（可选）

### 订单管理接口

- [x] `drain_pending_orders()` 获取待发委托队列
- [x] `drain_pending_stop_orders()` 获取待发止损单队列
- [x] `drain_pending_cancellations()` 获取待撤单队列
- [x] `update_position()` 更新持仓
- [x] `get_position()` 获取当前持仓
- [x] `get_target()` / `set_target()` 目标仓位管理（目标仓位策略）

### 策略类型（StrategyType）

- [x] Spot - 现货交易策略
- [x] Futures - 期货 CTA 策略
- [x] Grid - 网格交易策略
- [x] MarketMaking - 做市策略
- [x] Arbitrage - 套利策略

### 策略状态（StrategyState）

- [x] NotInited - 未初始化
- [x] Inited - 已初始化未启动
- [x] Trading - 交易中
- [x] Stopped - 已停止
- [x] Error - 错误状态

### BaseStrategy 基础实现

- [x] 持仓跟踪（positions, targets）
- [x] 活跃委托跟踪（active_orderids, active_stop_orderids）
- [x] 待发委托队列（pending_orders, pending_stop_orders, pending_cancellations）
- [x] `buy()` 买入开仓/现货买入
- [x] `sell()` 卖出平仓/现货卖出
- [x] `short()` 卖出开仓（期货做空）
- [x] `cover()` 买入平仓（期货平空）
- [x] `cancel_order()` 撤销委托
- [x] `cancel_all()` 撤销所有委托
- [x] `send_stop_order()` 发送止损单
- [x] `cancel_stop_order()` 撤销止损单
- [x] `write_log()` 写入日志
- [x] `sync_position()` 同步持仓数据

### FuturesStrategy 期货策略模板

- [x] OffsetMode 枚举（OpenFirst, CloseYesterdayFirst, LockMode）
- [x] 今昨仓分离跟踪（long_td, long_yd, short_td, short_yd）
- [x] 冻结仓位管理（long_td_frozen, long_yd_frozen, short_td_frozen, short_yd_frozen）
- [x] `buy_open()` 买入开仓
- [x] `buy_close()` 买入平仓（支持 SHFE/INE 拆单）
- [x] `sell_close()` 卖出平仓（支持 SHFE/INE 拆单）
- [x] `short_open()` 卖出开仓
- [x] `cover()` 买入平仓（别名）
- [x] `requires_offset_split()` 检测是否需要今昨仓拆分
- [x] `close_long_split()` 多头平仓拆分（CloseToday/CloseYesterday）
- [x] `close_short_split()` 空头平仓拆分（CloseToday/CloseYesterday）
- [x] `update_position_from_trade()` 从成交更新今昨仓

### GridStrategy 网格策略模板

- [x] GridStatus 枚举（Pending, Active, Filled, Cancelled）
- [x] GridLevel 网格级别结构
- [x] 网格参数（center_price, grid_step, grid_count, grid_volume）
- [x] `init_grid()` 初始化网格级别
- [x] `place_pending_orders()` 下挂单
- [x] `handle_fill()` 成交后自动下反向单
- [x] `get_grid_pnl()` 获取已实现盈亏
- [x] `get_filled_count()` 获取已成交级别数
- [x] `get_active_count()` 获取活跃级别数

### VolatilityStrategy 波动率突破策略

- [x] ATR 波动率指标
- [x] NATR 波动率过滤（低波动不交易）
- [x] 布林带突破入场
- [x] 动态移动止盈（trailing stop）
- [x] 固定止损（stop loss）
- [x] ArrayManager 集成

### TargetPosTemplate 目标仓位模板

- [x] `calculate_target()` 计算目标仓位
- [x] `rebalance_portfolio()` 调仓至目标仓位
- [x] `get_min_volume()` 获取最小下单量

### StrategyContext 上下文

- [x] Tick 缓存（tick_cache）
- [x] Bar 缓存（bar_cache）
- [x] 历史 Bar 缓存（historical_bars）
- [x] 数据库集成（load_bar）
- [x] 指标注册与派发（register_indicator, update_indicators）
- [x] `get_tick()` / `get_bar()` / `get_bars()` 数据访问

### StrategyEngine 策略引擎

- [x] `add_strategy()` 添加策略
- [x] `init_strategy()` 初始化策略（含历史数据加载）
- [x] `start_strategy()` 启动策略
- [x] `stop_strategy()` 停止策略（含撤单）
- [x] `remove_strategy()` 移除策略
- [x] `send_order()` 发送委托（含风控检查）
- [x] `cancel_strategy_order()` 撤销委托
- [x] `process_pending_orders()` 处理待发委托
- [x] `process_pending_stop_orders()` 处理待发止损单
- [x] `process_pending_cancellations()` 处理待撤单
- [x] `process_all_pending()` 处理所有待处理动作
- [x] 事件路由（tick/bar/order/trade/depth）

### 策略引擎高级功能

- [x] 止损单管理（register_stop_order, cancel_strategy_stop_order）
- [x] 多周期 Bar 合成（register_bar_synthesizer）
- [x] PnL 跟踪（get_strategy_pnl, get_strategy_unrealized_pnl, get_strategy_total_pnl）
- [x] 交易计数（get_strategy_trade_count）
- [x] 平仓冻结管理（freeze_close_volume, get_frozen_close_volume, get_available_position）
- [x] Symbol-Strategy 双向映射
- [x] OrderID-Strategy 映射（委托回调路由）
- [x] 成交去重（LRU 淘汰机制）

### StrategyRiskConfig 策略风控配置

- [x] max_order_volume 单笔最大下单量
- [x] max_position_volume 最大持仓量
- [x] max_order_notional 单笔最大名义价值
- [x] max_active_orders 最大活跃委托数
- [x] 检查开关（check_order_volume, check_position_volume, check_order_notional, check_active_orders）
- [x] `unrestricted()` 无限制配置
- [x] `conservative_spot()` 保守现货配置
- [x] `conservative_futures()` 保守期货配置

### Python 集成（PyO3）

- [x] `add_python_strategy()` 添加 Python 策略适配器
- [x] `get_context_caches()` 获取上下文缓存（供 Python 访问）

### 单元测试覆盖

- [x] StrategyContext 测试
- [x] BaseStrategy 测试（buy/sell/short/cover/cancel）
- [x] FuturesStrategy 测试（今昨仓跟踪、拆单检测）
- [x] GridStrategy 测试（网格初始化、参数配置）
- [x] VolatilityStrategy 测试（创建、初始化、Bar 处理）
- [x] StrategyEngine 测试（生命周期、PnL 跟踪、冻结管理）
