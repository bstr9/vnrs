# Backtest Results Log

This file records backtest results for the Volatility Strategy on BTCUSDT.

---

## Backtest Result — 2026-04-18 14:41:41 UTC (Parameter Sweep Best)

| Item | Value |
|------|-------|
| Strategy | VolBTC_Spot |
| Symbol | BTCUSDT.BINANCE |
| Bars | 30000 |
| Period | 2025-01-01 to 2025-01-21 |
| Total Days | 21 |
| **Sharpe Ratio** | **3.2156** |
| **Max Drawdown** | **141.56 (0.14%)** |
| End Balance | 100306.56 |
| Total Net PnL | 306.56 |
| Total Return | 3.68% |
| Win Rate | 44.44% |
| Profit Factor | 1.6242 |
| Total Trades | 22 |
| Sortino Ratio | 4.1399 |
| Calmar Ratio | 25.9940 |
| Max Consecutive Wins | 2 |
| Max Consecutive Losses | 2 |
| Total Commission | 103.96 |
| Total Slippage | 1.10 |

**Parameters:**
- `atr_length` = 14
- `boll_length` = 30
- `boll_dev` = 2.0
- `natr_threshold` = 0.6
- `tp_atr_mult` = 4.0
- `sl_atr_mult` = 1.0
- `fixed_size` = 0.1
- `am_length` = 100

**Sweep Methodology:**
- Parameter sweep over 144 combinations (boll_dev × natr_threshold × tp_atr_mult × sl_atr_mult × boll_length)
- Synthetic data: 30K 1-minute bars with GBM + mean-reversion + trending drift + volatility regimes
- Best Sharpe achieved with wider Bollinger bands (2.0 dev, 30-period), moderate NATR filter (0.6%), wide TP (4× ATR), tight SL (1× ATR)
- Key insight: fewer but higher-quality trades (22 trades vs 807 at default params) produce positive returns

---
