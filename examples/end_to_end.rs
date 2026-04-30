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
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
//! End-to-End Pipeline Example
//!
//! Demonstrates the complete workflow from backtesting through optimization
//! to paper trading deployment:
//!
//! 1. **Backtest** — Run strategy with default parameters on synthetic data
//! 2. **Optimize** — Grid search over parameter space, find best Sharpe
//! 3. **Validate** — Re-run backtest with optimal parameters
//! 4. **Deploy** — Generate `LiveDeploymentConfig` for paper/live trading
//!
//! Usage:
//!   cargo run --example end_to_end --features "gui,alpha,python"

use chrono::{DateTime, Duration, TimeZone, Utc};
use std::collections::HashMap;

use trade_engine::backtesting::{
    BacktestingEngine, BacktestingMode, BacktestingStatistics,
    OptimizationEngine, OptimizationSettings, OptimizationTarget,
    Parameter,
};
use trade_engine::strategy::volatility::VolatilityStrategy;
use trade_engine::strategy::StrategyTemplate;
use trade_engine::trader::{BarData, Exchange, Interval};

// ============================================================================
// Synthetic data generator
// ============================================================================

/// Generate synthetic BTCUSDT 1-minute bars with volatility regimes.
fn generate_synthetic_bars(
    start_price: f64,
    num_bars: usize,
    start_dt: DateTime<Utc>,
) -> Vec<BarData> {
    let mut bars = Vec::with_capacity(num_bars);
    let mut price = start_price;
    let mut dt = start_dt;

    for i in 0..num_bars {
        // Cycle volatility: high every 500 bars, low every 500 bars
        let cycle_pos = (i % 1000) as f64 / 1000.0;
        let vol = 0.0005 + 0.002 * (cycle_pos * std::f64::consts::PI * 2.0).sin().abs();

        // Random walk with slight mean reversion
        let noise: f64 = ((i * 1103515245 + 12345) % 10000) as f64 / 10000.0;
        let drift = (start_price - price) * 0.00001;
        let ret = drift + vol * (noise - 0.5) * 2.0;

        let open = price;
        let close = price * (1.0 + ret);
        let high = open.max(close) * (1.0 + vol * 0.5 * noise);
        let low = open.min(close) * (1.0 - vol * 0.5 * (1.0 - noise));
        let volume = 100.0 + 50.0 * noise;

        let bar = BarData {
            gateway_name: "BACKTEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover: close * volume,
            open_interest: 0.0,
            extra: None,
        };

        bars.push(bar);
        price = close;
        dt += Duration::minutes(1);
    }

    bars
}

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

/// Convert a ParameterSet (HashMap<String, f64>) to StrategySetting (HashMap<String, serde_json::Value>).
fn params_to_setting(params: &HashMap<String, f64>) -> HashMap<String, serde_json::Value> {
    let mut setting = HashMap::new();
    // Defaults for parameters not in the optimization space
    setting.insert("atr_length".to_string(), serde_json::json!(22));
    setting.insert("tp_atr_mult".to_string(), serde_json::json!(3.0));
    setting.insert("sl_atr_mult".to_string(), serde_json::json!(1.5));
    setting.insert("fixed_size".to_string(), serde_json::json!(1.0));
    setting.insert("am_length".to_string(), serde_json::json!(100));
    // Override with optimized parameters
    for (k, v) in params {
        setting.insert(k.clone(), serde_json::json!(v));
    }
    setting
}

/// Print backtest statistics summary.
fn print_stats(label: &str, stats: &BacktestingStatistics) {
    println!("  [{label}]");
    println!("    Sharpe Ratio:  {:.4}", stats.sharpe_ratio);
    println!("    Total Return:  {:.2}%", stats.return_mean * 100.0);
    println!("    Max Drawdown:  {:.2}%", stats.max_drawdown_percent);
    println!("    Win Rate:      {:.1}%", stats.win_rate * 100.0);
    println!("    Total Trades:  {}", stats.total_trade_count);
}

// ============================================================================
// Pipeline stages
// ============================================================================

/// Stage 1: Run a backtest with given parameters and return statistics.
async fn run_backtest(
    bars: &[BarData],
    setting: &HashMap<String, serde_json::Value>,
    vt_symbol: &str,
    start_dt: DateTime<Utc>,
    end_dt: DateTime<Utc>,
) -> Result<BacktestingStatistics, String> {
    let strategy = VolatilityStrategy::new(
        "VolBTC_E2E".to_string(),
        vt_symbol.to_string(),
        setting.clone(),
    );

    let mut engine = BacktestingEngine::new();
    engine.set_parameters(
        vt_symbol.to_string(),
        Interval::Minute,
        start_dt,
        end_dt,
        0.001,     // 0.1% commission
        0.5,       // $0.5 slippage
        1.0,       // Contract size
        0.01,      // Price tick
        100000.0,  // Initial capital
        BacktestingMode::Bar,
    );
    engine.set_history_data(bars.to_vec());
    engine.add_strategy(Box::new(strategy));

    engine.run_backtesting().await.map_err(|e| e.to_string())?;
    Ok(engine.calculate_statistics(true))
}

// ============================================================================
// Main pipeline
// ============================================================================

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║       End-to-End Pipeline: Backtest → Optimize → Deploy    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Configuration
    let vt_symbol = "BTCUSDT.BINANCE";
    let start_price = 50000.0;
    let num_bars = 10_000; // ~7 days of 1-minute data
    let start_dt = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let end_dt = start_dt + Duration::minutes(num_bars as i64);

    // Generate synthetic data
    println!("Generating {} synthetic 1-minute bars...", num_bars);
    let bars = generate_synthetic_bars(start_price, num_bars, start_dt);
    let bars: Vec<BarData> = bars
        .into_iter()
        .filter(|b| b.close_price > 0.0 && b.high_price > 0.0 && b.low_price > 0.0)
        .collect();
    println!("  {} valid bars", bars.len());

    // ── Stage 1: Baseline Backtest ──────────────────────────────────────
    println!("\n--- Stage 1: Baseline Backtest (default parameters) ---");

    let default_setting = make_setting(22, 20, 2.0, 0.5, 3.0, 1.5, 1.0, 100);
    match run_backtest(&bars, &default_setting, vt_symbol, start_dt, end_dt).await {
        Ok(stats) => print_stats("Baseline", &stats),
        Err(e) => println!("  Baseline backtest failed: {e}"),
    }

    // ── Stage 2: Parameter Optimization (Grid Search) ───────────────────
    println!("\n--- Stage 2: Parameter Optimization (Grid Search) ---");

    let opt_settings = OptimizationSettings {
        vt_symbol: vt_symbol.to_string(),
        interval: Interval::Minute,
        start: start_dt,
        end: end_dt,
        rate: 0.001,
        slippage: 0.5,
        size: 1.0,
        pricetick: 0.01,
        capital: 100000.0,
        mode: BacktestingMode::Bar,
    };

    let mut opt = OptimizationEngine::new(opt_settings);
    opt.set_history_data(bars.clone());

    // Define parameter space for grid search
    opt.add_parameter(Parameter::new("boll_length", 15.0, 30.0, 5.0));
    opt.add_parameter(Parameter::new("boll_dev", 1.5, 2.5, 0.5));
    opt.add_parameter(Parameter::new("natr_threshold", 0.3, 0.7, 0.2));

    // Strategy factory: ParameterSet (f64) → StrategyTemplate
    let vt_symbol_for_factory = vt_symbol.to_string();
    let results = opt.run_grid_search(
        move |params: &HashMap<String, f64>| {
            let setting = params_to_setting(params);
            Box::new(VolatilityStrategy::new(
                "VolBTC_E2E".to_string(),
                vt_symbol_for_factory.clone(),
                setting,
            )) as Box<dyn StrategyTemplate>
        },
        OptimizationTarget::SharpeRatio,
    );

    println!("  Grid search: {} parameter combinations tested", results.len());

    let optimal_params = if let Some(best) = results.first() {
        println!("  Best Sharpe ratio: {:.4}", best.target_value);
        println!("  Best parameters:");
        for (k, v) in &best.parameters {
            println!("    {k} = {v:.2}");
        }
        best.parameters.clone()
    } else {
        println!("  No optimization results, using defaults");
        let mut defaults = HashMap::new();
        defaults.insert("boll_length".to_string(), 20.0);
        defaults.insert("boll_dev".to_string(), 2.0);
        defaults.insert("natr_threshold".to_string(), 0.5);
        defaults
    };

    // ── Stage 3: Validated Backtest with Optimal Parameters ─────────────
    println!("\n--- Stage 3: Validated Backtest (optimal parameters) ---");

    let optimal_setting = params_to_setting(&optimal_params);
    match run_backtest(&bars, &optimal_setting, vt_symbol, start_dt, end_dt).await {
        Ok(stats) => print_stats("Optimized", &stats),
        Err(e) => println!("  Validated backtest failed: {e}"),
    }

    // ── Stage 4: Generate Deployment Config ─────────────────────────────
    println!("\n--- Stage 4: Generate Live Deployment Config ---");

    // Run one more backtest with the optimal strategy to get to_live_config()
    let strategy = VolatilityStrategy::new(
        "VolBTC_E2E".to_string(),
        vt_symbol.to_string(),
        optimal_setting,
    );
    let mut engine = BacktestingEngine::new();
    engine.set_parameters(
        vt_symbol.to_string(),
        Interval::Minute,
        start_dt,
        end_dt,
        0.001,
        0.5,
        1.0,
        0.01,
        100000.0,
        BacktestingMode::Bar,
    );
    engine.set_history_data(bars.clone());
    engine.add_strategy(Box::new(strategy));

    if engine.run_backtesting().await.is_ok() {
        let config = engine.to_live_config().with_parameters(optimal_params.clone());

        println!("  Strategy Name:  {}", config.strategy_name);
        println!("  Symbol:         {}", config.vt_symbol);
        println!("  Exchange:       {:?}", config.exchange);
        println!("  Interval:       {:?}", config.interval);
        println!("  Capital:        ${:.0}", config.capital);
        println!("  Commission:     {:.2}%", config.rate * 100.0);
        println!("  Slippage:       ${:.2}", config.slippage);
        println!("  Contract Size:  {}", config.size);
        println!("  Price Tick:     ${:.2}", config.pricetick);
        println!("  Optimal Parameters:");
        for (k, v) in &config.optimal_parameters {
            println!("    {k} = {v:.2}");
        }

        // ── Stage 5: Paper Trading Setup (demonstration) ───────────────
        println!("\n--- Stage 5: Paper Trading Setup ---");
        println!("  To deploy this strategy to paper trading:");
        println!("    1. Create MainEngine + EventEngine");
        println!("    2. Create StrategyEngine from MainEngine");
        println!("    3. Call strategy_engine.switch_to_paper()");
        println!("    4. Add the VolatilityStrategy with optimal parameters");
        println!("    5. Call strategy_engine.init()");
        println!("    6. Call strategy_engine.start()");
        println!("    7. Connect gateway and subscribe to {} @ {:?}", config.vt_symbol, config.interval);
        println!();
        println!("  Example code:");
        println!("    let main_engine = Arc::new(MainEngine::new());");
        println!("    let event_engine = Arc::new(EventEngine::new());");
        println!("    let mut strat_engine = StrategyEngine::new(main_engine, event_engine);");
        println!("    strat_engine.switch_to_paper();");
        println!("    let strategy = VolatilityStrategy::new(name, vt_symbol, optimal_setting);");
        println!("    strat_engine.add_strategy(Box::new(strategy), setting);");
        println!("    strat_engine.init();");
        println!("    strat_engine.start();");
        println!();
        println!("  Orders will be locally matched against live market data");
        println!("  via PaperTradingEngine before being sent to the exchange.");
    } else {
        println!("  Failed to generate deployment config (backtest error)");
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║               Pipeline Complete ✓                            ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}
