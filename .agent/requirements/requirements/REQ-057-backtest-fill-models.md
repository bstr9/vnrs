---
id: REQ-057
title: "回测引擎：填充模型、模拟交易所、保证金与统计"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T12:00:00"
priority: P0
level: epic
cluster: Backtesting
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: [REQ-062, REQ-059]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现回测引擎已实现5种填充模型、模拟交易所撮合、5种保证金模型、完整统计指标，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "回测引擎包含5种填充模型、模拟交易所撮合、5种保证金模型、完整绩效统计"
---

# 回测引擎：填充模型、模拟交易所、保证金与统计

## 描述

回测引擎提供从订单填充仿真到绩效统计的完整回测链路。核心设计遵循 nautilus_trader 架构：填充模型可插拔、模拟交易所按合约独立撮合、保证金按阶梯计算、统计指标覆盖夏普/索提诺/卡尔马比率及最大回撤等。事件循环通过严格时序（先撮合历史挂单，再回调策略）防止前视偏差。

## 验收标准

### 填充模型（fill_model.rs）

- [x] FillModel trait 定义统一接口：simulate_limit_fill、simulate_market_fill、simulate_stop_fill、simulate_tick_fill
- [x] FillResult 结构体包含 filled、fill_price、fill_qty、slippage、liquidity_side、prob_fill
- [x] LiquiditySide 枚举区分 Maker/Taker/NoLiquidity
- [x] BestPriceFillModel：乐观填充，限价单按委托价成交，市价单按 bar 高/低价成交，可配置固定滑点
- [x] IdealFillModel：零滑点理想填充，限价单按委托价、市价单按收盘价、停损单按触发价成交
- [x] TwoTierFillModel：两档流动性模型，按订单量级分小单/大单两档滑点和填充概率（prob_base/prob_large）
- [x] SizeAwareFillModel：基于订单量占 bar 成交量比计算市场冲击，滑点 = base_slippage + impact_coefficient * (order_size / bar_volume)，支持部分成交
- [x] ProbabilisticFillModel：概率填充模型，限价单按 prob_fill_on_limit 概率成交，可配置 prob_slippage 和随机种子
- [x] 所有模型处理停损单时考虑跳空（gap handling）：使用 max(trigger_price, close_price) 计算最差成交价
- [x] 所有模型支持 tick 级别填充（simulate_tick_fill），使用 bid/ask 价格判断
- [x] is_limit_marketable 和 is_stop_triggered 默认方法提供 bar 范围判断
- [x] Box\<dyn FillModel\> 实现 Clone（clone_box 模式）

### 模拟交易所（simulated_exchange.rs）

- [x] FeeModel trait：calculate_fee 接口，按流动性方向区分费率
- [x] MakerTakerFeeModel：Maker/Taker 不同费率，Binance 默认 0.1%/0.1%
- [x] FlatFeeModel：每笔固定手续费
- [x] PercentFeeModel：按成交额百分比计费
- [x] NoFeeModel：零手续费（测试用）
- [x] LatencyModel trait：latency_ms 接口
- [x] ZeroLatency：零延迟即时执行
- [x] FixedLatency：固定延迟（毫秒）
- [x] RandomLatency：均匀分布随机延迟（回测取中值保证确定性）
- [x] InstrumentConfig：按合约配置 pricetick、size、fill_model、fee_model
- [x] InstrumentMatchingEngine：按合约独立撮合引擎，维护各自订单簿（limit_orders、stop_orders、active_limit_orders、active_stop_orders）
- [x] InstrumentMatchingEngine 支持 submit_order、submit_stop_order、cancel_order
- [x] InstrumentMatchingEngine 支持 process_bar 和 process_tick 返回成交列表
- [x] InstrumentMatchingEngine 独立 Position 追踪，apply_fill 更新仓位
- [x] SimulatedExchange：顶层交易所抽象，路由订单到对应合约撮合引擎
- [x] SimulatedExchange 集成 RiskEngine 做提交前风控检查
- [x] SimulatedExchange 支持默认手续费模型和按合约手续费模型
- [x] SimulatedExchange 支持 process_bar_all 和 process_tick_all 跨合约批量撮合
- [x] SimulatedExchange.calculate_fee：优先使用合约级 fee_model，回退到交易所默认

### 回测引擎集成（engine.rs）

- [x] BacktestingEngine 集成 Box\<dyn FillModel\>，默认 BestPriceFillModel
- [x] BacktestingEngine 可选 SimulatedExchange（enable_simulated_exchange / enable_simulated_exchange_with_fee）
- [x] 事件循环时序防前视偏差：update_dt → cross_pending_orders → update_indicators → strategy.on_bar()
- [x] 支持 Bar 和 Tick 两种回测模式
- [x] 支持 StopLimit 订单类型（触发后按限价单撮合）
- [x] 支持 BacktestEmulatedOrder（TrailingStopPct、TrailingStopAbs、MIT、LIT）
- [x] 支持 BacktestBracketGroup（Bracket、OCO、OTO）组合单状态机
- [x] 确定性时钟 TestClock 替代 current_dt，保证 research-to-live 一致性
- [x] 集成 RiskEngine 做提交前风控（size/position/daily limits）
- [x] 集成 Position 追踪：apply_fill 更新仓位、frozen_close_volume 冻结平仓量
- [x] DailyResult 按日汇总，new_day/close_day 自动切换
- [x] 支持从 CSV、PostgreSQL、Binance REST API、BaseDatabase 加载历史数据
- [x] 部分成交处理：fill_qty < order.volume 时保留订单继续挂单

### 保证金模型（margin_model.rs）

- [x] MarginModel trait：initial_margin、maintenance_margin、check_margin 接口
- [x] MarginCheckResult：is_sufficient、initial_margin、maintenance_margin、available_balance、reason
- [x] MarginBracket：按名义价值区间定义保证金率（notional_floor、notional_cap、initial_rate、maintenance_rate、addl_margin）
- [x] LinearMarginModel：恒定费率模型，initial_margin = qty * price * initial_rate
- [x] TieredMarginModel：阶梯保证金模型，按名义价值区间递进计算，自动重算 addl_margin 累加值
- [x] TieredMarginModel 支持 futures_only 模式跳过非期货品种
- [x] NoMarginModel：零保证金模型（现货模式，始终返回充足）
- [x] CannedMarginModel：固定费率模型，initial/maintenance 使用同一费率
- [x] BinanceUsdmMarginModel：Binance USDT-M 合约默认阶梯保证金，含 default_brackets、leverage_20x、leverage_10x 预设
- [x] Box\<dyn MarginModel\> 实现 Clone（clone_box 模式）

### 绩效统计（statistics.rs）

- [x] calculate_statistics 从 DailyResult 集合计算完整统计
- [x] Sharpe Ratio：年化超额收益 / 收益标准差
- [x] Sortino Ratio：年化超额收益 / 下行标准差（仅负收益）
- [x] Calmar Ratio：年化收益 / 最大回撤
- [x] Maximum Drawdown：最大回撤金额和百分比
- [x] Win Rate：盈利交易占比
- [x] Profit Factor：总盈利 / 总亏损
- [x] Average Trade PnL、Average Winning/Losing Trade
- [x] Largest Winning/Losing Trade
- [x] Max Consecutive Wins/Losses
- [x] Daily 平均值：daily_net_pnl、daily_commission、daily_slippage、daily_turnover、daily_trade_count
- [x] Return Mean：日均收益率年化
- [x] Return Std：日收益率标准差
- [x] calculate_max_drawdown 独立函数从余额序列计算回撤
- [x] calculate_returns 独立函数从余额序列计算收益率
- [x] calculate_sharpe_ratio 独立函数从收益率序列计算夏普比率

### 组合回测（portfolio.rs）

- [x] PortfolioBacktestingEngine 支持多品种同时回测
- [x] SymbolConfig 按品种配置 size、pricetick、min_volume
- [x] K-way merge 迭代器（BarMergeIterator、TickMergeIterator）按时间戳合并多品种数据
- [x] PortfolioDailyResult 按日汇总各品种仓位、成交、PnL
- [x] PortfolioStatistics 汇总组合级和品种级统计
- [x] SymbolStatistics 按品种统计 trade_count、total_pnl、position
