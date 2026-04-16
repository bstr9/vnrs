# BACKTESTING MODULE

Strategy simulation engine with realistic fill models and risk controls.

## OVERVIEW
Single-symbol and portfolio backtesting. Prevents look-ahead bias via event ordering: pending orders crossed BEFORE strategy callback.

## STRUCTURE
```
backtesting/
├── engine.rs        # Core BacktestingEngine (single-symbol)
├── portfolio.rs     # PortfolioBacktestingEngine (multi-symbol)
├── position.rs      # Position tracking with avg price, realized PnL, flip handling
├── fill_model.rs    # 5 fill models (BestPrice, Ideal, TwoTier, SizeAware, Probabilistic)
├── risk_engine.rs   # Pre-trade risk checks (size/position/daily limits)
├── statistics.rs    # Performance metrics (Sharpe, drawdown, returns)
├── data_merge.rs    # K-way merge iterators for multi-symbol data
├── database.rs      # PostgreSQL loader (feature-gated)
├── optimization.rs  # Grid search + genetic algorithm (Rayon parallel)
└── base.rs          # BacktestingMode, DailyResult, BacktestingResult
```

## WHERE TO LOOK
| Task | Location |
|------|----------|
| Modify fill logic | `fill_model.rs` - implement `FillModel` trait |
| Add risk checks | `risk_engine.rs` - RiskConfig + RiskEngine |
| Add performance metrics | `statistics.rs` + `base.rs` |
| Multi-symbol backtest | `portfolio.rs` + `data_merge.rs` |
| Parameter optimization | `optimization.rs` |

## KEY PATTERNS
- **Event ordering**: `update_dt → cross_pending_orders → update_indicators → strategy.on_bar()`
- **Fill models**: Pluggable via `Box<dyn FillModel>`
- **Position tracking**: Volume-weighted avg entry, proper flip handling
- **Optimization**: Rayon parallel execution, each worker has isolated engine

## CONVENTIONS
- Stop fills handle price gaps (use worse of trigger vs close)
- Daily results reset on new trading day
- Risk engine disabled by default (`new_unrestricted()`)
