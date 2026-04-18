//! Volatility Strategy Backtest Example
//!
//! Generates synthetic BTCUSDT minute-bar data with realistic volatility regimes
//! (trending + mean-reverting cycles), then runs the VolatilityStrategy backtest.
//!
//! Usage:
//!   cargo run --example volatility_backtest --features "gui,alpha,python"
//!
//! After running, results are appended to `backtest_log.md`.

use chrono::{DateTime, Duration, TimeZone, Utc};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;

use trade_engine::backtesting::{
    BacktestingEngine, BacktestingMode, BacktestingStatistics,
};
use trade_engine::strategy::volatility::VolatilityStrategy;
use trade_engine::trader::{BarData, Exchange, Interval};

// ============================================================================
// Synthetic data generator
// ============================================================================

/// Generate synthetic BTCUSDT 1-minute bars with volatility regimes.
///
/// Uses geometric Brownian motion with mean-reverting drift to keep prices
/// in a realistic range, and cyclically varying volatility.
fn generate_synthetic_bars(
    start_price: f64,
    num_bars: usize,
    start_dt: DateTime<Utc>,
) -> Vec<BarData> {
    let mut bars = Vec::with_capacity(num_bars);
    let mut price = start_price;
    let log_price_mean = start_price.ln(); // Mean-reversion target

    // Volatility regime parameters
    let base_vol = 0.0005; // ~0.05% per minute (low vol) — realistic for 1m BTC
    let high_vol = 0.003; // ~0.3% per minute (high vol)
    let cycle_period = 480.0; // Volatility cycle: ~8 hours

    // Mean reversion speed (keeps price around start_price)
    let reversion_speed = 0.0001;

    // Simple deterministic PRNG — xorshift64
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    let next_uniform = |s: &mut u64| -> f64 {
        // xorshift64
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        // Map to [0, 1) using high bits
        (*s >> 12) as f64 / (1u64 << 52) as f64
    };

    for i in 0..num_bars {
        let dt = start_dt + Duration::minutes(i as i64);

        // Volatility regime: sine wave creates alternating low/high vol periods
        let phase = 2.0 * std::f64::consts::PI * (i as f64) / cycle_period;
        let vol_factor = 0.5 * (1.0 + phase.sin()); // 0..1
        let current_vol = base_vol + (high_vol - base_vol) * vol_factor;

        // Mean-reverting drift: pull log-price back toward log_price_mean
        let log_price = price.ln();
        let drift = reversion_speed * (log_price_mean - log_price);

        // Normal-ish random via Box-Muller (simplified: sum of uniforms)
        let u1 = next_uniform(&mut state);
        let u2 = next_uniform(&mut state);
        let u3 = next_uniform(&mut state);
        let u4 = next_uniform(&mut state);
        // Central limit theorem: sum of 4 uniforms approximates normal
        let normal = (u1 + u2 + u3 + u4 - 2.0) * 1.5; // ~N(0, ~0.6)

        // GBM step
        let ret = drift + current_vol * normal;

        // Generate OHLCV
        let open = price;
        let close = price * (ret.exp());
        let intra_high = open.max(close);
        let intra_low = open.min(close);
        let range_frac = 0.3; // 30% of range for wicks
        let high = intra_high + (intra_high - intra_low) * range_frac * next_uniform(&mut state);
        let low = (intra_low - (intra_high - intra_low) * range_frac * next_uniform(&mut state))
            .max(intra_low * 0.999); // Floor at 99.9% of low

        let volume = 50.0 + 200.0 * vol_factor * next_uniform(&mut state);
        let turnover = (open + close) / 2.0 * volume;

        bars.push(BarData {
            gateway_name: "SYNTHETIC".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover,
            open_interest: 0.0,
            extra: None,
        });

        price = close;
    }

    bars
}

// ============================================================================
// Backtest log writer
// ============================================================================

fn append_backtest_log(
    stats: &BacktestingStatistics,
    strategy_name: &str,
    vt_symbol: &str,
    num_bars: usize,
    params: &HashMap<String, String>,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("backtest_log.md")?;

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

    writeln!(file, "## Backtest Result — {}", now)?;
    writeln!(file)?;
    writeln!(file, "| Item | Value |")?;
    writeln!(file, "|------|-------|")?;
    writeln!(file, "| Strategy | {} |", strategy_name)?;
    writeln!(file, "| Symbol | {} |", vt_symbol)?;
    writeln!(file, "| Bars | {} |", num_bars)?;
    writeln!(file, "| Period | {} to {} |", stats.start_date, stats.end_date)?;
    writeln!(file, "| Total Days | {} |", stats.total_days)?;
    writeln!(file, "| **Sharpe Ratio** | **{:.4}** |", stats.sharpe_ratio)?;
    writeln!(
        file,
        "| **Max Drawdown** | **{:.2} ({:.2}%)** |",
        stats.max_drawdown, stats.max_drawdown_percent
    )?;
    writeln!(file, "| End Balance | {:.2} |", stats.end_balance)?;
    writeln!(file, "| Total Net PnL | {:.2} |", stats.total_net_pnl)?;
    writeln!(file, "| Total Return | {:.2}% |", stats.return_mean * 100.0)?;
    writeln!(file, "| Win Rate | {:.2}% |", stats.win_rate * 100.0)?;
    writeln!(file, "| Profit Factor | {:.4} |", stats.profit_factor)?;
    writeln!(file, "| Total Trades | {} |", stats.total_trade_count)?;
    writeln!(file, "| Sortino Ratio | {:.4} |", stats.sortino_ratio)?;
    writeln!(file, "| Calmar Ratio | {:.4} |", stats.calmar_ratio)?;
    writeln!(file, "| Max Consecutive Wins | {} |", stats.max_consecutive_wins)?;
    writeln!(file, "| Max Consecutive Losses | {} |", stats.max_consecutive_losses)?;
    writeln!(file, "| Total Commission | {:.2} |", stats.total_commission)?;
    writeln!(file, "| Total Slippage | {:.2} |", stats.total_slippage)?;

    // Parameters
    writeln!(file)?;
    writeln!(file, "**Parameters:**")?;
    for (k, v) in params {
        writeln!(file, "- `{}` = {}", k, v)?;
    }
    writeln!(file)?;
    writeln!(file, "---")?;
    writeln!(file)?;

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("=== Volatility Strategy Backtest ===\n");

    // Configuration
    let vt_symbol = "BTCUSDT.BINANCE";
    let start_price = 50000.0;
    let num_bars = 30_000; // ~20 trading days of 1-minute data

    let start_dt = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let end_dt = start_dt + Duration::minutes(num_bars as i64);

    // Generate synthetic data
    println!("Generating {} synthetic 1-minute bars...", num_bars);
    let bars = generate_synthetic_bars(start_price, num_bars, start_dt);

    // Filter out any bars with zero/negative prices
    let bars: Vec<BarData> = bars
        .into_iter()
        .filter(|b| b.close_price > 0.0 && b.high_price > 0.0 && b.low_price > 0.0)
        .collect();
    println!("  {} valid bars after filtering", bars.len());
    println!(
        "  Price range: {:.2} - {:.2}",
        bars.iter().map(|b| b.low_price).fold(f64::MAX, f64::min),
        bars.iter().map(|b| b.high_price).fold(f64::MIN, f64::max),
    );

    // Strategy parameters
    let mut setting = HashMap::new();
    setting.insert("atr_length".to_string(), serde_json::json!(14));
    setting.insert("boll_length".to_string(), serde_json::json!(20));
    setting.insert("boll_dev".to_string(), serde_json::json!(2.0));
    setting.insert("natr_threshold".to_string(), serde_json::json!(0.1)); // Lower threshold for 1m data
    setting.insert("tp_atr_mult".to_string(), serde_json::json!(3.0));
    setting.insert("sl_atr_mult".to_string(), serde_json::json!(2.0));
    setting.insert("fixed_size".to_string(), serde_json::json!(0.1)); // Larger size for visibility
    setting.insert("am_length".to_string(), serde_json::json!(100));

    let strategy = VolatilityStrategy::new(
        "VolBTC_Spot".to_string(),
        vt_symbol.to_string(),
        setting.clone(),
    );

    // Configure backtesting engine
    let mut engine = BacktestingEngine::new();
    engine.set_parameters(
        vt_symbol.to_string(),
        Interval::Minute,
        start_dt,
        end_dt,
        0.001,   // 0.1% commission (Binance spot maker)
        0.5,     // $0.5 slippage per BTC
        1.0,     // Contract size
        0.01,    // Price tick
        100000.0, // Initial capital ($100k)
        BacktestingMode::Bar,
    );
    engine.set_history_data(bars);
    engine.add_strategy(Box::new(strategy));

    // Run backtest
    println!("\nRunning backtest...");
    match engine.run_backtesting().await {
        Ok(()) => println!("Backtest completed successfully."),
        Err(e) => {
            eprintln!("Backtest failed: {}", e);
            std::process::exit(1);
        }
    }

    // Calculate and display statistics
    let stats = engine.calculate_statistics(true);

    println!("\n=== Key Results ===");
    println!("Sharpe Ratio:     {:.4}", stats.sharpe_ratio);
    println!(
        "Max Drawdown:     {:.2} ({:.2}%)",
        stats.max_drawdown, stats.max_drawdown_percent
    );
    println!("Total Net PnL:    {:.2}", stats.total_net_pnl);
    println!("End Balance:      {:.2}", stats.end_balance);
    println!("Total Trades:     {}", stats.total_trade_count);
    println!("Win Rate:         {:.2}%", stats.win_rate * 100.0);

    // Append to backtest_log.md
    let params: HashMap<String, String> = setting
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect();

    match append_backtest_log(&stats, "VolBTC_Spot", vt_symbol, num_bars, &params) {
        Ok(()) => println!("\nResults appended to backtest_log.md"),
        Err(e) => eprintln!("\nFailed to write backtest_log.md: {}", e),
    }
}
