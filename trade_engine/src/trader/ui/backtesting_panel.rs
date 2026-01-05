//! Backtesting UI Panel
//!
//! Provides GUI interface for backtesting configuration and result visualization

use chrono::{DateTime, NaiveDateTime, Utc};
use egui::{Button, Context, Grid, ScrollArea, Ui};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::backtesting::{BacktestingEngine, BacktestingMode, BacktestingStatistics};
use crate::trader::{Exchange, Interval};

#[cfg(feature = "python")]
use crate::python::{load_strategies_from_directory, PythonStrategyAdapter};

/// Backtesting panel state
pub struct BacktestingPanel {
    // Configuration
    vt_symbol: String,
    interval: Interval,
    start_date: String,
    end_date: String,
    rate: String,
    slippage: String,
    capital: String,
    mode: BacktestingMode,

    // Strategy configuration
    strategy_file: String,
    strategy_class: String,
    strategy_name: String,
    available_strategies: Vec<(String, String, String)>, // (file_name, file_path, class_name)
    selected_strategy_index: usize,

    // Parameters
    fast_window: String,
    slow_window: String,
    fixed_size: String,

    // Status
    is_running: bool,
    progress: f32,
    status_message: String,

    // Results
    results: Option<BacktestingStatistics>,
    daily_pnl: Vec<(f64, f64)>, // (day_index, pnl)

    // Engine
    engine: Arc<Mutex<Option<BacktestingEngine>>>,
}

impl Default for BacktestingPanel {
    fn default() -> Self {
        Self {
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            interval: Interval::Minute,
            start_date: "2024-01-01 00:00:00".to_string(),
            end_date: "2024-12-31 23:59:59".to_string(),
            rate: "0.0003".to_string(),
            slippage: "0.0001".to_string(),
            capital: "100000.0".to_string(),
            mode: BacktestingMode::Bar,
            strategy_file: "".to_string(),
            strategy_class: "BollChannelStrategy".to_string(),
            strategy_name: "BollChannel".to_string(),
            available_strategies: Vec::new(),
            selected_strategy_index: 0,
            fast_window: "10".to_string(),
            slow_window: "20".to_string(),
            fixed_size: "1.0".to_string(),
            is_running: false,
            progress: 0.0,
            status_message: "就绪".to_string(),
            results: None,
            daily_pnl: Vec::new(),
            engine: Arc::new(Mutex::new(None)),
        }
    }
}

impl BacktestingPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the panel
    pub fn ui(&mut self, ctx: &Context, ui: &mut Ui) {
        // Check for background thread results
        self.check_results();

        ui.heading("回测配置");
        ui.separator();

        ScrollArea::vertical().show(ui, |ui| {
            // Configuration section
            self.render_configuration(ui);

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Control buttons
            self.render_controls(ui);

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Status section
            self.render_status(ui);

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Results section
            if self.results.is_some() {
                self.render_results(ui);
            }
        });
    }

    /// Render configuration section
    fn render_configuration(&mut self, ui: &mut Ui) {
        Grid::new("backtest_config_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label("交易品种:");
                ui.text_edit_singleline(&mut self.vt_symbol);
                ui.end_row();

                ui.label("K线周期:");
                egui::ComboBox::from_label("")
                    .selected_text(format!("{:?}", self.interval))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.interval, Interval::Minute, "1分钟");
                        ui.selectable_value(&mut self.interval, Interval::Minute15, "15分钟");
                        ui.selectable_value(&mut self.interval, Interval::Hour, "1小时");
                        ui.selectable_value(&mut self.interval, Interval::Hour4, "4小时");
                        ui.selectable_value(&mut self.interval, Interval::Daily, "日线");
                    });
                ui.end_row();

                ui.label("开始时间:");
                ui.text_edit_singleline(&mut self.start_date);
                ui.end_row();

                ui.label("结束时间:");
                ui.text_edit_singleline(&mut self.end_date);
                ui.end_row();

                ui.label("手续费率:");
                ui.text_edit_singleline(&mut self.rate);
                ui.end_row();

                ui.label("滑点:");
                ui.text_edit_singleline(&mut self.slippage);
                ui.end_row();

                ui.label("初始资金:");
                ui.text_edit_singleline(&mut self.capital);
                ui.end_row();

                ui.label("回测模式:");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.mode, BacktestingMode::Bar, "Bar回测");
                    ui.radio_value(&mut self.mode, BacktestingMode::Tick, "Tick回测");
                });
                ui.end_row();

                ui.label("策略目录:");
                ui.horizontal(|ui| {
                    if ui.text_edit_singleline(&mut self.strategy_file).changed() {
                        // If user manually edits, treat as directory path
                    }

                    if ui.button("刷新策略列表").clicked() {
                        self.scan_strategies_directory();
                    }

                    if !self.available_strategies.is_empty() {
                        egui::ComboBox::from_id_source("strategy_selector")
                            .selected_text(
                                if self.selected_strategy_index < self.available_strategies.len() {
                                    &self.available_strategies[self.selected_strategy_index].0
                                } else {
                                    "选择策略"
                                },
                            )
                            .show_ui(ui, |ui| {
                                for (i, (name, _path, class_name)) in
                                    self.available_strategies.iter().enumerate()
                                {
                                    if ui
                                        .selectable_value(
                                            &mut self.selected_strategy_index,
                                            i,
                                            name,
                                        )
                                        .clicked()
                                    {
                                        // Update fields when selected
                                        self.strategy_file = self.available_strategies[i].1.clone();
                                        self.strategy_class = class_name.clone();
                                    }
                                }
                            });
                    }
                });
                ui.end_row();

                ui.label("策略类名:");
                ui.text_edit_singleline(&mut self.strategy_class);
                ui.end_row();

                ui.label("策略名称:");
                ui.text_edit_singleline(&mut self.strategy_name);
                ui.end_row();

                ui.label("快速周期:");
                ui.text_edit_singleline(&mut self.fast_window);
                ui.end_row();

                ui.label("慢速周期:");
                ui.text_edit_singleline(&mut self.slow_window);
                ui.end_row();

                ui.label("固定手数:");
                ui.text_edit_singleline(&mut self.fixed_size);
                ui.end_row();
            });
    }

    /// Render control buttons
    fn render_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("开始回测").clicked() && !self.is_running {
                self.start_backtesting();
            }

            if ui.button("停止回测").clicked() && self.is_running {
                self.stop_backtesting();
            }

            if ui.button("清空结果").clicked() {
                self.clear_results();
            }

            if ui.button("导出结果").clicked() && self.results.is_some() {
                self.export_results();
            }
        });
    }

    /// Render status section
    fn render_status(&mut self, ui: &mut Ui) {
        ui.heading("运行状态");

        ui.horizontal(|ui| {
            ui.label("状态:");
            ui.label(&self.status_message);
        });

        if self.is_running {
            ui.add(egui::ProgressBar::new(self.progress).show_percentage());
        }
    }

    /// Render results section
    fn render_results(&mut self, ui: &mut Ui) {
        ui.heading("回测结果");

        if let Some(ref stats) = self.results {
            Grid::new("backtest_results_grid")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label("开始日期:");
                    ui.label(format!("{}", stats.start_date));
                    ui.end_row();

                    ui.label("结束日期:");
                    ui.label(format!("{}", stats.end_date));
                    ui.end_row();

                    ui.label("总天数:");
                    ui.label(format!("{}", stats.total_days));
                    ui.end_row();

                    ui.label("盈利天数:");
                    ui.label(format!("{}", stats.profit_days));
                    ui.end_row();

                    ui.label("亏损天数:");
                    ui.label(format!("{}", stats.loss_days));
                    ui.end_row();

                    ui.label("结束余额:");
                    ui.label(format!("{:.2}", stats.end_balance));
                    ui.end_row();

                    ui.label("总净盈亏:");
                    ui.label(format!("{:.2}", stats.total_net_pnl));
                    ui.end_row();

                    ui.label("每日收益:");
                    ui.label(format!("{:.4}", stats.daily_return));
                    ui.end_row();

                    ui.label("夏普比率:");
                    ui.label(format!("{:.4}", stats.sharpe_ratio));
                    ui.end_row();

                    ui.label("最大回撤:");
                    ui.label(format!("{:.2}%", stats.max_drawdown_percent * 100.0));
                    ui.end_row();

                    ui.label("收益率标准差:");
                    ui.label(format!("{:.4}", stats.return_std));
                    ui.end_row();

                    ui.label("总手续费:");
                    ui.label(format!("{:.2}", stats.total_commission));
                    ui.end_row();

                    ui.label("总滑点:");
                    ui.label(format!("{:.2}", stats.total_slippage));
                    ui.end_row();

                    ui.label("总成交额:");
                    ui.label(format!("{:.2}", stats.total_turnover));
                    ui.end_row();

                    ui.label("总成交笔数:");
                    ui.label(format!("{}", stats.total_trade_count));
                    ui.end_row();
                });

            ui.add_space(10.0);

            // Chart would go here if egui plot is available
            if !self.daily_pnl.is_empty() {
                ui.heading("每日盈亏曲线");
                ui.label(format!("共 {} 个数据点", self.daily_pnl.len()));
            }
        }
    }

    /// Start backtesting
    fn start_backtesting(&mut self) {
        self.is_running = true;
        self.progress = 0.0;
        self.status_message = "正在初始化...".to_string();

        // Parse parameters
        let rate = self.rate.parse::<f64>().unwrap_or(0.0003);
        let slippage = self.slippage.parse::<f64>().unwrap_or(0.0001);
        let capital = self.capital.parse::<f64>().unwrap_or(100000.0);
        let vt_symbol = self.vt_symbol.clone();
        let interval = self.interval;
        let mode = self.mode.clone();

        // Parse dates
        let start = NaiveDateTime::parse_from_str(&self.start_date, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());

        let end = NaiveDateTime::parse_from_str(&self.end_date, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());

        // Strategy info
        let strategy_file = self.strategy_file.clone();
        let strategy_class = self.strategy_class.clone();
        let strategy_name = self.strategy_name.clone();

        // Clone engine arc to pass to thread
        let engine_arc = self.engine.clone();

        // Spawn thread
        thread::spawn(move || {
            // Create runtime
            let rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                // Create engine
                let mut engine = BacktestingEngine::new();
                engine.set_parameters(
                    vt_symbol.clone(),
                    interval,
                    start,
                    end,
                    rate,
                    slippage,
                    1.0,  // size
                    0.01, // pricetick
                    capital,
                    mode,
                );

                // Load data from database using DatabaseLoader
                use crate::backtesting::database::DatabaseLoader;

                let mut loader = DatabaseLoader::new();

                // Parse symbol and exchange from vt_symbol
                let parts: Vec<&str> = vt_symbol.split('.').collect();
                let symbol_only = parts.get(0).unwrap_or(&"BTCUSDT").to_string();
                let exchange_str = parts.get(1).unwrap_or(&"BINANCE");

                // Parse exchange from string
                let exchange = match exchange_str.to_uppercase().as_str() {
                    "BINANCE" => Exchange::Binance,
                    "BINANCE_USDM" => Exchange::BinanceUsdm,
                    "BINANCE_COINM" => Exchange::BinanceCoinm,
                    "OKEX" | "OKX" | "BYBIT" | "HUOBI" => Exchange::Global,
                    "LOCAL" => Exchange::Local,
                    _ => Exchange::Binance,
                };

                // Connect to database (PostgreSQL)
                let db_url = "postgresql://localhost/market_data";
                if let Err(_e) = loader.connect(db_url).await {
                    // If PostgreSQL fails, database feature might not be enabled
                    // Fall back to generating mock data
                    eprintln!("Database connection failed, using mock data");

                    let mut bars = Vec::new();
                    let mut base_price = 40000.0;

                    for i in 0..1000 {
                        let dt = start
                            + chrono::Duration::minutes(
                                match interval {
                                    Interval::Minute => 1,
                                    Interval::Minute15 => 15,
                                    Interval::Hour => 60,
                                    Interval::Hour4 => 240,
                                    Interval::Daily => 1440,
                                    _ => 15,
                                } * i,
                            );
                        if dt > end {
                            break;
                        }

                        let open_price = base_price + (rand::random::<f64>() - 0.5) * 100.0;
                        let high_price = open_price + rand::random::<f64>() * 50.0;
                        let low_price = open_price - rand::random::<f64>() * 50.0;
                        let close_price =
                            low_price + rand::random::<f64>() * (high_price - low_price);
                        base_price = close_price;

                        let bar = crate::trader::BarData {
                            gateway_name: "MOCK".to_string(),
                            symbol: symbol_only.clone(),
                            exchange,
                            datetime: dt,
                            interval: Some(interval),
                            open_price,
                            high_price,
                            low_price,
                            close_price,
                            volume: rand::random::<f64>() * 90.0 + 10.0,
                            turnover: 0.0,
                            open_interest: 0.0,
                            extra: None,
                        };
                        bars.push(bar);
                    }

                    engine.set_history_data(bars);
                } else {
                    // Load from database
                    match loader
                        .load_bar_data(&symbol_only, exchange, interval, start, end)
                        .await
                    {
                        Ok(bars) => {
                            if bars.is_empty() {
                                eprintln!(
                                    "No data found in database for {}.{}",
                                    symbol_only, exchange_str
                                );
                            } else {
                                eprintln!("Loaded {} bars from database", bars.len());
                            }
                            engine.set_history_data(bars);
                        }
                        Err(e) => {
                            eprintln!("Failed to load data from database: {}", e);
                            return;
                        }
                    }
                }

                // Load Strategy
                #[cfg(feature = "python")]
                {
                    use crate::python::PythonStrategyAdapter;

                    // Initialize Python interpreter if not already initialized
                    // This is required when running in a background thread
                    pyo3::prepare_freethreaded_python();

                    match PythonStrategyAdapter::load_from_file(
                        &strategy_file,
                        &strategy_class,
                        strategy_name.clone(),
                        vec![vt_symbol.clone()],
                        None,
                    ) {
                        Ok(adapter) => {
                            engine.add_strategy(Box::new(adapter));
                        }
                        Err(e) => {
                            eprintln!("Failed to load strategy: {}", e);
                            return;
                        }
                    }
                }

                // Run backtesting
                if let Err(e) = engine.run_backtesting().await {
                    eprintln!("Backtesting failed: {}", e);
                    // We should signal error to UI somehow, maybe via logging to engine logs
                    return;
                }

                // Calculate statistics
                engine.calculate_statistics(false);

                *engine_arc.lock().unwrap() = Some(engine);
            });
        });
    }

    /// Check for results (call this in ui() loop)
    fn check_results(&mut self) {
        // Poll the engine for results if we are running
        if self.is_running {
            let mut engine_guard = self.engine.lock().unwrap();
            if let Some(engine) = engine_guard.as_ref() {
                let logs = engine.get_logs();
                if !logs.is_empty() && logs.last().unwrap().contains("回测运行结束") {
                    // It finished!
                    self.is_running = false;
                    self.status_message = "回测完成".to_string();

                    let stats = engine.calculate_statistics(false);
                    self.results = Some(stats);

                    let res = engine.calculate_result();
                    self.daily_pnl = res
                        .daily_results
                        .values()
                        .enumerate()
                        .map(|(i, dr)| (i as f64, dr.net_pnl))
                        .collect();
                }
            }
        }
    }

    /// Stop backtesting
    fn stop_backtesting(&mut self) {
        self.is_running = false;
        self.status_message = "已停止".to_string();
    }

    /// Clear results
    fn clear_results(&mut self) {
        self.results = None;
        self.daily_pnl.clear();
        self.status_message = "就绪".to_string();
        self.progress = 0.0;
    }

    /// Export results
    fn export_results(&self) {
        // TODO: Implement export to CSV/JSON
        println!("导出回测结果...");
    }

    /// Browse for strategy file
    fn browse_strategy_file(&mut self) {
        #[cfg(feature = "python")]
        {
            // Open file dialog in blocking mode
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Python策略文件", &["py"])
                .add_filter("所有文件", &["*"])
                .set_title("选择Python策略文件")
                .pick_file()
            {
                if let Some(path_str) = path.to_str() {
                    self.strategy_file = path_str.to_string();
                    self.status_message = format!("已选择: {}", path_str);

                    // Auto-scan just this file to find class name?
                    // Or just let user click Scan?
                    // Let's rely on Scan button for getting class name for now or implementing parse here
                } else {
                    self.status_message = "无法解析文件路径".to_string();
                }
            } else {
                self.status_message = "未选择文件".to_string();
            }
        }

        #[cfg(not(feature = "python"))]
        {
            self.status_message = "Python功能未启用".to_string();
        }
    }

    /// Scan strategies directory
    fn scan_strategies_directory(&mut self) {
        #[cfg(feature = "python")]
        {
            use std::path::Path;

            // Determine directory to scan
            let dir = if !self.strategy_file.is_empty() {
                let path = Path::new(&self.strategy_file);

                if path.is_dir() {
                    // If it's already a directory, use it
                    self.strategy_file.clone()
                } else if path.is_file() {
                    // If it's a file, use parent directory
                    path.parent()
                        .map(|p| p.to_str().unwrap_or("."))
                        .unwrap_or("./examples")
                        .to_string()
                } else {
                    // Treat as directory path even if it doesn't exist yet
                    self.strategy_file.clone()
                }
            } else {
                // Default directories to try
                let default_dirs = vec![
                    r"D:\Code\quant\vnpy_ctastrategy\vnpy_ctastrategy\strategies",
                    "./examples",
                    "../vnpy_ctastrategy/vnpy_ctastrategy/strategies",
                ];

                // Find first existing directory
                default_dirs
                    .iter()
                    .find(|d| Path::new(d).is_dir())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "./examples".to_string())
            };

            match load_strategies_from_directory(&dir) {
                Ok(strategies) => {
                    self.available_strategies = strategies;
                    self.status_message = format!(
                        "在 {} 找到 {} 个策略文件",
                        dir,
                        self.available_strategies.len()
                    );
                }
                Err(e) => {
                    self.status_message = format!("扫描失败: {}", e);
                }
            }
        }

        #[cfg(not(feature = "python"))]
        {
            self.status_message = "Python功能未启用".to_string();
        }
    }
}
