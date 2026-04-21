---
id: REQ-055
title: "ArrayManager 技术指标库"
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
  related_to: [REQ-061]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现 ArrayManager 已实现 25+ 技术指标，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "ArrayManager 提供 25+ 技术指标计算（SMA/EMA/RSI/MACD/Bollinger 等）"
---

# ArrayManager 技术指标库

## 描述

ArrayManager 是 vnrs 交易引擎的核心时序数据容器，维护滚动 OHLCV 数组并提供 25+ 技术指标计算能力。基于 `ta-rs` crate 实现标准指标，部分复杂指标（ADX/DI、SAR、Aroon、Ultimate Oscillator、BOP）手动实现以保证算法正确性。每个指标同时提供单值和数组两种返回形式，供策略回测和图表渲染使用。

图表层（`src/chart/indicator.rs`）提供独立的增量式指标实现，支持 egui 实时渲染，包含 MA/EMA/WMA/BOLL/VWAP/AVL/TRIX/SAR/SuperTrend 及自定义表达式指标。

## 验收标准

### 数据容器基础

- [x] ArrayManager 维护 7 个滚动数组：open/high/low/close/volume/turnover/open_interest
- [x] 配置化数组长度（size 参数），默认 100
- [x] update_bar() 增量更新，rotate_left 实现环形缓冲
- [x] is_inited() 判断数组是否填满，防止冷启动误算
- [x] 提供 open()/high()/low()/close()/volume()/turnover()/open_interest() 数组访问器

### 移动平均线

- [x] SMA（简单移动平均）- ta-rs SimpleMovingAverage
- [x] SMA 数组版本 sma_array()
- [x] EMA（指数移动平均）- ta-rs ExponentialMovingAverage
- [x] EMA 数组版本 ema_array()

### 动量指标

- [x] RSI（相对强弱指数）- ta-rs RelativeStrengthIndex
- [x] RSI 数组版本 rsi_array()
- [x] ROC（变动率）- ta-rs RateOfChange
- [x] ROC 数组版本 roc_array()
- [x] MOM（动量）- 手动实现，close[last] - close[last-n]

### 波动率指标

- [x] STDDEV（标准差）- ta-rs StandardDeviation
- [x] STDDEV 数组版本 std_array()
- [x] ATR（平均真实波幅）- ta-rs AverageTrueRange
- [x] ATR 数组版本 atr_array()
- [x] TRANGE（真实波幅）- ta-rs TrueRange
- [x] TRANGE 数组版本 trange_array()
- [x] NATR（归一化ATR）= ATR / Close * 100

### 趋势指标

- [x] MACD（移动平均收敛发散）- ta-rs MovingAverageConvergenceDivergence，返回 (macd, signal, histogram)
- [x] MACD 数组版本 macd_array()
- [x] CCI（商品通道指数）- ta-rs CommodityChannelIndex
- [x] CCI 数组版本 cci_array()

### 通道指标

- [x] Bollinger Bands（布林带）- ta-rs BollingerBands，返回 (upper, middle, lower)
- [x] Bollinger Bands 数组版本 boll_array()
- [x] Keltner Channel（肯特纳通道）- ta-rs KeltnerChannel，返回 (upper, middle, lower)
- [x] Keltner Channel 数组版本 keltner_array()
- [x] Donchian Channel（唐奇安通道）- ta-rs Maximum/Minimum 组合，返回 (upper, lower)
- [x] Donchian Channel 数组版本 donchian_array()

### 振荡器

- [x] Fast Stochastic（快速随机振荡）- ta-rs FastStochastic
- [x] Slow Stochastic（慢速随机振荡）- ta-rs SlowStochastic
- [x] Full Stochastic（完整随机振荡）- FastStochastic + SMA 平滑，返回 (%K, %D)
- [x] Williams %R - 手动实现，(highest_high - close) / (highest_high - lowest_low) * -100

### 成交量指标

- [x] OBV（能量潮）- ta-rs OnBalanceVolume
- [x] OBV 数组版本 obv_array()
- [x] MFI（资金流量指数）- ta-rs MoneyFlowIndex
- [x] MFI 数组版本 mfi_array()

### 价格极值

- [x] highest(n) - n 周期最高价
- [x] lowest(n) - n 周期最低价

### 方向性运动指标（手动实现）

- [x] ADX（平均方向指数）- Wilder 平滑法手动实现
- [x] +DI（正方向指标）- plus_di()
- [x] -DI（负方向指标）- minus_di()
- [x] Wilder 平滑辅助函数 wilder_smooth()

### Parabolic SAR（手动实现）

- [x] SAR（抛物线转向指标）- 手动实现，支持 acceleration/maximum 参数
- [x] 多空切换逻辑，前两根K线极值约束

### Aroon 指标（手动实现）

- [x] Aroon Up/Down - 返回 (aroon_up, aroon_down)
- [x] Aroon Oscillator - aroon_up - aroon_down

### Ultimate Oscillator（手动实现）

- [x] 三周期终极振荡器 ultosc(period1, period2, period3)
- [x] Buying Pressure + True Range 加权计算

### Balance of Power（手动实现）

- [x] BOP = (close - open) / (high - low)

### BarDataItem 适配层

- [x] BarDataItem 结构体实现 Open/High/Low/Close/Volume trait
- [x] get_data_item() 为 ta-rs 指标提供 OHLCV 数据桥接

### 图表层指标（src/chart/indicator.rs）

- [x] Indicator trait 统一接口：update/is_ready/current_value/reset/calculate/get_value/get_line_config/get_y_range
- [x] IndicatorBase 公共状态管理：count/initialized/check_initialized
- [x] MA（简单移动平均）- 增量式，VecDeque 窗口
- [x] EMA（指数移动平均）- 增量式，含 update_raw() 子指标组合接口
- [x] WMA（加权移动平均）- 增量式，线性加权
- [x] BOLL（布林带）- 增量式，Welford 在线统计算法
- [x] VWAP（成交量加权平均价格）- 增量式
- [x] AVL（均价线）- (high+low+close)/3
- [x] TRIX（三重指数平滑）- 三层 EmaState 组合 + 信号线
- [x] SAR（抛物线转向）- 增量式，含前后两根K线约束
- [x] SuperTrend - 增量式，SMA-ATR + 趋势方向判断
- [x] CustomIndicator - 表达式解析指标，递归下降解析器
- [x] IndicatorType 枚举（MA/EMA/WMA/BOLL/VWAP/AVL/TRIX/SAR/SUPER）
- [x] LineStyle 支持 Solid/Dashed/Dotted
- [x] 多序列 Y 轴范围计算 get_y_range_for_multi_series()
