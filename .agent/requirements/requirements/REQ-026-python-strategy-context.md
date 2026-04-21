---
id: REQ-026
title: "StrategyContext 暴露给 Python 策略（含数据查询方法）"
status: completed
completed_at: "2026-04-21T00:00:00"
created_at: "2026-04-19T14:00:00"
updated_at: "2026-04-19T19:04:20"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: [REQ-031, REQ-032]
  depends_on: [REQ-020]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T14:00:00"
    author: ai
    context: "代码分析发现 Rust 端 StrategyContext 提供 get_tick/get_bar/get_bars/load_bar 等关键方法，但 Python 策略无法访问。Python Strategy 仅有 self.engine (PyAny) 间接调用，无法直接获取缓存数据。nautilus_trader Strategy 可直接访问 cache 对象。"
    reason: "Python 策略需要访问缓存的市场数据（最新 tick/bar、历史 bars），当前无法获取"
    snapshot: "将 StrategyContext 暴露为 Python 对象，策略通过 self.context 访问 get_tick/get_bar/get_bars 等方法"
  - version: 2
    date: "2026-04-20T10:00:00"
    author: ai
    context: "需求整理合并：REQ-031(get_bars) 和 REQ-032(get_tick/get_bar) 是 REQ-026 的验收标准子项，合并以消除冗余。"
    reason: "合并 REQ-031、REQ-032 到验收标准"
    snapshot: "StrategyContext 暴露给 Python，含 get_tick/get_bar/get_bars/load_bar 全部数据查询方法"
  - version: 3
    date: "2026-04-19T19:04:20"
    author: ai
    context: "元数据自动同步"
    reason: "自动补充反向关系: refined_by"
    snapshot: "自动同步元数据"
---

# StrategyContext 暴露给 Python 策略（含数据查询方法）

## 描述

Rust 端 `StrategyContext`（`src/strategy/template.rs`）提供了丰富的市场数据访问方法，但 Python 策略完全无法访问。当前 Python 策略只能依赖 `on_bar(bar)` 回调参数（且为 dict），无法主动查询数据。

**核心痛点：**
- 计算技术指标（MA/RSI/Bollinger）需要多根 Bar → 缺 get_bars()
- on_order/on_trade 中需要当前价格 → 缺 get_tick/get_bar()
- 策略初始化需要加载历史数据 → 缺 load_bar()

nautilus_trader Strategy 可直接访问 cache 对象获取所有市场数据。

## 验收标准

- [x] 新增 `PyStrategyContext` PyO3 类，包装 `StrategyContext`
- [x] Python `Strategy` 添加 `self.context` 属性（注入时机与 portfolio 一致）
- [x] `context.get_tick(vt_symbol)` → 返回 PyTickData 或 None（原 REQ-032）
- [x] `context.get_bar(vt_symbol)` → 返回 PyBarData 或 None（原 REQ-032）
- [x] `context.get_bars(vt_symbol, count)` → 返回 List[PyBarData]，按时间升序（原 REQ-031）
  - 回测模式：从 BacktestingEngine 历史数据缓存获取
  - 实盘模式：从 StrategyContext 的 historical_bars 缓存获取
  - 无数据时返回空列表
- [x] `context.load_bar(vt_symbol, days, interval)` → 返回 List[PyBarData]（从数据库/缓存加载）
- [x] 回测引擎和实盘引擎均注入 context
- [x] 与 REQ-020（类型化数据类）对齐

## 影响范围

- 新增 `src/python/context.rs` — PyStrategyContext 定义
- `src/python/strategy.rs` — 添加 context 属性
- `src/python/backtesting_bindings.rs` — 注入 context
- `src/python/bindings.rs` — 注入 context（实盘）
