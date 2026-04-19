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

## Backtest Result — 2026-04-19 (Live Binance Data, Framework Integration Test)

| Item | Value |
|------|-------|
| Strategy | VolBTC_Spot |
| Symbol | BTCUSDT.BINANCE |
| Data Source | Binance REST API (live download) |
| Interval | 1m |
| Period | 2026-04-11 to 2026-04-18 |
| Trading Days | 8 |
| **Sharpe Ratio** | **-6.1884** |
| **Max Drawdown** | **15.33 (0.02%)** |
| End Balance | 99,986.19 |
| Total Net PnL | -13.81 |
| Total Return | -0.01% |
| Win Rate | 20.00% |
| Total Trades | 20 |
| Sortino Ratio | -4.4372 |
| Calmar Ratio | -28.3779 |
| Profit Days | 1 |
| Loss Days | 4 |
| Total Commission | 15.03 |

**Parameters:**
- `atr_length` = 14
- `boll_length` = 20
- `boll_dev` = 2.0
- `natr_threshold` = 0.15
- `tp_atr_mult` = 3.0
- `sl_atr_mult` = 1.5
- `fixed_size` = 0.01
- `am_length` = 50

**Notes:**
- First end-to-end test using `BacktestingEngine::load_data_from_binance()` bridge method
- Data downloaded directly from Binance REST API via DataDownloadManager
- 7-day window with 1m bars (~10K bars), 50-bar ArrayManager window
- Low NATR threshold (0.15) generated 20 signals but most were false breakouts
- Commission dominates PnL at 0.01 BTC size — $15.03 commission vs -$13.81 net loss
- Strategy needs either: larger position size, higher NATR filter, or longer data period
- Framework integration verified: DataDownloadManager → BinanceRestClient → BacktestingEngine → VolatilityStrategy → Statistics pipeline works end-to-end

---
## Backtest Result — 2026-04-19 04:44:37 UTC

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
- `tp_atr_mult` = 4.0
- `sl_atr_mult` = 1.0
- `boll_dev` = 2.0
- `boll_length` = 30
- `am_length` = 100
- `fixed_size` = 0.1
- `natr_threshold` = 0.6

---

## Python Strategies Backtest - 2026-04-19 14:05

**Data**: Synthetic (500 bars, 2% volatility, seed=42)

| Strategy | Sharpe Ratio | Max DD% | PnL ($) | Trades |
|----------|-------------|---------|---------|--------|
| atr_rsi_strategy | 0.0000 | 0.00% | $168414.91 | 5 |
| bitcoin_spot_strategy | 0.0000 | 4.58% | $-4582.78 | 18 |
| boll_channel_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| double_ma_strategy | 0.0000 | 19.91% | $-19910.88 | 22 |
| dual_thrust_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| king_keltner_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| multi_signal_strategy | 0.0000 | 26.43% | $-26431.34 | 25 |
| multi_timeframe_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| test_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| turtle_15min_ema_strategy | 0.0000 | 53.86% | $-53856.48 | 9 |
| turtle_signal_strategy | 0.0000 | 4837.21% | $-4837213.84 | 1000 |

## Python Strategies Backtest - 2026-04-19 15:15

**Data**: Synthetic (500 bars, 2% volatility, seed=42)

| Strategy | Sharpe Ratio | Max DD% | PnL ($) | Trades |
|----------|-------------|---------|---------|--------|
| atr_rsi_strategy | 0.0000 | 0.00% | $168414.91 | 5 |
| bitcoin_spot_strategy | 0.0000 | 4.58% | $-4582.78 | 18 |
| boll_channel_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| double_ma_strategy | 0.0000 | 19.91% | $-19910.88 | 22 |
| dual_thrust_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| king_keltner_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| multi_signal_strategy | 0.0000 | 26.43% | $-26431.34 | 25 |
| multi_timeframe_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| test_strategy | 0.0000 | 0.00% | $0.00 | 0 |
| turtle_15min_ema_strategy | 0.0000 | 53.86% | $-53856.48 | 9 |
| turtle_signal_strategy | 0.0000 | 4837.21% | $-4837213.84 | 1000 |
