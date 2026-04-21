//! Volatility Strategy Backtest Example
//!
//! Generates synthetic BTCUSDT minute-bar data with realistic volatility regimes
//! (trending + mean-reverting cycles), then runs a parameter sweep over
//! VolatilityStrategy configurations and logs the best result.
//!
//! Usage:
//!   cargo run --example volatility_backtest --features "gui,alpha,python"
//!
//! After running, the best result is appended to `backtest_log.md`.

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
/// in a realistic range, and cyclically varying volatility with trending phases.
fn generate_synthetic_bars(
    start_price: f64,
    num_bars: usize,
    start_dt: DateTime<Utc>,
) -> Vec<BarData> {
    let mut bars = Vec::with_capacity(num_bars);
    let mut price = start_price;
    let log_price_mean = start_price.ln(); // Mean-reversion target

    // Volatility regime parameters — calibrated to real BTC 1-minute behavior
    // Real BTC: 1m returns ~0.03%-0.08% normally, 0.1%-0.5% in high-vol periods
    let base_vol = 0.0008; // ~0.08% per minute (low vol)
    let high_vol = 0.005; // ~0.5% per minute (high vol — trending/momentum)
    let cycle_period = 720.0; // Volatility cycle: ~12 hours

    // Mean reversion speed — weaker to allow trending moves
    let reversion_speed = 0.00003;

    // Trending drift regime: adds momentum bursts for breakout signals to profit from
    let trend_period = 1440.0; // ~24 hour trend cycle
    let trend_strength = 0.0002; // Subtle directional drift during trend phases

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

        // Trending drift: alternating up/down momentum for breakout signals
        let trend_phase = 2.0 * std::f64::consts::PI * (i as f64) / trend_period;
        let trend_drift = trend_strength * trend_phase.sin();

        // Normal-ish random via Box-Muller (simplified: sum of uniforms)
        let u1 = next_uniform(&mut state);
        let u2 = next_uniform(&mut state);
        let u3 = next_uniform(&mut state);
        let u4 = next_uniform(&mut state);
        // Central limit theorem: sum of 4 uniforms approximates normal
        let normal = (u1 + u2 + u3 + u4 - 2.0) * 1.5; // ~N(0, ~0.6)

        // GBM step: mean-reversion + trend + random
        let ret = drift + trend_drift + current_vol * normal;

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
// Parameter sweep
// ============================================================================

/// Build a strategy setting HashMap from individual parameters.
#[allow(clippy::too_many_arguments)]
fn make_setting(
    atr_length: usize,
    boll_length: usize,
    boll_dev: f64,
    natr_threshold: f64,
    tp_atr_mult: f64,
    sl_atr_mult: f64,
    fixed_size: f64,
    am_length: usize,
) -> HashMap<String, serde_json::Value> {
    let mut s = HashMap::new();
    s.insert("atr_length".to_string(), serde_json::json!(atr_length));
    s.insert("boll_length".to_string(), serde_json::json!(boll_length));
    s.insert("boll_dev".to_string(), serde_json::json!(boll_dev));
    s.insert("natr_threshold".to_string(), serde_json::json!(natr_threshold));
    s.insert("tp_atr_mult".to_string(), serde_json::json!(tp_atr_mult));
    s.insert("sl_atr_mult".to_string(), serde_json::json!(sl_atr_mult));
    s.insert("fixed_size".to_string(), serde_json::json!(fixed_size));
    s.insert("am_length".to_string(), serde_json::json!(am_length));
    s
}

/// Run a single backtest and return (statistics, setting).
async fn run_single_backtest(
    bars: &[BarData],
    setting: HashMap<String, serde_json::Value>,
    vt_symbol: &str,
    start_dt: DateTime<Utc>,
    end_dt: DateTime<Utc>,
) -> (BacktestingStatistics, HashMap<String, serde_json::Value>) {
    let strategy = VolatilityStrategy::new(
        "VolBTC_Spot".to_string(),
        vt_symbol.to_string(),
        setting.clone(),
    );

    let mut engine = BacktestingEngine::new();
    engine.set_parameters(
        vt_symbol.to_string(),
        Interval::Minute,
        start_dt,
        end_dt,
        0.001,    // 0.1% commission (Binance spot maker)
        0.5,      // $0.5 slippage per BTC
        1.0,      // Contract size
        0.01,     // Price tick
        100000.0, // Initial capital ($100k)
        BacktestingMode::Bar,
    );
    engine.set_history_data(bars.to_vec());
    engine.add_strategy(Box::new(strategy));

    match engine.run_backtesting().await {
        Ok(()) => {
            let stats = engine.calculate_statistics(true);
            (stats, setting)
        }
        Err(e) => {
            eprintln!("Backtest failed: {}", e);
            // Return a terrible result so it's never picked
            let bad_stats = BacktestingStatistics::default();
            (bad_stats, setting)
        }
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("=== Volatility Strategy Backtest — Parameter Sweep ===\n");

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

    // Parameter sweep grid
    // Key insight from previous runs:
    //   - natr_threshold=0.1 → 807 trades, churn, Sharpe=-1.59
    //   - boll_dev=2.5, natr=0.5 → 76 trades, rare false breakouts, Sharpe=-5.04
    //   - boll_dev=1.5, natr=0.3 → 776 trades, still too many false signals
    // We need to find the sweet spot: selective enough to avoid churn,
    // permissive enough to catch real breakouts.
    let boll_devs: Vec<f64> = vec![1.5, 2.0];
    let natr_thresholds: Vec<f64> = vec![0.2, 0.4, 0.6, 0.8];
    let tp_atr_mults: Vec<f64> = vec![2.0, 3.0, 4.0];
    let sl_atr_mults: Vec<f64> = vec![1.0, 1.5, 2.0];
    let boll_lengths: Vec<usize> = vec![20, 30];

    let mut all_results: Vec<(BacktestingStatistics, HashMap<String, serde_json::Value>)> = Vec::new();
    let mut run_num = 0;
    let total_runs = boll_devs.len() * natr_thresholds.len() * tp_atr_mults.len()
        * sl_atr_mults.len() * boll_lengths.len();
    println!("\nRunning parameter sweep ({} combinations)...", total_runs);

    for &boll_length in &boll_lengths {
        for &boll_dev in &boll_devs {
            for &natr_threshold in &natr_thresholds {
                for &tp_atr_mult in &tp_atr_mults {
                    for &sl_atr_mult in &sl_atr_mults {
                        run_num += 1;
                        // Ensure TP > SL (otherwise strategy makes no sense)
                        if tp_atr_mult <= sl_atr_mult {
                            continue;
                        }

                        let setting = make_setting(
                            14,
                            boll_length,
                            boll_dev,
                            natr_threshold,
                            tp_atr_mult,
                            sl_atr_mult,
                            0.1,
                            100,
                        );

                        let (stats, s) = run_single_backtest(
                            &bars, setting, vt_symbol, start_dt, end_dt,
                        )
                        .await;

                        if run_num <= 5 || run_num % 20 == 0 || stats.sharpe_ratio > 0.0 {
                            println!(
                                "  [{}/{}] dev={:.1} natr={:.1} tp={:.1} sl={:.1} boll_len={} → Sharpe={:.4}, Trades={}, WR={:.1}%",
                                run_num,
                                total_runs,
                                boll_dev,
                                natr_threshold,
                                tp_atr_mult,
                                sl_atr_mult,
                                boll_length,
                                stats.sharpe_ratio,
                                stats.total_trade_count,
                                stats.win_rate * 100.0,
                            );
                        }

                        // Only keep results with at least some trades (need signal)
                        if stats.total_trade_count >= 5 {
                            all_results.push((stats, s));
                        }
                    }
                }
            }
        }
    }

    // Sort by Sharpe ratio descending
    all_results.sort_by(|a, b| {
        b.0.sharpe_ratio
            .partial_cmp(&a.0.sharpe_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Display top 10 results
    println!("\n=== Top 10 Results (by Sharpe Ratio) ===");
    for (i, (stats, setting)) in all_results.iter().take(10).enumerate() {
        println!(
            "#{}) Sharpe={:.4}, MaxDD={:.2}%, Trades={}, WinRate={:.1}%, PnL={:.2}",
            i + 1,
            stats.sharpe_ratio,
            stats.max_drawdown_percent,
            stats.total_trade_count,
            stats.win_rate * 100.0,
            stats.total_net_pnl,
        );
        println!(
            "     dev={:?}, natr={:?}, tp={:?}, sl={:?}, boll_len={:?}",
            setting.get("boll_dev").map(|v| v.to_string()),
            setting.get("natr_threshold").map(|v| v.to_string()),
            setting.get("tp_atr_mult").map(|v| v.to_string()),
            setting.get("sl_atr_mult").map(|v| v.to_string()),
            setting.get("boll_length").map(|v| v.to_string()),
        );
    }

    // Log the best result
    if let Some((best_stats, best_setting)) = all_results.first() {
        println!("\n=== Best Result ===");
        println!("Sharpe Ratio:     {:.4}", best_stats.sharpe_ratio);
        println!(
            "Max Drawdown:     {:.2} ({:.2}%)",
            best_stats.max_drawdown, best_stats.max_drawdown_percent
        );
        println!("Total Net PnL:    {:.2}", best_stats.total_net_pnl);
        println!("End Balance:      {:.2}", best_stats.end_balance);
        println!("Total Trades:     {}", best_stats.total_trade_count);
        println!(
            "Win Rate:         {:.2}%",
            best_stats.win_rate * 100.0
        );

        let params: HashMap<String, String> = best_setting
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();

        match append_backtest_log(best_stats, "VolBTC_Spot", vt_symbol, num_bars, &params) {
            Ok(()) => println!("\nBest result appended to backtest_log.md"),
            Err(e) => eprintln!("\nFailed to write backtest_log.md: {}", e),
        }
    } else {
        eprintln!("\nNo results with >= 5 trades found. Try different parameters.");
    }
}
