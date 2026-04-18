//! End-to-end backtest example: Download BTCUSDT data from Binance and run VolatilityStrategy.
//!
//! Run: cargo run --example binance_backtest
//!
//! Prerequisites:
//!   - .rstrader/binance/gateway_configs.json must exist with BINANCE_SPOT config
//!   - Network access to Binance API (directly or via proxy)

use std::collections::HashMap;

use chrono::Utc;
use trade_engine::backtesting::BacktestingEngine;
use trade_engine::backtesting::BacktestingMode;
use trade_engine::trader::Interval;
use trade_engine::strategy::VolatilityStrategy;
use trade_engine::strategy::base::StrategySetting;

#[tokio::main]
async fn main() {
    println!("=== Binance Backtest: VolatilityStrategy on BTCUSDT ===\n");

    // 1. Configure backtesting engine
    let mut engine = BacktestingEngine::new();

    // Use last 3 days of data
    let end = Utc::now();
    let start = end - chrono::Duration::days(3);

    engine.set_parameters(
        "BTCUSDT.BINANCE".to_string(),
        Interval::Minute,       // 1-minute bars
        start,
        end,
        0.001,                  // 0.1% commission rate (Binance spot)
        1.0,                    // $1 slippage
        1.0,                    // contract size
        0.01,                   // price tick
        100_000.0,              // initial capital
        BacktestingMode::Bar,
    );

    // 2. Add VolatilityStrategy with custom parameters
    let mut setting: StrategySetting = HashMap::new();
    setting.insert("atr_length".to_string(), serde_json::json!(14));
    setting.insert("boll_length".to_string(), serde_json::json!(30));
    setting.insert("boll_dev".to_string(), serde_json::json!(2.0));
    setting.insert("natr_threshold".to_string(), serde_json::json!(0.3));
    setting.insert("tp_atr_mult".to_string(), serde_json::json!(3.0));
    setting.insert("sl_atr_mult".to_string(), serde_json::json!(1.5));
    setting.insert("fixed_size".to_string(), serde_json::json!(0.01));
    setting.insert("am_length".to_string(), serde_json::json!(100));

    let strategy = VolatilityStrategy::new(
        "VolBTC".to_string(),
        "BTCUSDT.BINANCE".to_string(),
        setting,
    );
    engine.add_strategy(Box::new(strategy));

    // 3. Download data from Binance
    println!("Downloading BTCUSDT 1m data from Binance...");
    println!("  Period: {} to {}", start.format("%Y-%m-%d %H:%M"), end.format("%Y-%m-%d %H:%M"));
    match engine.load_data_from_binance().await {
        Ok(()) => println!("  Data loaded successfully"),
        Err(e) => {
            eprintln!("  Failed to load data: {}", e);
            eprintln!("  Make sure .rstrader/binance/gateway_configs.json exists with BINANCE_SPOT config");
            std::process::exit(1);
        }
    }

    // 4. Run backtesting
    println!("\nRunning backtest...");
    match engine.run_backtesting().await {
        Ok(()) => println!("  Backtest completed"),
        Err(e) => {
            eprintln!("  Backtest failed: {}", e);
            std::process::exit(1);
        }
    }

    // 5. Calculate and display statistics
    println!("\n=== Backtest Results ===");
    let result = engine.calculate_result();
    let stats = engine.calculate_statistics(true);

    println!("\n--- Key Metrics ---");
    println!("  Total Return:     {:.2}%", result.total_return * 100.0);
    println!("  Sharpe Ratio:     {:.4}", stats.sharpe_ratio);
    println!("  Max Drawdown:     {:.2} ({:.2}%)", stats.max_drawdown, stats.max_drawdown_percent);
    println!("  Win Rate:         {:.2}%", stats.win_rate * 100.0);
    println!("  Total Trades:     {}", stats.total_trade_count);
    println!("  Net PnL:          {:.2}", stats.total_net_pnl);
    println!("  Commission:       {:.2}", stats.total_commission);
    println!("  Sortino Ratio:    {:.4}", stats.sortino_ratio);
    println!("  Calmar Ratio:     {:.4}", stats.calmar_ratio);
    println!("  Annual Return:    {:.2}%", stats.return_mean * 100.0);

    println!("\n--- Period ---");
    println!("  Start Date:       {}", stats.start_date);
    println!("  End Date:         {}", stats.end_date);
    println!("  Trading Days:     {}", stats.total_days);
    println!("  Profit Days:      {}", stats.profit_days);
    println!("  Loss Days:        {}", stats.loss_days);

    println!("\n=== Backtest Complete ===");
}
