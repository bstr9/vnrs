// Allow pedantic lints (personal project, pragmatic approach)
#![allow(
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::redundant_closure,
    clippy::filter_map_identity,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::unnecessary_wraps,
    clippy::unnecessary_debug_formatting,
    clippy::uninlined_format_args,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::wildcard_imports,
    clippy::if_not_else,
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::used_underscore_binding,
    clippy::unreadable_literal,
    clippy::non_std_lazy_statics,
    clippy::default_trait_access,
    clippy::semicolon_if_nothing_returned,
    clippy::redundant_closure_for_method_calls,
    clippy::float_cmp,
    clippy::implicit_hasher,
    clippy::unused_async,
    clippy::unnested_or_patterns,
    clippy::manual_let_else,
    clippy::single_char_pattern,
    clippy::assigning_clones,
    clippy::cloned_instead_of_copied,
    clippy::explicit_iter_loop,
    clippy::implicit_clone,
    clippy::trivially_copy_pass_by_ref,
    clippy::struct_excessive_bools,
    clippy::ignored_unit_patterns,
    clippy::match_wildcard_for_single_variants,
    clippy::unnecessary_literal_bound,
    clippy::manual_string_new,
    clippy::inefficient_to_string,
    clippy::no_effect_underscore_binding,
    clippy::doc_link_with_quotes,
    clippy::doc_lazy_continuation,
    clippy::format_push_string,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::enum_glob_use,
    clippy::unnecessary_map_or,
    clippy::comparison_chain,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::missing_fields_in_debug,
    clippy::field_reassign_with_default,
    clippy::derivable_impls,
    clippy::manual_midpoint,
    // Cast lints (trading code uses f64/i64 casts extensively)
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
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

    // Use last 7 days of data (need enough bars for ArrayManager init + signals)
    let end = Utc::now();
    let start = end - chrono::Duration::days(7);

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
    //    Lower NATR threshold (0.15) to generate more signals in typical BTCUSDT volatility
    //    am_length=50 allows faster strategy initialization (needs 50 bars instead of 100)
    let mut setting: StrategySetting = HashMap::new();
    setting.insert("atr_length".to_string(), serde_json::json!(14));
    setting.insert("boll_length".to_string(), serde_json::json!(20));
    setting.insert("boll_dev".to_string(), serde_json::json!(2.0));
    setting.insert("natr_threshold".to_string(), serde_json::json!(0.15));
    setting.insert("tp_atr_mult".to_string(), serde_json::json!(3.0));
    setting.insert("sl_atr_mult".to_string(), serde_json::json!(1.5));
    setting.insert("fixed_size".to_string(), serde_json::json!(0.01));
    setting.insert("am_length".to_string(), serde_json::json!(50));

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
