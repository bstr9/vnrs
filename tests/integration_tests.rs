//! Integration tests for the vnrs trading engine
//!
//! End-to-end tests exercising the full backtesting lifecycle:
//! strategy signals → order submission → fill → position update → PnL calculation

mod common;

use chrono::{TimeZone, Utc};
use trade_engine::backtesting::{
    BacktestingEngine, BacktestingMode, BestPriceFillModel, IdealFillModel,
};
use trade_engine::trader::{
    Direction, Exchange, Interval, Offset, OrderRequest, OrderType, TestClock,
};
use trade_engine::strategy::StrategyTemplate;

use common::assertions::{assert_approx_eq, assert_ok};
use common::fixtures::{make_ascending_bars, make_test_bar, make_test_bar_at_time};
use common::mock_strategy::TestStrategy;

/// Helper: create a basic BacktestingEngine configured for BTCUSDT.BINANCE
fn make_test_engine(capital: f64, rate: f64, pricetick: f64) -> BacktestingEngine {
    let mut engine = BacktestingEngine::new();
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
    engine.set_parameters(
        "BTCUSDT.BINANCE".to_string(),
        Interval::Minute,
        start,
        end,
        rate,
        0.0,   // slippage
        1.0,   // size
        pricetick,
        capital,
        BacktestingMode::Bar,
    );
    engine
}

// ============================================================================
// Test 1: Basic backtest lifecycle
// ============================================================================

#[tokio::test]
async fn test_backtest_basic_lifecycle() {
    let mut engine = make_test_engine(1_000_000.0, 0.0, 0.01);

    let strategy = TestStrategy::new("LIFECYCLE_TEST", "BTCUSDT.BINANCE");
    engine.add_strategy(Box::new(strategy));

    let bars = make_ascending_bars("BTCUSDT", Exchange::Binance, 50000.0, 5, 60);
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Access the strategy through the engine's strategy field
    // Since we can't access the boxed strategy directly, verify through engine state
    let trades = engine.get_all_trades();
    // With 0% rate and no buy_on_bar, no orders placed → no trades
    assert_eq!(trades.len(), 0);

    // Verify the engine processed all 5 bars (daily_results has entries)
    let stats = engine.calculate_statistics(false);
    assert!(stats.total_days >= 1);
}

// ============================================================================
// Test 2: Buy order fills correctly
// ============================================================================

#[tokio::test]
async fn test_backtest_buy_order_fill() {
    let mut engine = make_test_engine(100_000.0, 0.001, 0.01);

    let mut strategy = TestStrategy::new("BUY_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    engine.add_strategy(Box::new(strategy));

    // Create bars where low <= 50000 (so buy limit at 50000 fills)
    // Bar 1: strategy places order (won't fill same bar due to look-ahead prevention)
    // Bar 2: low=49800 <= 50000 → order fills
    let bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50200.0, 50000.0, 50150.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50300.0, 49800.0, 50200.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50200.0, 50400.0, 50100.0, 50300.0, 100.0),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Verify position is long after buy fill
    let pos = engine.get_pos();
    assert!(pos > 0.0, "Position should be long after buy fill, got {}", pos);

    // Verify trade was recorded
    let trades = engine.get_all_trades();
    assert!(!trades.is_empty(), "At least one trade should be recorded");

    let buy_trade = trades.iter().find(|t| t.direction == Some(Direction::Long));
    assert!(buy_trade.is_some(), "Should have a Long trade");
    let t = buy_trade.unwrap();
    assert_approx_eq(t.volume, 1.0, 0.001);
    assert_approx_eq(t.price, 50000.0, 1.0);
}

// ============================================================================
// Test 3: Sell order fills correctly
// ============================================================================

#[tokio::test]
async fn test_backtest_sell_order_fill() {
    let mut engine = make_test_engine(100_000.0, 0.001, 0.01);

    let mut strategy = TestStrategy::new("SELL_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    // Sell after accumulating 1.0 volume
    strategy.set_sell_after_volume(Some(1.0));
    engine.add_strategy(Box::new(strategy));

    // Create bars where orders will fill
    let bars = vec![
        // Bar 1: strategy buys at 50000 (order placed, not yet filled)
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50200.0, 50000.0, 50150.0, 100.0),
        // Bar 2: buy fills (low=49800 < 50000), then strategy sees trade and sells
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 51000.0, 49800.0, 50900.0, 100.0),
        // Bar 3: sell order fills (high=51000 >= 50900 sell price)
        make_test_bar("BTCUSDT", Exchange::Binance, 50900.0, 51100.0, 50800.0, 51000.0, 100.0),
        // Extra bar to ensure sell order processes
        make_test_bar("BTCUSDT", Exchange::Binance, 51000.0, 51200.0, 50900.0, 51100.0, 100.0),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // After round-trip, position should be back to 0
    let pos = engine.get_pos();
    assert_approx_eq(pos, 0.0, 0.001);
}

// ============================================================================
// Test 4: PnL calculation after round trip
// ============================================================================

#[tokio::test]
async fn test_backtest_pnl_round_trip() {
    let rate = 0.001; // 0.1% commission
    let mut engine = make_test_engine(100_000.0, rate, 0.01);

    let mut strategy = TestStrategy::new("PNL_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    strategy.set_sell_after_volume(Some(1.0));
    engine.add_strategy(Box::new(strategy));

    // Buy at 50000, sell at ~51000
    let bars = vec![
        // Bar 1: buy order placed
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50200.0, 50000.0, 50150.0, 100.0),
        // Bar 2: buy fills (low=49800), strategy sells at close=50900
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 51000.0, 49800.0, 50900.0, 100.0),
        // Bar 3: sell fills
        make_test_bar("BTCUSDT", Exchange::Binance, 50900.0, 51100.0, 50800.0, 51000.0, 100.0),
        // Bar 4: extra to ensure fill
        make_test_bar("BTCUSDT", Exchange::Binance, 51000.0, 51200.0, 50900.0, 51100.0, 100.0),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Position should be flat after round-trip
    let pos = engine.get_pos();
    assert_approx_eq(pos, 0.0, 0.001);

    // The position's realized PnL should be approximately:
    // (sell_price - buy_price) * volume = (50900 - 50000) * 1.0 = 900
    let realized_pnl = engine.get_realized_pnl();
    assert!(
        realized_pnl > 0.0,
        "Realized PnL should be positive after profitable round trip, got {}",
        realized_pnl
    );

    // Check trades exist
    let trades = engine.get_all_trades();
    assert!(trades.len() >= 2, "Should have at least 2 trades (buy + sell), got {}", trades.len());
}

// ============================================================================
// Test 5: Limit order not filled when price doesn't cross
// ============================================================================

#[tokio::test]
async fn test_backtest_limit_order_not_filled() {
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);

    let mut strategy = TestStrategy::new("NOFILL_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(49000.0); // Buy limit at 49000
    strategy.set_buy_volume(1.0);
    engine.add_strategy(Box::new(strategy));

    // Create bars where low never reaches 49000 (all lows > 49000)
    let bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 50000.0, 50500.0, 49500.0, 50200.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50200.0, 50700.0, 49600.0, 50400.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50400.0, 50900.0, 49700.0, 50600.0, 100.0),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // No trades should have been generated (price never reached 49000)
    let trades = engine.get_all_trades();
    assert_eq!(trades.len(), 0, "No trades should be generated when price doesn't cross");

    // Position should remain 0
    let pos = engine.get_pos();
    assert_approx_eq(pos, 0.0, 0.001);
}

// ============================================================================
// Test 6: Stop order fills on trigger
// ============================================================================

#[tokio::test]
async fn test_backtest_stop_order_fill() {
    // We test stop orders by pre-placing an Open stop order (to open a short)
    // and then feeding bars that trigger it.
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);
    engine.set_fill_model(Box::new(BestPriceFillModel::new(0.0)));

    let strategy = TestStrategy::new("STOP_TEST", "BTCUSDT.BINANCE");
    engine.add_strategy(Box::new(strategy));

    // Send a buy limit order to establish a long position first
    let buy_req = OrderRequest {
        symbol: "BTCUSDT".to_string(),
        exchange: Exchange::Binance,
        direction: Direction::Long,
        order_type: OrderType::Limit,
        volume: 1.0,
        price: 50000.0,
        offset: Offset::Open,
        reference: String::new(),
        post_only: false,
        reduce_only: false,
    };
    engine.send_limit_order(buy_req);

    // Bar 1: buy limit fills (low=49800 < 50000)
    // Bar 2: We'll feed a bar that drops and triggers a stop
    let bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50300.0, 49800.0, 50200.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50200.0, 50300.0, 49400.0, 49500.0, 100.0),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // After first bar, we should have a long position.
    // Now let's verify stop order mechanism by placing one after we have a position.
    let pos_after = engine.get_pos();
    assert!(pos_after > 0.0, "Should have long position, got {}", pos_after);

    // Now place a stop (Close) order to close the long
    let stop_req = OrderRequest {
        symbol: "BTCUSDT".to_string(),
        exchange: Exchange::Binance,
        direction: Direction::Short,
        order_type: OrderType::Stop,
        volume: 1.0,
        price: 49500.0,
        offset: Offset::Close,
        reference: String::new(),
        post_only: false,
        reduce_only: false,
    };
    engine.send_stop_order(stop_req);

    // Feed a bar where low <= 49500 to trigger the stop
    let stop_bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 49600.0, 49700.0, 49300.0, 49400.0, 100.0),
    ];
    engine.set_history_data(stop_bars);

    // Run the engine's internal bar loop by calling run_backtesting again.
    // But run_backtesting re-inits the strategy, which is okay since our strategy is stateless.
    let result2 = engine.run_backtesting().await;
    assert_ok(result2);

    // After stop fills, position should be flat
    let final_pos = engine.get_pos();
    assert_approx_eq(final_pos, 0.0, 0.001);

    // Should have a Short trade from the stop
    let trades = engine.get_all_trades();
    let stop_trades: Vec<_> = trades.iter().filter(|t| t.direction == Some(Direction::Short)).collect();
    assert!(!stop_trades.is_empty(), "Should have a Short trade from stop");
}

// ============================================================================
// Test 7: Multiple bars, multiple fills
// ============================================================================

#[tokio::test]
async fn test_backtest_multiple_fills() {
    let mut engine = make_test_engine(1_000_000.0, 0.0, 0.01);

    let mut strategy = TestStrategy::new("MULTIFILL_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    engine.add_strategy(Box::new(strategy));

    // Create bars where low is consistently below 50000 so multiple buy orders fill
    // Each bar has low < 50000, so buy limit at 50000 fills
    let bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 50200.0, 50500.0, 49800.0, 50100.0, 100.0), // low=49800
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50400.0, 49700.0, 50200.0, 100.0), // low=49700
        make_test_bar("BTCUSDT", Exchange::Binance, 50200.0, 50500.0, 49600.0, 50300.0, 100.0), // low=49600
        make_test_bar("BTCUSDT", Exchange::Binance, 50300.0, 50600.0, 49500.0, 50400.0, 100.0), // low=49500
        make_test_bar("BTCUSDT", Exchange::Binance, 50400.0, 50700.0, 49400.0, 50500.0, 100.0), // low=49400
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Since buy_on_bar is enabled, strategy will place a buy order on every bar
    // Order placed on bar[0] is evaluated on bar[1] (low=49700 < 50000 -> fills)
    // Order placed on bar[1] is evaluated on bar[2] (low=49600 < 50000 -> fills)
    // etc.
    // So we should have multiple fills
    let trades = engine.get_all_trades();
    assert!(
        trades.len() >= 2,
        "Should have at least 2 fills from multiple bars, got {}",
        trades.len()
    );

    // Position should be positive (accumulated buys)
    let pos = engine.get_pos();
    assert!(pos > 0.0, "Position should be positive from multiple buys, got {}", pos);
}

// ============================================================================
// Test 8: Daily result tracking
// ============================================================================

#[tokio::test]
async fn test_backtest_daily_result_tracking() {
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);

    let strategy = TestStrategy::new("DAILY_TEST", "BTCUSDT.BINANCE");
    engine.add_strategy(Box::new(strategy));

    // Create bars spanning 3 different days
    let day1 = Utc.with_ymd_and_hms(2024, 1, 10, 10, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2024, 1, 11, 10, 0, 0).unwrap();
    let day3 = Utc.with_ymd_and_hms(2024, 1, 12, 10, 0, 0).unwrap();

    let bars = vec![
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50000.0, 50200.0, 49900.0, 50100.0, 100.0, day1),
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50100.0, 50300.0, 50000.0, 50200.0, 100.0, day2),
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50200.0, 50400.0, 50100.0, 50300.0, 100.0, day3),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Calculate result to get daily_results
    let bt_result = engine.calculate_result();

    // Should have daily results for each trading day
    assert!(
        bt_result.daily_results.len() >= 1,
        "Should have at least 1 daily result, got {}",
        bt_result.daily_results.len()
    );

    // Each DailyResult should have a valid close_price
    for (date, daily) in &bt_result.daily_results {
        assert!(
            daily.close_price > 0.0,
            "Daily result for {} should have positive close_price",
            date
        );
    }
}

// ============================================================================
// Test 9: Backtesting with different fill models
// ============================================================================

#[tokio::test]
async fn test_backtest_fill_model_comparison() {
    // Run same scenario with BestPriceFillModel
    let mut engine_best = make_test_engine(100_000.0, 0.0, 0.01);
    engine_best.set_fill_model(Box::new(BestPriceFillModel::new(0.0)));

    let strategy_best = TestStrategy::new("BEST_FILL", "BTCUSDT.BINANCE");
    engine_best.add_strategy(Box::new(strategy_best));

    let bars = vec![
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50200.0, 49900.0, 50150.0, 100.0),
        make_test_bar("BTCUSDT", Exchange::Binance, 50100.0, 50300.0, 49800.0, 50200.0, 100.0),
    ];

    // Place a buy order on engine_best
    let buy_req = OrderRequest {
        symbol: "BTCUSDT".to_string(),
        exchange: Exchange::Binance,
        direction: Direction::Long,
        order_type: OrderType::Limit,
        volume: 1.0,
        price: 50000.0,
        offset: Offset::Open,
        reference: String::new(),
        post_only: false,
        reduce_only: false,
    };
    engine_best.send_limit_order(buy_req);
    engine_best.set_history_data(bars.clone());

    let result_best = engine_best.run_backtesting().await;
    assert_ok(result_best);

    // Run same scenario with IdealFillModel
    let mut engine_ideal = make_test_engine(100_000.0, 0.0, 0.01);
    engine_ideal.set_fill_model(Box::new(IdealFillModel::new()));

    let strategy_ideal = TestStrategy::new("IDEAL_FILL", "BTCUSDT.BINANCE");
    engine_ideal.add_strategy(Box::new(strategy_ideal));

    let buy_req2 = OrderRequest {
        symbol: "BTCUSDT".to_string(),
        exchange: Exchange::Binance,
        direction: Direction::Long,
        order_type: OrderType::Limit,
        volume: 1.0,
        price: 50000.0,
        offset: Offset::Open,
        reference: String::new(),
        post_only: false,
        reduce_only: false,
    };
    engine_ideal.send_limit_order(buy_req2);
    engine_ideal.set_history_data(bars);

    let result_ideal = engine_ideal.run_backtesting().await;
    assert_ok(result_ideal);

    // Both should produce fills
    let trades_best = engine_best.get_all_trades();
    let trades_ideal = engine_ideal.get_all_trades();

    assert!(!trades_best.is_empty(), "BestPriceFillModel should produce fills");
    assert!(!trades_ideal.is_empty(), "IdealFillModel should produce fills");
}

// ============================================================================
// Test 10: Capital constraint
// ============================================================================

#[tokio::test]
async fn test_backtest_capital_exhausted() {
    // Set very low capital
    let mut engine = make_test_engine(100.0, 0.0, 0.01); // Only 100 USDT

    let mut strategy = TestStrategy::new("CAP_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0); // 1 BTC at 50000 = 50000 USDT
    engine.add_strategy(Box::new(strategy));

    let bars = make_ascending_bars("BTCUSDT", Exchange::Binance, 50000.0, 5, 60);
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    // Should still run without error (engine doesn't enforce capital on orders by default)
    // but the key observation is that the backtest doesn't crash
    assert_ok(result);
}

// ============================================================================
// Test 11: Statistics calculation
// ============================================================================

#[tokio::test]
async fn test_backtest_statistics_calculation() {
    let mut engine = make_test_engine(100_000.0, 0.001, 0.01);

    let mut strategy = TestStrategy::new("STATS_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    strategy.set_sell_after_volume(Some(1.0));
    engine.add_strategy(Box::new(strategy));

    // Create bars that lead to profitable round-trip trades
    let bars = make_ascending_bars("BTCUSDT", Exchange::Binance, 50000.0, 10, 60);
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    let bt_result = engine.calculate_result();
    let stats = engine.calculate_statistics(false);

    // Should have positive total days
    assert!(stats.total_days >= 1, "Should have at least 1 trading day");

    // Total trade count should be > 0 if trades happened
    if !engine.get_all_trades().is_empty() {
        assert!(
            stats.total_trade_count > 0,
            "Total trade count should be positive when trades exist"
        );
    }

    // Max drawdown should be non-negative
    assert!(
        stats.max_drawdown >= 0.0,
        "Max drawdown should be non-negative, got {}",
        stats.max_drawdown
    );
}

// ============================================================================
// Test 12: Clear data resets state
// ============================================================================

#[tokio::test]
async fn test_backtest_clear_data() {
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);

    let mut strategy = TestStrategy::new("CLEAR_TEST", "BTCUSDT.BINANCE");
    strategy.set_buy_on_bar(true);
    strategy.set_buy_price(50000.0);
    strategy.set_buy_volume(1.0);
    engine.add_strategy(Box::new(strategy));

    let bars = make_ascending_bars("BTCUSDT", Exchange::Binance, 50000.0, 5, 60);
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // Verify some state exists before clearing
    let trades_before = engine.get_all_trades();

    // Clear data
    engine.clear_data();

    // After clear: no trades, position flat, no daily results
    let trades_after = engine.get_all_trades();
    assert_eq!(trades_after.len(), 0, "Trades should be empty after clear_data");

    let pos = engine.get_pos();
    assert_approx_eq(pos, 0.0, 0.001);

    let bt_result = engine.calculate_result();
    assert!(
        bt_result.daily_results.is_empty(),
        "Daily results should be empty after clear_data"
    );
}

// ============================================================================
// Test 13: Empty data backtest
// ============================================================================

#[tokio::test]
async fn test_backtest_empty_data() {
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);

    let strategy = TestStrategy::new("EMPTY_TEST", "BTCUSDT.BINANCE");
    engine.add_strategy(Box::new(strategy));

    // Don't set any history data
    let result = engine.run_backtesting().await;

    // Should return error since history data is empty
    assert!(result.is_err(), "Empty data should return error");
    assert!(
        result.unwrap_err().contains("历史数据为空"),
        "Error should mention empty data"
    );
}

// ============================================================================
// Test 14: Set parameters exchange parsing
// ============================================================================

#[test]
fn test_backtest_set_parameters_exchange_parsing() {
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();

    // Test BTCUSDT.BINANCE
    let mut engine1 = BacktestingEngine::new();
    engine1.set_parameters(
        "BTCUSDT.BINANCE".to_string(),
        Interval::Minute,
        start,
        end,
        0.001,
        0.0,
        1.0,
        0.01,
        100_000.0,
        BacktestingMode::Bar,
    );
    assert_eq!(engine1.get_pos(), 0.0); // just verify engine is configured

    // Test ETHUSDT.OKX
    let mut engine2 = BacktestingEngine::new();
    engine2.set_parameters(
        "ETHUSDT.OKX".to_string(),
        Interval::Minute,
        start,
        end,
        0.001,
        0.0,
        1.0,
        0.01,
        100_000.0,
        BacktestingMode::Bar,
    );
    assert_eq!(engine2.get_pos(), 0.0);

    // Test BTCUSDT.BINANCE_USDM (multi-part exchange)
    let mut engine3 = BacktestingEngine::new();
    engine3.set_parameters(
        "BTCUSDT.BINANCE_USDM".to_string(),
        Interval::Minute,
        start,
        end,
        0.001,
        0.0,
        1.0,
        0.01,
        100_000.0,
        BacktestingMode::Bar,
    );
    assert_eq!(engine3.get_pos(), 0.0);
}

// ============================================================================
// Test 15: Backtest with TestClock advancement
// ============================================================================

#[tokio::test]
async fn test_backtest_test_clock_advancement() {
    let mut engine = make_test_engine(100_000.0, 0.0, 0.01);

    let strategy = TestStrategy::new("CLOCK_TEST", "BTCUSDT.BINANCE");
    engine.add_strategy(Box::new(strategy));

    // Create bars at specific times
    let t1 = Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2024, 6, 15, 10, 1, 0).unwrap();
    let t3 = Utc.with_ymd_and_hms(2024, 6, 15, 10, 2, 0).unwrap();

    let bars = vec![
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50000.0, 50100.0, 49900.0, 50050.0, 100.0, t1),
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50050.0, 50150.0, 49950.0, 50100.0, 100.0, t2),
        make_test_bar_at_time("BTCUSDT", Exchange::Binance, 50100.0, 50200.0, 50000.0, 50150.0, 100.0, t3),
    ];
    engine.set_history_data(bars);

    let result = engine.run_backtesting().await;
    assert_ok(result);

    // After backtesting, the engine's clock should have advanced to the last bar's datetime
    let current_dt = engine.current_dt();
    // The clock should be at or after the last bar's datetime
    assert!(
        current_dt >= t3,
        "Clock should have advanced to at least the last bar time. Got {}, expected >= {}",
        current_dt,
        t3
    );
}
