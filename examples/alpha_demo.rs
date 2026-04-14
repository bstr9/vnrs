//! Alpha Demo Example
//! Demonstrates the usage of the alpha research framework

use chrono::{Duration, Utc};
use polars::prelude::*;
use trade_engine::alpha::dataset::{processor::get_all_processors, AlphaDataset};
use trade_engine::alpha::model::LinearRegressionModel;
use trade_engine::alpha::strategy::{AlphaStrategy, BacktestingEngine};
use trade_engine::alpha::{logger::AlphaLogger, AlphaLab};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Alpha Demo Example ===\n");

    let logger = AlphaLogger;
    logger.info("Starting Alpha Demo");

    let lab = AlphaLab::new();
    println!("✓ AlphaLab created at ./demo_alpha_lab\n");

    // Test dataset creation
    println!("Creating AlphaDataset...");

    // Create sample data
    let dates = vec![
        Utc::now() - Duration::days(10),
        Utc::now() - Duration::days(9),
        Utc::now() - Duration::days(8),
        Utc::now() - Duration::days(7),
        Utc::now() - Duration::days(6),
    ];

    let symbols = vec!["AAPL", "GOOGL", "MSFT"];

    let mut all_dates = Vec::new();
    let mut all_symbols = Vec::new();
    let mut all_opens = Vec::new();
    let mut all_closes = Vec::new();
    let mut all_volumes = Vec::new();

    for date in &dates {
        for symbol in &symbols {
            all_dates.push(*date);
            all_symbols.push(symbol.to_string());
            all_opens.push(100.0 + (date.timestamp() % 100) as f64);
            all_closes.push(100.0 + (date.timestamp() % 100) as f64 + 1.0);
            all_volumes.push(1000.0 + (date.timestamp() % 500) as f64);
        }
    }

    let df = DataFrame::new(vec![
        Column::new(
            "datetime".into(),
            all_dates
                .iter()
                .map(|dt| dt.timestamp_millis())
                .collect::<Vec<i64>>(),
        ),
        Column::new("vt_symbol".into(), all_symbols),
        Column::new("open".into(), all_opens),
        Column::new("close".into(), all_closes),
        Column::new("volume".into(), all_volumes),
    ])?;

    let dataset = AlphaDataset::new(
        df,
        ("2023-01-01".to_string(), "2023-01-03".to_string()),
        ("2023-01-04".to_string(), "2023-01-05".to_string()),
        ("2023-01-06".to_string(), "2023-01-07".to_string()),
    );

    println!("✓ AlphaDataset created with {} rows\n", dataset.df.height());

    // Test feature addition
    println!("Adding features...");
    let mut ds = dataset;
    ds.add_feature("returns".to_string(), "close / open - 1".to_string());
    ds.set_label("future_return".to_string());
    println!("✓ Added features: returns\n");

    // Test processors
    println!("Adding processors...");
    let processors = get_all_processors();
    println!(
        "Available processors: {:?}",
        processors.iter().map(|(name, _)| *name).collect::<Vec<_>>()
    );
    println!("✓ Processor system working\n");

    // Test model
    println!("Creating LinearRegressionModel...");
    let _model = LinearRegressionModel::new();
    // Skip fit for now since we don't have real data
    println!("✓ Model created (fit skipped - no data)\n");

    // Test backtesting engine
    println!("Creating BacktestingEngine...");
    let _engine = BacktestingEngine::new();
    println!("✓ BacktestingEngine created\n");

    // Test strategy
    println!("Creating AlphaStrategy...");
    let mut strategy = AlphaStrategy::new(
        "TestStrategy".to_string(),
        vec!["AAPL".to_string(), "GOOGL".to_string()],
        Default::default(),
    );
    strategy.on_init();
    println!("✓ Strategy initialized\n");

    // Test contract settings
    println!("Loading contract settings...");
    let _settings = lab.load_contract_settings();
    println!("✓ Contract settings loaded\n");

    // Test lab functionality
    println!("Testing AlphaLab...");
    println!("  Datasets: {:?}", lab.list_all_datasets());
    println!("  Models: {:?}", lab.list_all_models());
    println!("  Signals: {:?}", lab.list_all_signals());
    println!("✓ AlphaLab working\n");

    println!("=== Demo Complete ===");
    let logger = AlphaLogger;
    logger.info("Alpha Demo completed successfully");

    Ok(())
}
