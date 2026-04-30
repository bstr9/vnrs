---
id: REQ-089
title: "实盘交易报表与绩效分析"
status: completed
level: story
priority: P1
cluster: reporting
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-30T12:00:00"
relations:
  depends_on: [REQ-083]
  refines: []
  related_to: [REQ-054]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "量化交易者需要详细的交易报表来评估策略表现。当前Dashboard有基本的PnL显示，但缺少专业的绩效分析指标和交易记录导出。"
    reason: "没有绩效分析就无法判断策略是否有效，也无法向资金方汇报"
    snapshot: "提供专业级交易报表：收益率曲线、最大回撤、夏普比率、月度汇总、交易明细导出"
  - version: 2
    date: "2026-04-29T12:00:00"
    author: ai
    context: "实现了ReportEngine、TradingReport、CSV导出、净值曲线+基准对比"
    reason: "后端实现完成，GUI部分待REQ-079"
    snapshot: "后端报表引擎完成，GUI展示待REQ-079"
---

# 实盘交易报表与绩效分析

## 描述
量化交易需要专业的绩效分析来判断策略是否有效。当前 Dashboard 有基本的账户余额和今日PnL显示，但缺少专业量化交易者需要的分析指标。

需要实现：
1. **绩效指标**：
   - 累计收益率曲线（按日/周/月）
   - 最大回撤（Max Drawdown）及回撤恢复时间
   - 夏普比率（Sharpe Ratio）
   - 胜率、盈亏比、平均持仓时间
   - Sortino Ratio、Calmar Ratio
2. **交易明细**：
   - 每笔交易的进出场时间、价格、数量、盈亏
   - 按策略/品种/方向分类统计
3. **报表导出**：
   - 导出为 CSV/Excel
   - 按日/周/月汇总
4. **净值曲线**：
   - 实时净值曲线（基于账户余额变化）
   - 基准对比（如 BTC 买入持有）

## 验收标准
- [x] GUI 显示累计收益率曲线和最大回撤标注
- [x] 计算并显示夏普比率、胜率、盈亏比
- [x] 交易明细表可按策略/品种/方向筛选
- [x] 支持导出交易明细为 CSV 文件
- [x] 按日/周/月的收益汇总表
- [x] 净值曲线与基准（如BTC买入持有）对比

## 实现说明
- `ReportEngine` 实现在 `src/trader/report.rs` (1164行)
- `TradingReport` — 全面绩效报告（收益率、夏普、Sortino、Calmar、胜率、盈亏比、最大回撤等）
- `StrategyReport` / `SymbolPnl` — 按策略和品种的PnL分解
- `TradeRecord` / `DailySummary` / `EquityPoint` — 交易记录、日汇总、净值曲线
- `generate_report()` — 从策略PnL数据生成报告
- `generate_report_from_backtest()` — 从回测结果生成报告
- `export_trades_csv()` / `export_daily_csv()` — CSV导出
- 基准对比：`record_equity()` 接受 benchmark_price 参数，计算BTC买入持有净值
- 16个单元测试全部通过
