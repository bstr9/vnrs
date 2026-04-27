---
id: REQ-084
title: "策略止盈止损自动管理"
status: active
level: story
priority: P0
cluster: strategy-execution
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-083]
  refines: []
  related_to: [REQ-063, REQ-067]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "量化策略开仓后必须设置止盈止损来控制风险。StopOrderEngine和BracketOrderEngine已存在，但策略层缺少一键'开仓+自动挂止盈止损'的高级接口。"
    reason: "没有止盈止损的量化策略等于裸奔，一次极端行情就可能爆仓"
    snapshot: "策略开仓后能自动挂止盈止损单，支持固定价位和ATR动态止损"
---

# 策略止盈止损自动管理

## 描述
量化策略开仓后必须立即设置止盈止损来控制单笔风险。当前 `StopOrderEngine` 和 `BracketOrderEngine` 已实现，但策略层缺少便捷的高级接口。

需要实现：
1. `buy_with_stop(vt_symbol, price, volume, stop_loss, take_profit)` — 开仓同时挂止盈止损
2. `sell_with_stop(vt_symbol, price, volume, stop_loss, take_profit)` — 同上做空版
3. 支持固定价位止盈止损（如入场价±2%）
4. 支持 ATR 动态止损（如入场价 - 2×ATR）
5. 支持移动止损（trailing stop）：价格朝有利方向移动时，止损线跟随上移
6. 止盈止损单与开仓单关联，开仓成交后自动激活
7. 止损触发后自动取消对应的止盈单，反之亦然

## 验收标准
- [ ] 策略调用 `buy_with_stop()` 后，开仓成交自动挂止损和止盈单
- [ ] 止损单触发后，对应的止盈单自动取消
- [ ] 止盈单触发后，对应的止损单自动取消
- [ ] 支持移动止损：每次新高/新低时自动修改止损价位
- [ ] 支持 ATR 动态止损距离
- [ ] GUI 上能看到策略关联的止盈止损单状态
- [ ] 策略可通过 `cancel_stop_orders(vt_symbol)` 取消所有关联的止盈止损单
