---
id: REQ-063
title: "止损单引擎 + 模拟订单 + 组合单（Bracket/OCO/OTO）"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P1
level: story
cluster: Core-Trading
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: [REQ-025, REQ-017]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现止损单引擎（stop_order.rs, 674行）、模拟订单引擎（order_emulator.rs, 1291行）、组合单引擎（bracket_order.rs, 1035行）已完整实现，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "止损单引擎 + 模拟订单引擎 + 组合单引擎实现本地条件单、高级订单类型和组合委托管理"
---

# 止损单引擎 + 模拟订单 + 组合单（Bracket/OCO/OTO）

## 描述

vnrs 实现了三套本地订单管理引擎，覆盖交易所不原生支持的高级订单类型：

1. **StopOrderEngine**（stop_order.rs）：管理本地止损/止盈/追踪止损条件单，监控行情触发后自动提交真实委托
2. **OrderEmulator**（order_emulator.rs）：模拟交易所不支持的订单类型——追踪止损、止损限价、冰山单、MIT、LIT
3. **BracketOrderEngine**（bracket_order.rs）：组合单管理——Bracket（入场+止盈+止损）、OCO（一成交另一自动撤销）、OTO（一成交触发另一）

三套引擎均实现 BaseEngine trait，通过 GatewayEvent 接收行情和委托更新，通过回调函数提交/撤销真实委托。

## 验收标准

### StopOrderEngine（stop_order.rs）

- [x] StopOrderType 枚举：StopMarket、StopLimit、TrailingStopPct、TrailingStopAbs、TakeProfit
- [x] StopOrderStatus 枚举：Pending、Triggered、Cancelled、Expired
- [x] StopOrder 结构体：id、symbol、exchange、direction、stop_type、stop_price、limit_price、volume、offset、status、trail_pct/trail_abs、highest_price/lowest_price、gateway_name、reference、created_at、triggered_at、expires_at、tag
- [x] StopOrderRequest 构造器：stop_market()、take_profit()、trailing_stop_pct()、trailing_stop_abs()
- [x] add_stop_order()：参数校验（volume>0, StopLimit需limit_price, TrailingStopPct在(0,1), TrailingStopAbs>0）、原子ID递增、双索引插入（stop_orders + symbol_index）
- [x] cancel_stop_order()：状态检查（仅 Pending 可撤销）
- [x] cancel_orders_for_symbol()：批量撤销指定合约的止损单
- [x] get_stop_order()、get_all_stop_orders()、get_active_stop_orders()、get_stop_orders_for_symbol() 查询方法
- [x] check_trigger()：StopMarket/StopLimit 按方向判断触发，TakeProfit 反向触发，TrailingStop 按当前止损价触发
- [x] update_trailing_stop()：TrailingStopPct 更新 highest/lowest 并计算新止损价（仅单方向移动），TrailingStopAbs 绝对距离版本
- [x] process_tick_internal()：遍历合约止损单、更新追踪、检查触发、执行回调
- [x] process_bar_internal()：使用 bar high/low 触发检查
- [x] register_callback()：触发时回调 (StopOrder, OrderRequest)
- [x] cleanup()：清理非活跃止损单
- [x] BaseEngine 实现：engine_name、close（取消所有 Pending）、process_event（路由 tick/bar）
- [x] 单元测试：10 个测试覆盖 add/cancel/trigger/trailing/cleanup

### OrderEmulator（order_emulator.rs）

- [x] EmulatedOrderType 枚举：TrailingStopPct、TrailingStopAbs、StopLimit、Iceberg、Mit、Lit
- [x] EmulatedOrderStatus 枚举：Pending、Triggered、Completed、Cancelled、Expired、Rejected
- [x] EmulatedOrder 结构体：id、order_type、status、symbol、exchange、direction、offset、volume、remaining_volume、trail_pct/trail_abs、current_stop、highest/lowest_price、trigger_price、limit_price、visible_volume、iceberg_price、real_order_id、created_at、expires_at、gateway_name、reference
- [x] EmulatedOrderRequest 构造器：trailing_stop_pct()、trailing_stop_abs()、stop_limit()、market_if_touched()、limit_if_touched()、iceberg()
- [x] add_order()：按类型校验参数、原子ID递增、冰山单首笔立即提交
- [x] cancel_order()：撤销活跃单、如有真实委托则通过 cancel_callback 撤销、清理 real_order_index
- [x] cancel_orders_for_symbol()：批量撤销指定合约的模拟委托
- [x] get_order()、get_all_orders()、get_active_orders()、get_orders_for_symbol() 查询方法
- [x] set_send_order_callback()、set_cancel_order_callback()：真实委托提交/撤销回调
- [x] update_trailing()：TrailingStopPct 百分比追踪（Ratchet 只升不降）、TrailingStopAbs 绝对距离追踪
- [x] check_trigger()：TrailingStop 按 current_stop 判断、StopLimit 按 trigger_price 判断、MIT/LIT 反向触发
- [x] trigger_order()：通过 send_callback 提交真实委托、更新 real_order_id 和 real_order_index
- [x] submit_iceberg_slice()：冰山单切片提交，计算 min(visible_volume, remaining_volume)
- [x] process_order_update()：真实委托状态更新——AllTraded 时冰山单提交下一切片、Cancelled/Rejected 时更新状态
- [x] process_tick_internal()：逐单检查触发
- [x] process_bar_internal()：Bar high/low 触发检查，区分 Long/Short 方向逻辑
- [x] cleanup()：清理非活跃模拟委托
- [x] real_order_index：真实委托ID → 模拟委托ID 反向索引
- [x] BaseEngine 实现：engine_name、close（撤销所有活跃单）、process_event（路由 tick/bar/order）
- [x] 单元测试：13 个测试覆盖类型显示、请求构造、参数校验、追踪止损逻辑、事件处理

### BracketOrderEngine（bracket_order.rs）

- [x] ContingencyType 枚举：Oco、Oto、Bracket
- [x] OrderGroupState 枚举：Pending、EntryActive、SecondaryActive、Completed、Cancelled、Rejected
- [x] OrderRole 枚举：Entry、TakeProfit、StopLoss、Primary、Secondary、OrderA、OrderB
- [x] ChildOrder 结构体：role、request、vt_orderid、status、filled_volume、avg_fill_price、is_active()、is_fully_filled()
- [x] OrderGroup 结构体：id、contingency_type、state、vt_symbol、gateway_name、orders HashMap、reference、created_at、completed_at、tag、is_active()
- [x] BracketOrderRequest：entry_price/volume/type + tp_price + sl_price/sl_type
- [x] OcoOrderRequest：order_a_price/type + order_b_price/type
- [x] OtoOrderRequest：primary + secondary 方向/价格/数量/类型
- [x] add_bracket_order()：创建 Entry+TP+SL 三单组，先提交 Entry，Entry 成交后提交 TP+SL
- [x] add_oco_order()：创建 A+B 两单组，同时提交，一单成交撤销另一单
- [x] add_oto_order()：创建 Primary+Secondary 两单组，先提交 Primary，成交后提交 Secondary
- [x] cancel_group()：撤销委托组所有活跃子委托
- [x] get_group()、get_all_groups()、get_active_groups() 查询方法
- [x] handle_bracket_fill()：Entry 成交→提交 TP+SL，TP 或 SL 成交→撤销另一出场单并标记组完成
- [x] handle_oco_fill()：一单成交→撤销另一单并标记组完成
- [x] handle_oto_fill()：Primary 成交→提交 Secondary，Secondary 成交→标记组完成
- [x] handle_rejection()：Entry/Primary 被拒→标记组 Rejected
- [x] handle_cancellation()：Entry/Primary 被撤→标记组 Cancelled；TP/SL 被撤→撤销兄弟单；OCO 单被撤→撤销兄弟单
- [x] set_send_order_callback()、set_cancel_order_callback()、set_state_change_callback() 回调
- [x] process_order_update()：匹配 vt_orderid → group，更新子委托状态，路由到对应 fill/reject/cancel 处理
- [x] process_trade()：更新子委托 filled_volume 和 avg_fill_price
- [x] fire_state_change()：状态变更回调通知
- [x] orderid_to_group 反向索引：vt_orderid → GroupId
- [x] cleanup()：清理非活跃委托组
- [x] BaseEngine 实现：engine_name、close（取消所有活跃组）、process_event（路由 order/trade）
- [x] 单元测试：11 个测试覆盖构造、校验、状态显示、组合单创建、事件处理

## 影响范围

- `src/trader/stop_order.rs` — StopOrderEngine（674 行）
- `src/trader/order_emulator.rs` — OrderEmulator（1291 行）
- `src/trader/bracket_order.rs` — BracketOrderEngine（1035 行）
- `src/trader/engine.rs` — MainEngine 集成
- `src/trader/mod.rs` — 模块导出
- `src/backtesting/engine.rs` — 回测集成（BacktestEmulatedOrder、BacktestBracketGroup）
