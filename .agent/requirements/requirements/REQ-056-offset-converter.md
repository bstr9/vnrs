---
id: REQ-056
title: "OffsetConverter 仓位净ting与SHFE平今平昨"
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
  related_to: [REQ-021]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现 OffsetConverter 已实现仓位净ting、SHFE平今平昨拆分、锁仓模式等功能，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "OffsetConverter 实现开平方向转换、SHFE平今/平昨拆分、锁仓/净仓模式"
---

# OffsetConverter 仓位净ting与SHFE平今平昨

## 描述

OffsetConverter 是期货交易中处理开平方向转换的核心组件。中国期货交易所（SHFE、INE）要求区分平今和平昨，这与其他交易所（CZCE、DCE、CFFEX）只使用 Close 的方式不同。此外，锁仓模式和净仓模式对同一笔平仓单有不同的处理逻辑。

OffsetConverter 管理每个合约的 PositionHolding 状态，根据持仓方向（Long/Short/Net）、持仓时段（今仓/昨仓）、冻结量、活跃委托等信息，将用户发出的平仓请求转换为符合交易所规则的一个或多个子单。

## 验收标准

### Offset 枚举

- [x] `Offset::None` 默认值，无开平标记（现货等场景）
- [x] `Offset::Open` 开仓
- [x] `Offset::Close` 平仓（通用平仓，非SHFE/INE交易所使用）
- [x] `Offset::CloseToday` 平今（SHFE/INE交易所专用）
- [x] `Offset::CloseYesterday` 平昨（SHFE/INE交易所专用）
- [x] Offset 枚举实现 Display（中文：空、"开"、"平"、"平今"、"平昨"）
- [x] Offset 枚举实现 Default（None）、Serialize、Deserialize

### PositionHolding 持仓状态管理

- [x] 按 vt_symbol 维护独立的 PositionHolding 实例
- [x] 跟踪 Long/Short 双向持仓（long_pos, short_pos）
- [x] 区分今仓和昨仓（long_td/long_yd, short_td/short_yd）
- [x] 跟踪冻结量（long_td_frozen/long_yd_frozen, short_td_frozen/short_yd_frozen）
- [x] 维护活跃委托列表（active_orders HashMap）
- [x] `update_position()` 按 Direction 更新持仓：Long 更新多头、Short 更新空头、Net 计算净变化后分配到多或空
- [x] `update_order()` 管理活跃委托并重新计算冻结量
- [x] `update_order_request()` 从委托请求创建 OrderData 并更新
- [x] `update_trade()` 按 offset 类型更新今仓/昨仓，SHFE/INE 的 Close 优先减昨仓，其他交易所优先减今仓溢出到昨仓

### 冻结量计算

- [x] `calculate_frozen()` 遍历所有活跃平仓委托计算冻结量
- [x] CloseToday 委托冻结对应方向的今仓
- [x] CloseYesterday 委托冻结对应方向的昨仓
- [x] Close 委托优先冻结今仓，今仓不足则溢出到昨仓
- [x] `sum_pos_frozen()` 确保冻结量不超过实际持仓量
- [x] 开仓委托（Offset::Open）不参与冻结计算

### SHFE/INE 平今平昨拆分

- [x] `convert_order_request_shfe()` 将 Close 请求拆分为 CloseToday + CloseYesterday
- [x] 平仓量不超过可用持仓时，优先平今仓
- [x] 今仓不足时，剩余部分转为 CloseYesterday
- [x] 平仓量超过总可用持仓时返回空列表（拒绝委托）
- [x] Open 请求在 SHFE 模式下直接返回，不拆分

### 锁仓模式（Lock Mode）

- [x] `convert_order_request_lock()` 实现锁仓逻辑
- [x] 存在今仓时，非SHFE/INE交易所转为开仓（锁仓而非平仓）
- [x] 存在今仓时，SHFE/INE交易所走 CloseYesterday + Open 拆分
- [x] 无今仓时，先平昨仓再开新仓（CloseYesterday/Close + Open）
- [x] 平昨量不超过可用昨仓量
- [x] 超出部分转为 Open 开仓

### 净仓模式（Net Mode）

- [x] `convert_order_request_net()` 实现净仓逻辑
- [x] SHFE/INE交易所：按 CloseToday → CloseYesterday → Open 顺序拆分
- [x] 其他交易所：按 Close → Open 顺序拆分
- [x] 各子单数量不超过对应的可用量（扣除冻结）
- [x] 剩余未平仓量转为 Open 开仓

### OffsetConverter 顶层编排

- [x] `OffsetConverter` 持有 HashMap<String, PositionHolding> 管理所有合约持仓
- [x] 通过 ContractLookup 闭包按需查找合约信息
- [x] `convert_order_request(lock, net)` 根据 lock/net 标志路由到对应转换方法
- [x] 非 lock 非 net 且为 SHFE/INE 时走 SHFE 拆分逻辑
- [x] 非 lock 非 net 且非 SHFE/INE 时原样返回（无需转换）
- [x] `is_convert_required()` 仅对非净仓合约（net_position=false）执行转换
- [x] 合约不在缓存中时按需创建 PositionHolding
- [x] 合约不存在时原样返回请求，不报错

### 测试覆盖

- [x] PositionHolding 创建后所有值为零
- [x] Long/Short 方向 update_position 正确计算今仓昨仓
- [x] Net 方向正净变化增加多头、负净变化增加空头
- [x] Close 委托正确冻结今仓量
- [x] OffsetConverter 无合约时原样返回
- [x] 现货交易原样返回（Offset::None）
- [x] SHFE Close 请求拆分为 CloseToday + CloseYesterday
- [x] Net 模式下超过持仓量的请求拆分为 CloseToday + CloseYesterday + Open

## 影响范围

- `src/trader/converter.rs` — OffsetConverter + PositionHolding 完整实现
- `src/trader/constant.rs` — Offset 枚举定义
