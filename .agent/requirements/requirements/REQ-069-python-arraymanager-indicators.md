---
id: REQ-069
title: "Python ArrayManager 技术指标暴露"
status: completed
created_at: "2026-04-22T20:00:00"
updated_at: "2026-04-22T20:00:00"
priority: P2
level: story
cluster: Python-API
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-055]
  merged_from: []
  refined_by: []
  related_to: [REQ-062, REQ-058]
  depends_on: []
versions:
  - version: 1
    date: "2026-04-22T20:00:00"
    author: ai
    context: "集成审计发现 ArrayManager（src/trader/utility.rs）实现 25+ 技术指标（SMA/EMA/MACD/RSI/ATR/布林带等），但 Python 绑定完全无暴露。Python 策略无法使用任何技术指标计算功能。"
    reason: "录入 Python ArrayManager 指标暴露需求"
    snapshot: "Python 通过 PyArrayManager 访问 25+ 技术指标计算"
---

# Python ArrayManager 技术指标暴露

## 描述

ArrayManager（`src/trader/utility.rs`）是 vnrs 的核心技术指标计算模块，提供 25+ 技术指标：
- 趋势指标：SMA、EMA、MACD、ADX
- 震荡指标：RSI、KDJ、CCI、WR
- 波动指标：ATR、布林带
- 成交量指标：OBV、VWAP

但 Python 绑定中**完全无暴露**，Python 策略无法使用这些指标。

## 验收标准

### PyArrayManager 类
- [x] `PyArrayManager` 类暴露 `update(bar_data)` 方法
- [x] `PyArrayManager` 类暴露 `inited` 属性（数据是否足够计算指标）
- [x] `PyArrayManager` 类暴露 `size` 属性（已缓存数据条数）

### 趋势指标
- [x] `sma(n)` 简单移动平均
- [x] `ema(n)` 指数移动平均
- [x] `macd(fast, slow, signal)` MACD 指标
- [x] `adx(n)` 平均趋向指标

### 震荡指标
- [x] `rsi(n)` 相对强弱指标
- [x] `kdj(n, m1, m2)` 随机指标
- [x] `cci(n)` 商品通道指标

### 波动指标
- [x] `atr(n)` 真实波幅
- [x] `boll(n, dev)` 布林带（upper, middle, lower）

### 集成
- [x] Python 策略可通过 StrategyContext 获取 PyArrayManager
- [x] PyArrayManager 在 on_bar 中自动更新
- [x] 添加 Python 使用 ArrayManager 的示例

## 影响范围

- `src/python/bindings.rs` — 添加 PyArrayManager 类
- `src/python/` — 可能需要新增 `arraymanager.rs` 绑定文件
- `examples/` — Python 指标策略示例
