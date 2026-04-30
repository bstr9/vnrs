//! Backtesting UI Panel
//!
//! Provides GUI interface for backtesting configuration and result visualization

use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use egui::{Color32, Context, Grid, Id, Pos2, Rect, ScrollArea, Stroke, Ui, Vec2};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::backtesting::{
    BacktestingEngine, BacktestingMode, BacktestingStatistics,
    BestPriceFillModel, TwoTierFillModel, SizeAwareFillModel,
    ProbabilisticFillModel, IdealFillModel, FillModel,
};
use crate::chart::TradeOverlay;
use crate::trader::{Exchange, Interval};
use super::workflow_state::{SharedWorkflowState, WorkflowAction, StrategyDeployConfig, DeployMode};
use crate::strategy::base::StrategySetting;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillModelType {
    #[default]
    BestPrice,
    Ideal,
    TwoTier,
    SizeAware,
    Probabilistic,
}

impl FillModelType {
    /// All available fill model variants
    pub const ALL: [FillModelType; 5] = [
        FillModelType::BestPrice,
        FillModelType::Ideal,
        FillModelType::TwoTier,
        FillModelType::SizeAware,
        FillModelType::Probabilistic,
    ];

    /// Chinese display name
    pub fn label(&self) -> &'static str {
        match self {
            FillModelType::BestPrice => "最优价格 (BestPrice)",
            FillModelType::Ideal => "理想成交 (Ideal)",
            FillModelType::TwoTier => "双层流动性 (TwoTier)",
            FillModelType::SizeAware => "规模感知 (SizeAware)",
            FillModelType::Probabilistic => "概率成交 (Probabilistic)",
        }
    }

    /// Model description shown in the UI
    pub fn description(&self) -> &'static str {
        match self {
            FillModelType::BestPrice => "乐观假设：订单总在K线最优价格成交，适合快速验证策略逻辑",
            FillModelType::Ideal => "理想条件：零滑点成交，限价单按委托价、市价单按收盘价成交",
            FillModelType::TwoTier => "双层流动性：小单高概率成交+低滑点，大单低概率+额外滑点",
            FillModelType::SizeAware => "规模感知：根据订单量占K线成交量比例动态调整冲击成本和成交比例",
            FillModelType::Probabilistic => "概率成交：限价单按设定概率成交，可模拟滑点出现的概率",
        }
    }

    /// Estimated fill rate description
    pub fn estimated_fill_rate(&self) -> &'static str {
        match self {
            FillModelType::BestPrice => "≈ 100% (乐观)",
            FillModelType::Ideal => "100% (无滑点)",
            FillModelType::TwoTier => "取决于订单规模",
            FillModelType::SizeAware => "取决于成交量占比",
            FillModelType::Probabilistic => "按设定概率",
        }
    }
}

/// Backtesting sub-tab selection
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BacktestingTab {
    #[default]
    Backtest,
    Optimization,
}

impl BacktestingTab {
    pub const ALL: [BacktestingTab; 2] = [BacktestingTab::Backtest, BacktestingTab::Optimization];

    pub fn label(&self) -> &'static str {
        match self {
            BacktestingTab::Backtest => "策略回测",
            BacktestingTab::Optimization => "参数优化",
        }
    }
}

/// Parameter search range configuration for optimization
#[derive(Clone, Debug)]
pub struct OptParamConfig {
    pub name: String,
    pub start: String,
    pub end: String,
    pub step: String,
}

impl OptParamConfig {
    pub fn new(name: &str, start: &str, end: &str, step: &str) -> Self {
        Self {
            name: name.to_string(),
            start: start.to_string(),
            end: end.to_string(),
            step: step.to_string(),
        }
    }

    /// Convert to a backtesting::Parameter, returns None if parsing fails
    pub fn to_parameter(&self) -> Option<crate::backtesting::Parameter> {
        let start = self.start.parse::<f64>().ok()?;
        let end = self.end.parse::<f64>().ok()?;
        let step = self.step.parse::<f64>().ok()?;
        if step <= 0.0 || start > end {
            return None;
        }
        Some(crate::backtesting::Parameter::new(&self.name, start, end, step))
    }
}

/// Optimization target selection (simplified, Copy-friendly version for UI)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptTargetType {
    #[default]
    SharpeRatio,
    TotalReturn,
    ProfitFactor,
    MaxDrawdown,
}

impl OptTargetType {
    pub const ALL: [OptTargetType; 4] = [
        OptTargetType::SharpeRatio,
        OptTargetType::TotalReturn,
        OptTargetType::ProfitFactor,
        OptTargetType::MaxDrawdown,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            OptTargetType::SharpeRatio => "夏普比率",
            OptTargetType::TotalReturn => "总收益率",
            OptTargetType::ProfitFactor => "盈亏比",
            OptTargetType::MaxDrawdown => "最大回撤",
        }
    }

    /// Convert to the backend OptimizationTarget
    pub fn to_optimization_target(&self) -> crate::backtesting::OptimizationTarget {
        match self {
            OptTargetType::SharpeRatio => crate::backtesting::OptimizationTarget::SharpeRatio,
            OptTargetType::TotalReturn => crate::backtesting::OptimizationTarget::TotalReturn,
            OptTargetType::ProfitFactor => crate::backtesting::OptimizationTarget::Custom(
                std::sync::Arc::new(|stats| stats.profit_factor),
            ),
            OptTargetType::MaxDrawdown => crate::backtesting::OptimizationTarget::MaxDrawdown,
        }
    }
}

/// Heatmap data for 2-parameter optimization visualization
#[derive(Clone, Debug)]
pub struct HeatmapData {
    pub x_name: String,
    pub y_name: String,
    pub x_values: Vec<f64>,
    pub y_values: Vec<f64>,
    /// y_rows × x_cols, each cell is the target value
    pub values: Vec<Vec<f64>>,
}

/// Optimization results display data
#[derive(Clone, Debug)]
pub struct OptimizationResultsDisplay {
    pub best_params: std::collections::HashMap<String, f64>,
    pub best_target_value: f64,
    pub best_statistics: crate::backtesting::BacktestingStatistics,
    pub heatmap_data: Option<HeatmapData>,
    pub all_results_count: usize,
}

#[cfg(feature = "python")]
use crate::python::load_strategies_from_directory;

/// Simple date picker widget with popup calendar
#[derive(Clone)]
pub struct DatePicker {
    year: i32,
    month: u32,
    day: u32,
    show_popup: bool,
}

impl DatePicker {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            year,
            month: month.clamp(1, 12),
            day: day.clamp(1, 31),
            show_popup: false,
        }
    }

    /// Create from `chrono::NaiveDate`
    pub fn from_date(date: chrono::NaiveDate) -> Self {
        Self::new(date.year(), date.month(), date.day())
    }

    /// Show the date picker widget
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // value fits in target type; value is non-negative
    pub fn show(&mut self, ui: &mut Ui, label: &str, popup_id: Id) {
        ui.horizontal(|ui| {
            ui.label(label);
            let text = format!("{:04}-{:02}-{:02}", self.year, self.month, self.day);
            let response = ui.button(&text);
            if response.clicked() {
                self.show_popup = !self.show_popup;
            }
        });

        if self.show_popup {
            egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(220.0);

                        // Year / Month navigation
                        ui.horizontal(|ui| {
                            if ui.button("◀").clicked() {
                                self.month -= 1;
                                if self.month == 0 {
                                    self.month = 12;
                                    self.year -= 1;
                                }
                            }
                            ui.label(format!("{:04}年 {:02}月", self.year, self.month));
                            if ui.button("▶").clicked() {
                                self.month += 1;
                                if self.month > 12 {
                                    self.month = 1;
                                    self.year += 1;
                                }
                            }
                        });

                        ui.add_space(4.0);

                        // Day-of-week header
                        ui.horizontal(|ui| {
                            for day_name in &["日", "一", "二", "三", "四", "五", "六"] {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(28.0, 16.0),
                                    egui::Layout::centered_and_justified(
                                        egui::Direction::TopDown,
                                    ),
                                    |ui| {
                                        ui.label(egui::RichText::new(*day_name).small().strong());
                                    },
                                );
                            }
                        });

                        // Calendar grid
                        let first_day = chrono::NaiveDate::from_ymd_opt(self.year, self.month, 1);
                        let days_in_month = first_day
                            .and_then(|d| {
                                (d + chrono::Duration::days(32))
                                    .with_day(1)
                                    .map(|next| (next - d).num_days() as u32)
                            })
                            .unwrap_or(30);
                        let mut weekday_offset = first_day
                            .map(|d| d.weekday().num_days_from_sunday())
                            .unwrap_or(0);

                        let mut day_counter: u32 = 0;
                        for _week in 0..6 {
                            if day_counter >= days_in_month {
                                break;
                            }
                            ui.horizontal(|ui| {
                                for _weekday in 0..7 {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(28.0, 22.0),
                                        egui::Layout::centered_and_justified(
                                            egui::Direction::TopDown,
                                        ),
                                        |ui| {
                                            if day_counter >= days_in_month {
                                                return;
                                            }
                                            if weekday_offset > 0 {
                                                weekday_offset -= 1;
                                                return;
                                            }
                                            day_counter += 1;
                                            let is_selected = day_counter == self.day;
                                            if is_selected {
                                                ui.painter().circle_filled(
                                                    ui.available_rect_before_wrap().center(),
                                                    10.0,
                                                    ui.style().visuals.selection.bg_fill,
                                                );
                                            }
                                            let text_color = if is_selected {
                                                ui.style().visuals.selection.stroke.color
                                            } else {
                                                ui.style().visuals.text_color()
                                            };
                                            ui.label(
                                                egui::RichText::new(format!("{day_counter}"))
                                                    .color(text_color),
                                            );
                                            if ui.allocate_response(
                                                Vec2::splat(22.0),
                                                egui::Sense::click(),
                                            ).clicked()
                                            {
                                                self.day = day_counter;
                                                self.show_popup = false;
                                            }
                                        },
                                    );
                                }
                            });
                        }

                        ui.add_space(4.0);

                        // Manual entry row
                        ui.horizontal(|ui| {
                            ui.label("日期:");
                            ui.add(
                                egui::DragValue::new(&mut self.year)
                                    .speed(1.0)
                                    .range(2020..=2030)
                                    .custom_formatter(|n, _| format!("{n:.0}")),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut self.month)
                                    .speed(0.1)
                                    .range(1..=12)
                                    .custom_formatter(|n, _| format!("{n:02.0}")),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut self.day)
                                    .speed(0.1)
                                    .range(1..=31)
                                    .custom_formatter(|n, _| format!("{n:02.0}")),
                            );
                        });

                        if ui.button("确定").clicked() {
                            self.show_popup = false;
                        }
                    });
                });

            // Close popup when clicking elsewhere
            if ui.ctx().input(|i| i.pointer.any_click()) {
                // Let the popup content handle its own clicks; close only on outside clicks
                // This is handled naturally by egui's area system
            }
        }
    }

    /// Convert to datetime string format expected by backtesting engine
    pub fn to_datetime_string(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} 00:00:00",
            self.year, self.month, self.day
        )
    }

    /// Convert to end-of-day datetime string
    pub fn to_end_datetime_string(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} 23:59:59",
            self.year, self.month, self.day
        )
    }

    /// Convert to YYYYMMDD format
    #[allow(dead_code)]
    pub fn to_date_string(&self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }
}

/// Backtesting panel state
pub struct BacktestingPanel {
    // Configuration
    vt_symbol: String,
    interval: Interval,
    start_date_picker: DatePicker,
    end_date_picker: DatePicker,
    rate: String,
    slippage: String,
    capital: String,
    mode: BacktestingMode,

    // Fill model configuration
    fill_model_type: FillModelType,

    // TwoTier parameters
    two_tier_slippage_base: String,
    two_tier_slippage_extra: String,
    two_tier_size_threshold: String,
    two_tier_prob_base: String,
    two_tier_prob_large: String,

    // SizeAware parameters
    size_aware_base_slippage: String,
    size_aware_max_slippage: String,
    size_aware_impact_coefficient: String,
    size_aware_max_fill_pct: String,

    // Probabilistic parameters
    prob_slippage: String,
    prob_fill_on_limit: String,
    prob_slippage_probability: String,

    // Strategy configuration
    strategy_file: String,
    strategy_class: String,
    strategy_name: String,
    available_strategies: Vec<(String, String, String)>, // (file_name, file_path, class_name)
    selected_strategy_index: usize,
    strategies_scanned: bool,

    // Parameters
    fast_window: String,
    slow_window: String,
    fixed_size: String,

    // Status
    is_running: bool,
    progress: f32,
    status_message: String,
    backtest_error: Option<String>,

    // Results
    results: Option<BacktestingStatistics>,
    daily_pnl: Vec<(f64, f64)>, // (day_index, pnl)

    // Data source warning
    using_mock_data: bool,
    using_mock_data_flag: Arc<Mutex<bool>>,
    // Error from background thread
    backtest_error_flag: Arc<Mutex<Option<String>>>,

    // Trade overlay for chart visualization
    trade_overlay: TradeOverlay,

    // Engine
    engine: Arc<Mutex<Option<BacktestingEngine>>>,

    // Workflow state for cross-panel coordination
    workflow_state: Option<SharedWorkflowState>,

    // Tab selection
    active_tab: BacktestingTab,

    // Optimization configuration
    opt_parameters: Vec<OptParamConfig>,
    opt_target: OptTargetType,
    opt_results: Option<OptimizationResultsDisplay>,
    opt_is_running: bool,
    opt_progress: f32,
    opt_error: Option<String>,
    // Shared result receiver for optimization background thread
    opt_result_flag: Arc<Mutex<Option<OptimizationResultsDisplay>>>,
    opt_error_flag: Arc<Mutex<Option<String>>>,
}

impl Default for BacktestingPanel {
    fn default() -> Self {
        let now = chrono::Local::now();
        let end_date = now.date_naive();
        let start_date = end_date - chrono::Duration::days(365);

        Self {
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            interval: Interval::Minute,
            start_date_picker: DatePicker::from_date(start_date),
            end_date_picker: DatePicker::from_date(end_date),
            rate: "0.0003".to_string(),
            slippage: "0.0001".to_string(),
            capital: "100000.0".to_string(),
            mode: BacktestingMode::Bar,
            fill_model_type: FillModelType::default(),
            two_tier_slippage_base: "0.1".to_string(),
            two_tier_slippage_extra: "0.2".to_string(),
            two_tier_size_threshold: "100.0".to_string(),
            two_tier_prob_base: "1.0".to_string(),
            two_tier_prob_large: "0.8".to_string(),
            size_aware_base_slippage: "0.1".to_string(),
            size_aware_max_slippage: "1.0".to_string(),
            size_aware_impact_coefficient: "0.5".to_string(),
            size_aware_max_fill_pct: "0.5".to_string(),
            prob_slippage: "0.2".to_string(),
            prob_fill_on_limit: "0.9".to_string(),
            prob_slippage_probability: "0.5".to_string(),
            strategy_file: "".to_string(),
            strategy_class: "BollChannelStrategy".to_string(),
            strategy_name: "BollChannel".to_string(),
            available_strategies: Vec::new(),
            selected_strategy_index: 0,
            strategies_scanned: false,
            fast_window: "10".to_string(),
            slow_window: "20".to_string(),
            fixed_size: "1.0".to_string(),
            is_running: false,
            progress: 0.0,
            status_message: "就绪".to_string(),
            backtest_error: None,
            results: None,
            daily_pnl: Vec::new(),
            using_mock_data: false,
            using_mock_data_flag: Arc::new(Mutex::new(false)),
            backtest_error_flag: Arc::new(Mutex::new(None)),
            trade_overlay: TradeOverlay::new(),
            engine: Arc::new(Mutex::new(None)),
            workflow_state: None,
            active_tab: BacktestingTab::default(),
            opt_parameters: vec![
                OptParamConfig::new("fast_window", "5", "30", "5"),
                OptParamConfig::new("slow_window", "20", "60", "10"),
            ],
            opt_target: OptTargetType::default(),
            opt_results: None,
            opt_is_running: false,
            opt_progress: 0.0,
            opt_error: None,
            opt_result_flag: Arc::new(Mutex::new(None)),
            opt_error_flag: Arc::new(Mutex::new(None)),
        }
    }
}

impl BacktestingPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the shared workflow state for cross-panel coordination
    pub fn set_workflow_state(&mut self, state: SharedWorkflowState) {
        self.workflow_state = Some(state);
    }

    /// Render the panel
    pub fn ui(&mut self, _ctx: &Context, ui: &mut Ui) {
        // Check for background thread results
        self.check_results();
        self.check_optimization_results();

        // Auto-scan strategies on first render if not already scanned
        #[cfg(feature = "python")]
        if !self.strategies_scanned {
            self.scan_strategies_directory();
            self.strategies_scanned = true;
        }

        ui.heading("回测配置");
        ui.separator();

        // Tab bar
        ui.horizontal(|ui| {
            for tab in BacktestingTab::ALL {
                let is_active = self.active_tab == tab;
                let text = if is_active {
                    egui::RichText::new(tab.label()).strong()
                } else {
                    egui::RichText::new(tab.label())
                };
                if ui.selectable_label(is_active, text).clicked() {
                    self.active_tab = tab;
                }
            }
        });
        ui.separator();

        ScrollArea::vertical().show(ui, |ui| {
            match self.active_tab {
                BacktestingTab::Backtest => self.render_backtest_tab(ui),
                BacktestingTab::Optimization => self.render_optimization_tab(ui),
            }
        });
    }

    /// Render the backtest sub-tab
    fn render_backtest_tab(&mut self, ui: &mut Ui) {
        // Configuration section
        self.render_configuration(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Fill model configuration section
        self.render_fill_model_config(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Configuration summary
        self.render_config_summary(ui);

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
    }

    /// Render the optimization sub-tab
    fn render_optimization_tab(&mut self, ui: &mut Ui) {
        // Optimization configuration
        self.render_optimization_config(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Optimization controls
        self.render_optimization_controls(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Optimization status
        self.render_optimization_status(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Optimization results
        if self.opt_results.is_some() {
            self.render_optimization_results(ui);
        }
    }

    /// Render optimization parameter configuration
    fn render_optimization_config(&mut self, ui: &mut Ui) {
        ui.heading("参数搜索范围");

        // Re-use the same basic config as backtest (vt_symbol, interval, dates, etc.)
        egui::Frame::group(ui.style())
            .inner_margin(8.0)
            .show(ui, |ui| {
                Grid::new("opt_base_config_grid")
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
                        self.start_date_picker.show(ui, "", ui.auto_id_with("opt_start_date_popup"));
                        ui.end_row();

                        ui.label("结束时间:");
                        self.end_date_picker.show(ui, "", ui.auto_id_with("opt_end_date_popup"));
                        ui.end_row();

                        ui.label("策略文件:");
                        ui.text_edit_singleline(&mut self.strategy_file);
                        ui.end_row();

                        ui.label("策略类名:");
                        ui.text_edit_singleline(&mut self.strategy_class);
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // Parameter list
        ui.heading("搜索参数");
        ui.add_space(4.0);

        // Target selector
        ui.horizontal(|ui| {
            ui.label("优化目标:");
            egui::ComboBox::from_id_salt("opt_target_selector")
                .selected_text(self.opt_target.label())
                .show_ui(ui, |ui| {
                    for target in OptTargetType::ALL {
                        ui.selectable_value(&mut self.opt_target, target, target.label());
                    }
                });
        });

        ui.add_space(8.0);

        // Parameter rows header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("参数名").strong());
            ui.add_space(60.0);
            ui.label(egui::RichText::new("起始").strong());
            ui.add_space(40.0);
            ui.label(egui::RichText::new("结束").strong());
            ui.add_space(40.0);
            ui.label(egui::RichText::new("步长").strong());
        });

        // Parameter rows
        let mut remove_index: Option<usize> = None;
        for (i, param) in self.opt_parameters.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut param.name).desired_width(80.0));
                ui.add(egui::TextEdit::singleline(&mut param.start).desired_width(60.0));
                ui.add(egui::TextEdit::singleline(&mut param.end).desired_width(60.0));
                ui.add(egui::TextEdit::singleline(&mut param.step).desired_width(60.0));
                if ui.button("删除").clicked() {
                    remove_index = Some(i);
                }
            });
        }

        if let Some(idx) = remove_index {
            self.opt_parameters.remove(idx);
        }

        ui.add_space(4.0);

        if ui.button("➕ 添加参数").clicked() {
            self.opt_parameters.push(OptParamConfig::new(
                "new_param", "1", "10", "1",
            ));
        }

        // Show estimated combinations count
        let total_combos: usize = self.opt_parameters.iter().map(|p| {
            if let Ok(start) = p.start.parse::<f64>() {
                if let Ok(end) = p.end.parse::<f64>() {
                    if let Ok(step) = p.step.parse::<f64>() {
                        if step > 0.0 && end >= start {
                            return ((end - start) / step + 1.0).ceil() as usize;
                        }
                    }
                }
            }
            0
        }).product();
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("预估参数组合数: {}", total_combos))
            .small()
            .color(ui.style().visuals.weak_text_color()));
    }

    /// Render optimization control buttons
    fn render_optimization_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.opt_is_running, egui::Button::new("开始优化"))
                .clicked() && !self.opt_is_running
            {
                self.start_optimization();
            }

            if ui.add_enabled(self.opt_is_running, egui::Button::new("停止优化"))
                .clicked() && self.opt_is_running
            {
                self.opt_is_running = false;
            }

            if ui.button("清空结果").clicked() {
                self.opt_results = None;
                self.opt_error = None;
                self.opt_progress = 0.0;
            }
        });
    }

    /// Render optimization status
    fn render_optimization_status(&mut self, ui: &mut Ui) {
        ui.heading("优化状态");

        if self.opt_is_running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在运行优化...");
            });
            ui.add(egui::ProgressBar::new(self.opt_progress).show_percentage());
        } else if self.opt_results.is_some() {
            ui.label("优化完成 ✓");
        } else if let Some(ref err) = self.opt_error {
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), format!("❌ {}", err));
        } else {
            ui.label("就绪");
        }
    }

    /// Render optimization results
    fn render_optimization_results(&mut self, ui: &mut Ui) {
        let Some(ref results) = self.opt_results else {
            return;
        };

        ui.heading("优化结果");
        ui.add_space(4.0);

        // Best parameters section
        ui.heading("最优参数");
        egui::Frame::group(ui.style())
            .inner_margin(8.0)
            .show(ui, |ui| {
                Grid::new("opt_best_params_grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        for (name, value) in &results.best_params {
                            ui.label(egui::RichText::new(format!("{}:", name)).strong());
                            ui.label(format!("{:.4}", value));
                            ui.end_row();
                        }

                        ui.label(egui::RichText::new("目标值:").strong());
                        ui.label(format!("{:.4}", results.best_target_value));
                        ui.end_row();

                        ui.label(egui::RichText::new("总组合数:").strong());
                        ui.label(format!("{}", results.all_results_count));
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // Best statistics section
        ui.heading("最优回测统计");
        egui::Frame::group(ui.style())
            .inner_margin(8.0)
            .show(ui, |ui| {
                let stats = &results.best_statistics;
                Grid::new("opt_best_stats_grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("结束余额:");
                        ui.label(format!("{:.2}", stats.end_balance));
                        ui.end_row();

                        ui.label("总净盈亏:");
                        ui.label(format!("{:.2}", stats.total_net_pnl));
                        ui.end_row();

                        ui.label("夏普比率:");
                        ui.label(format!("{:.4}", stats.sharpe_ratio));
                        ui.end_row();

                        ui.label("最大回撤:");
                        ui.label(format!("{:.2}%", stats.max_drawdown_percent * 100.0));
                        ui.end_row();

                        ui.label("总成交笔数:");
                        ui.label(format!("{}", stats.total_trade_count));
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // Heatmap (only for 2 parameters)
        if let Some(ref heatmap) = results.heatmap_data {
            ui.heading("参数热力图");
            self.render_heatmap(ui, heatmap);
        }

        ui.add_space(8.0);

        // Action buttons
        ui.horizontal(|ui| {
            if ui.button("应用到回测").clicked() {
                self.apply_optimal_to_backtest();
            }

            let deploy_button = ui.add(
                egui::Button::new(
                    egui::RichText::new("🚀 一键部署到模拟交易").color(egui::Color32::WHITE)
                )
                .fill(egui::Color32::from_rgb(50, 130, 220))
            );
            if deploy_button.clicked() {
                self.deploy_optimized_to_paper();
            }
        });
    }

    /// Render 2D heatmap using egui painter
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
    fn render_heatmap(&self, ui: &mut Ui, heatmap: &HeatmapData) {
        let available_width = ui.available_width();
        let legend_width = 60.0;
        let label_margin = 50.0;
        let top_margin = 20.0;

        // Calculate cell size
        let grid_width = available_width - label_margin - legend_width;
        let grid_height = 280.0 - top_margin;

        let x_count = heatmap.x_values.len().max(1);
        let y_count = heatmap.y_values.len().max(1);

        let cell_w = (grid_width / x_count as f32).clamp(20.0, 60.0);
        let cell_h = (grid_height / y_count as f32).clamp(20.0, 60.0);

        let total_w = label_margin + cell_w * x_count as f32 + legend_width;
        let total_h = top_margin + cell_h * y_count as f32 + 40.0;

        let (response, painter) = ui.allocate_painter(
            Vec2::new(total_w, total_h),
            egui::Sense::hover(),
        );
        let rect = response.rect;

        // Find min/max values
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for row in &heatmap.values {
            for &v in row {
                min_val = min_val.min(v);
                max_val = max_val.max(v);
            }
        }
        let val_range = (max_val - min_val).max(1e-10);

        // X axis label (column name)
        painter.text(
            Pos2::new(rect.left() + label_margin, rect.top()),
            egui::Align2::LEFT_TOP,
            &heatmap.x_name,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 180, 180),
        );

        // Y axis label
        painter.text(
            Pos2::new(rect.left(), rect.top() + top_margin),
            egui::Align2::LEFT_TOP,
            &heatmap.y_name,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 180, 180),
        );

        // Draw cells
        let grid_origin = Pos2::new(rect.left() + label_margin, rect.top() + top_margin);

        // Determine label skip for axis labels
        let x_label_step = if x_count > 10 { (x_count / 5).max(1) } else { 1 };
        let y_label_step = if y_count > 10 { (y_count / 5).max(1) } else { 1 };

        for (yi, row) in heatmap.values.iter().enumerate() {
            for (xi, &val) in row.iter().enumerate() {
                let cell_rect = Rect::from_min_size(
                    Pos2::new(
                        grid_origin.x + xi as f32 * cell_w,
                        grid_origin.y + yi as f32 * cell_h,
                    ),
                    Vec2::new(cell_w, cell_h),
                );

                let t = ((val - min_val) / val_range) as f32;
                let color = heatmap_color(t);

                painter.rect_filled(cell_rect, 0.0, color);
                painter.rect_stroke(cell_rect, 0.0, Stroke::new(0.5, Color32::from_rgb(40, 40, 40)), egui::StrokeKind::Inside);

                // X axis labels
                if yi == 0 && xi % x_label_step == 0 {
                    let x = cell_rect.center().x;
                    let y = grid_origin.y + y_count as f32 * cell_h + 2.0;
                    painter.text(
                        Pos2::new(x, y),
                        egui::Align2::CENTER_TOP,
                        format!("{:.1}", heatmap.x_values[xi]),
                        egui::FontId::proportional(9.0),
                        Color32::from_rgb(160, 160, 160),
                    );
                }

                // Y axis labels
                if xi == 0 && yi % y_label_step == 0 {
                    let x = grid_origin.x - 4.0;
                    let y = cell_rect.center().y;
                    painter.text(
                        Pos2::new(x, y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{:.1}", heatmap.y_values[yi]),
                        egui::FontId::proportional(9.0),
                        Color32::from_rgb(160, 160, 160),
                    );
                }
            }
        }

        // Show value tooltip on hover
        if let Some(hover_pos) = response.hover_pos() {
            let rel_x = hover_pos.x - grid_origin.x;
            let rel_y = hover_pos.y - grid_origin.y;
            let xi = (rel_x / cell_w).floor() as usize;
            let yi = (rel_y / cell_h).floor() as usize;
            if xi < x_count && yi < y_count {
                if let Some(row) = heatmap.values.get(yi) {
                    if let Some(&val) = row.get(xi) {
                        let tooltip = format!(
                            "{}={:.2}, {}={:.2}\n目标值: {:.4}",
                            heatmap.x_name, heatmap.x_values[xi],
                            heatmap.y_name, heatmap.y_values[yi],
                            val
                        );
                        egui::Area::new(ui.auto_id_with("heatmap_tooltip"))
                            .pivot(egui::Align2::LEFT_BOTTOM)
                            .current_pos(hover_pos + egui::vec2(4.0, -4.0))
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.label(&tooltip);
                                });
                            });
                    }
                }
            }
        }

        // Color legend bar on the right
        let legend_x = grid_origin.x + x_count as f32 * cell_w + 10.0;
        let legend_y = grid_origin.y;
        let legend_h = y_count as f32 * cell_h;
        let legend_bar_w = 16.0;

        let num_legend_steps = 20;
        let step_h = legend_h / num_legend_steps as f32;
        for i in 0..num_legend_steps {
            let t = 1.0 - i as f32 / (num_legend_steps - 1) as f32;
            let color = heatmap_color(t);
            let step_rect = Rect::from_min_size(
                Pos2::new(legend_x, legend_y + i as f32 * step_h),
                Vec2::new(legend_bar_w, step_h + 1.0),
            );
            painter.rect_filled(step_rect, 0.0, color);
        }

        // Legend labels
        painter.text(
            Pos2::new(legend_x + legend_bar_w + 4.0, legend_y),
            egui::Align2::LEFT_TOP,
            format!("{:.2}", max_val),
            egui::FontId::proportional(9.0),
            Color32::from_rgb(160, 160, 160),
        );
        painter.text(
            Pos2::new(legend_x + legend_bar_w + 4.0, legend_y + legend_h),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.2}", min_val),
            egui::FontId::proportional(9.0),
            Color32::from_rgb(160, 160, 160),
        );
    }

    /// Start optimization in background thread
    fn start_optimization(&mut self) {
        // Validate parameters
        if self.opt_parameters.is_empty() {
            self.opt_error = Some("请至少配置一个搜索参数".to_string());
            return;
        }

        // Validate all parameters can be parsed
        for param in &self.opt_parameters {
            if param.name.trim().is_empty() {
                self.opt_error = Some("参数名不能为空".to_string());
                return;
            }
            if param.to_parameter().is_none() {
                self.opt_error = Some(format!(
                    "参数 '{}' 配置无效 (起始/结束/步长需为有效数字，步长>0)",
                    param.name
                ));
                return;
            }
        }

        // Validate strategy
        if self.strategy_file.trim().is_empty() || self.strategy_class.trim().is_empty() {
            self.opt_error = Some("请选择策略文件和类名".to_string());
            return;
        }

        self.opt_is_running = true;
        self.opt_progress = 0.0;
        self.opt_error = None;
        self.opt_results = None;

        // Clone everything needed for the background thread
        let vt_symbol = self.vt_symbol.clone();
        let interval = self.interval;
        let mode = self.mode;
        let rate = self.rate.parse::<f64>().unwrap_or(0.0003);
        let slippage = self.slippage.parse::<f64>().unwrap_or(0.0001);
        let capital = self.capital.parse::<f64>().unwrap_or(100000.0);

        let start_str = self.start_date_picker.to_datetime_string();
        let end_str = self.end_date_picker.to_end_datetime_string();
        let start = NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());
        let end = NaiveDateTime::parse_from_str(&end_str, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());

        #[cfg(feature = "python")]
        let (strategy_file, strategy_class, strategy_name) = (
            self.strategy_file.clone(),
            self.strategy_class.clone(),
            self.strategy_name.clone(),
        );
        #[cfg(not(feature = "python"))]
        let _ = (&self.strategy_file, &self.strategy_class, &self.strategy_name);

        let parameters: Vec<crate::backtesting::Parameter> = self.opt_parameters
            .iter()
            .filter_map(|p| p.to_parameter())
            .collect();
        let param_names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
        let opt_target = self.opt_target;
        let result_flag = self.opt_result_flag.clone();
        let error_flag = self.opt_error_flag.clone();

        thread::spawn(move || {
            // Create optimization engine
            let settings = crate::backtesting::OptimizationSettings {
                vt_symbol: vt_symbol.clone(),
                interval,
                start,
                end,
                rate,
                slippage,
                size: 1.0,
                pricetick: 0.01,
                capital,
                mode,
            };
            let mut opt_engine = crate::backtesting::OptimizationEngine::new(settings);

            for param in parameters {
                opt_engine.add_parameter(param);
            }

            // Load data from database
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for optimization");
            let history_data = rt.block_on(async {
                use crate::backtesting::database::DatabaseLoader;
                let mut loader = DatabaseLoader::new();

                let parts: Vec<&str> = vt_symbol.split('.').collect();
                let symbol_only = parts.first().unwrap_or(&"BTCUSDT").to_string();
                let exchange_str = parts.get(1).unwrap_or(&"BINANCE");
                let exchange = match exchange_str.to_uppercase().as_str() {
                    "BINANCE" => Exchange::Binance,
                    "BINANCE_USDM" => Exchange::BinanceUsdm,
                    "BINANCE_COINM" => Exchange::BinanceCoinm,
                    "OKX" => Exchange::Okx,
                    "BYBIT" => Exchange::Bybit,
                    "LOCAL" => Exchange::Local,
                    _ => Exchange::Binance,
                };

                if loader.connect("postgresql://localhost/market_data").await.is_ok() {
                    loader.load_bar_data(&symbol_only, exchange, interval, start, end).await.unwrap_or_default()
                } else {
                    // Fallback to mock data
                    let mut bars = Vec::new();
                    let mut base_price = 40000.0;
                    for i in 0..1000 {
                        let dt = start + chrono::Duration::minutes(
                            match interval {
                                Interval::Minute => 1,
                                Interval::Minute15 => 15,
                                Interval::Hour => 60,
                                Interval::Hour4 => 240,
                                Interval::Daily => 1440,
                                _ => 15,
                            } * i,
                        );
                        if dt > end { break; }
                        let open_price = base_price + (rand::random::<f64>() - 0.5) * 100.0;
                        let high_price = open_price + rand::random::<f64>() * 50.0;
                        let low_price = open_price - rand::random::<f64>() * 50.0;
                        let close_price = low_price + rand::random::<f64>() * (high_price - low_price);
                        base_price = close_price;
                        bars.push(crate::trader::BarData {
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
                        });
                    }
                    bars
                }
            });

            if history_data.is_empty() {
                *error_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some("无法加载历史数据".to_string());
                return;
            }

            opt_engine.set_history_data(history_data);

            // Create strategy factory
            #[cfg(feature = "python")]
            let factory = {
                let sf = strategy_file.clone();
                let sc = strategy_class.clone();
                let sn = strategy_name.clone();
                let vt_sym = vt_symbol.clone();
                move |params: &std::collections::HashMap<String, f64>| -> Box<dyn crate::strategy::StrategyTemplate> {
                    pyo3::Python::initialize();
                    if let Err(e) = crate::python::setup_embedded_python_path() {
                        tracing::error!("Failed to setup Python path: {e}");
                    }

                    // Build a PyDict with the parameter values
                    let setting = pyo3::Python::attach(|py| {
                        let dict = pyo3::types::PyDict::new(py);
                        for (key, val) in params {
                            pyo3::types::PyDictMethods::set_item(&dict, key.as_str(), *val).ok();
                        }
                        Some(dict.unbind())
                    });

                    match crate::python::PythonStrategyAdapter::load_from_file(
                        &sf,
                        &sc,
                        sn.clone(),
                        vec![vt_sym.clone()],
                        setting,
                    ) {
                        Ok(adapter) => Box::new(adapter),
                        Err(e) => {
                            tracing::error!("Failed to load strategy in optimization: {}", e);
                            // Create a minimal no-op adapter as fallback
                            // We cannot proceed without a valid strategy, so log and return
                            // the best-effort adapter loaded without setting
                            match crate::python::PythonStrategyAdapter::load_from_file(
                                &sf,
                                &sc,
                                sn.clone(),
                                vec![vt_sym.clone()],
                                None,
                            ) {
                                Ok(adapter) => Box::new(adapter),
                                Err(e2) => {
                                    tracing::error!("Failed to load strategy without setting: {}", e2);
                                    panic!("Strategy factory failed — cannot continue optimization");
                                }
                            }
                        }
                    }
                }
            };

            #[cfg(not(feature = "python"))]
            let factory = |_params: &std::collections::HashMap<String, f64>| -> Box<dyn crate::strategy::StrategyTemplate> {
                // Without Python support, optimization is not functional
                // This path should not be reached in practice
                unimplemented!("Optimization requires the 'python' feature to be enabled")
            };

            let target = opt_target.to_optimization_target();

            // Run grid search
            let results = opt_engine.run_grid_search(factory, target);

            if results.is_empty() {
                *error_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some("优化未产生有效结果".to_string());
                return;
            }

            let best = &results[0];
            let best_params = best.parameters.clone();
            let best_target_value = best.target_value;
            let best_statistics = best.statistics.clone();
            let all_results_count = results.len();

            // Build heatmap if exactly 2 parameters
            let heatmap_data = if param_names.len() == 2 && all_results_count > 0 {
                build_heatmap_from_results(&results, &param_names)
            } else {
                None
            };

            let display = OptimizationResultsDisplay {
                best_params,
                best_target_value,
                best_statistics,
                heatmap_data,
                all_results_count,
            };

            *result_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(display);
        });
    }

    /// Check for optimization results from background thread
    fn check_optimization_results(&mut self) {
        // Check for errors
        if let Ok(mut flag) = self.opt_error_flag.lock() {
            if let Some(err) = flag.take() {
                self.opt_is_running = false;
                self.opt_error = Some(err);
                return;
            }
        }

        // Check for results
        if self.opt_is_running {
            if let Ok(mut flag) = self.opt_result_flag.lock() {
                if let Some(results) = flag.take() {
                    self.opt_is_running = false;
                    self.opt_progress = 1.0;
                    self.opt_results = Some(results);
                }
            }
        }
    }

    /// Apply optimal parameters back to the backtest panel fields
    fn apply_optimal_to_backtest(&mut self) {
        let Some(ref results) = self.opt_results else {
            return;
        };

        // Try to map common parameter names to the backtest fields
        for (name, value) in &results.best_params {
            match name.as_str() {
                "fast_window" | "fast" => {
                    self.fast_window = format!("{}", value.round() as i64);
                }
                "slow_window" | "slow" => {
                    self.slow_window = format!("{}", value.round() as i64);
                }
                "fixed_size" | "size" => {
                    self.fixed_size = format!("{}", value);
                }
                _ => {}
            }
        }

        // Switch to backtest tab to show the applied parameters
        self.active_tab = BacktestingTab::Backtest;
    }

    /// Deploy optimized strategy to paper trading
    fn deploy_optimized_to_paper(&mut self) {
        let Some(ref ws) = self.workflow_state else {
            return;
        };

        let config = StrategyDeployConfig {
            strategy_name: self.strategy_name.clone(),
            strategy_class: self.strategy_class.clone(),
            vt_symbol: self.vt_symbol.clone(),
            interval: self.interval,
            rate: self.rate.parse::<f64>().unwrap_or(0.0003),
            slippage: self.slippage.parse::<f64>().unwrap_or(0.0001),
            capital: self.capital.parse::<f64>().unwrap_or(100000.0),
            mode: DeployMode::Paper,
            strategy_setting: StrategySetting::default(),
        };

        ws.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_action(WorkflowAction::DeployToLive(config));

        self.status_message = "正在部署优化策略...".to_string();
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
                self.start_date_picker.show(ui, "", ui.auto_id_with("start_date_popup"));
                ui.end_row();

                ui.label("结束时间:");
                self.end_date_picker.show(ui, "", ui.auto_id_with("end_date_popup"));
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
                        egui::ComboBox::from_id_salt("strategy_selector")
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
                                        // Auto-generate strategy name from class name
                                        self.strategy_name = class_name.clone();
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

    /// Render fill model configuration section
    fn render_fill_model_config(&mut self, ui: &mut Ui) {
        ui.heading("填充模型");
        ui.add_space(4.0);

        // Fill model selector
        Grid::new("fill_model_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label("模型类型:");
                egui::ComboBox::from_id_salt("fill_model_selector")
                    .selected_text(self.fill_model_type.label())
                    .show_ui(ui, |ui| {
                        for model_type in FillModelType::ALL {
                            ui.selectable_value(
                                &mut self.fill_model_type,
                                model_type,
                                model_type.label(),
                            );
                        }
                    });
                ui.end_row();
            });

        // Model description
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ℹ").size(14.0));
            ui.label(
                egui::RichText::new(self.fill_model_type.description())
                    .small()
                    .color(ui.style().visuals.weak_text_color()),
            );
        });
        ui.add_space(8.0);

        // Model-specific parameters
        match self.fill_model_type {
            FillModelType::BestPrice => {
                // BestPrice uses the general slippage from config - no extra params
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("此模型使用上方配置的滑点参数")
                            .small()
                            .color(ui.style().visuals.weak_text_color()),
                    );
                });
            }
            FillModelType::Ideal => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("此模型无额外参数，零滑点理想成交")
                            .small()
                            .color(ui.style().visuals.weak_text_color()),
                    );
                });
            }
            FillModelType::TwoTier => {
                Grid::new("two_tier_params_grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("基础滑点:");
                        ui.add(egui::TextEdit::singleline(&mut self.two_tier_slippage_base).desired_width(100.0));
                        ui.end_row();

                        ui.label("大单额外滑点:");
                        ui.add(egui::TextEdit::singleline(&mut self.two_tier_slippage_extra).desired_width(100.0));
                        ui.end_row();

                        ui.label("大单阈值:");
                        ui.add(egui::TextEdit::singleline(&mut self.two_tier_size_threshold).desired_width(100.0));
                        ui.end_row();

                        ui.label("小单成交概率:");
                        ui.add(egui::TextEdit::singleline(&mut self.two_tier_prob_base).desired_width(100.0));
                        ui.end_row();

                        ui.label("大单成交概率:");
                        ui.add(egui::TextEdit::singleline(&mut self.two_tier_prob_large).desired_width(100.0));
                        ui.end_row();
                    });
            }
            FillModelType::SizeAware => {
                Grid::new("size_aware_params_grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("基础滑点:");
                        ui.add(egui::TextEdit::singleline(&mut self.size_aware_base_slippage).desired_width(100.0));
                        ui.end_row();

                        ui.label("最大滑点:");
                        ui.add(egui::TextEdit::singleline(&mut self.size_aware_max_slippage).desired_width(100.0));
                        ui.end_row();

                        ui.label("冲击系数:");
                        ui.add(egui::TextEdit::singleline(&mut self.size_aware_impact_coefficient).desired_width(100.0));
                        ui.end_row();

                        ui.label("最大成交比例:");
                        ui.add(egui::TextEdit::singleline(&mut self.size_aware_max_fill_pct).desired_width(100.0));
                        ui.end_row();
                    });
            }
            FillModelType::Probabilistic => {
                Grid::new("prob_params_grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("滑点:");
                        ui.add(egui::TextEdit::singleline(&mut self.prob_slippage).desired_width(100.0));
                        ui.end_row();

                        ui.label("限价单成交概率:");
                        ui.add(egui::TextEdit::singleline(&mut self.prob_fill_on_limit).desired_width(100.0));
                        ui.end_row();

                        ui.label("滑点出现概率:");
                        ui.add(egui::TextEdit::singleline(&mut self.prob_slippage_probability).desired_width(100.0));
                        ui.end_row();
                    });
            }
        }
    }

    /// Render configuration summary box
    fn render_config_summary(&mut self, ui: &mut Ui) {
        ui.heading("配置概要");
        ui.add_space(4.0);

        egui::Frame::group(ui.style())
            .inner_margin(8.0)
            .show(ui, |ui| {
                Grid::new("config_summary_grid")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("交易品种:").strong());
                        ui.label(&self.vt_symbol);
                        ui.end_row();

                        ui.label(egui::RichText::new("K线周期:").strong());
                        ui.label(format!("{:?}", self.interval));
                        ui.end_row();

                        ui.label(egui::RichText::new("回测模式:").strong());
                        ui.label(match self.mode {
                            BacktestingMode::Bar => "Bar回测",
                            BacktestingMode::Tick => "Tick回测",
                        });
                        ui.end_row();

                        ui.label(egui::RichText::new("填充模型:").strong());
                        ui.label(self.fill_model_type.label());
                        ui.end_row();

                        ui.label(egui::RichText::new("预估成交率:").strong());
                        ui.label(self.fill_model_type.estimated_fill_rate());
                        ui.end_row();

                        ui.label(egui::RichText::new("手续费率:").strong());
                        ui.label(&self.rate);
                        ui.end_row();

                        ui.label(egui::RichText::new("滑点:").strong());
                        ui.label(&self.slippage);
                        ui.end_row();

                        ui.label(egui::RichText::new("初始资金:").strong());
                        ui.label(&self.capital);
                        ui.end_row();

                        // Model-specific summary
                        match self.fill_model_type {
                            FillModelType::TwoTier => {
                                ui.label(egui::RichText::new("双层滑点:").strong());
                                ui.label(format!(
                                    "基础={} 大单额外={} 阈值={}",
                                    self.two_tier_slippage_base,
                                    self.two_tier_slippage_extra,
                                    self.two_tier_size_threshold,
                                ));
                                ui.end_row();

                                ui.label(egui::RichText::new("成交概率:").strong());
                                ui.label(format!(
                                    "小单={} 大单={}",
                                    self.two_tier_prob_base,
                                    self.two_tier_prob_large,
                                ));
                                ui.end_row();
                            }
                            FillModelType::SizeAware => {
                                ui.label(egui::RichText::new("冲击参数:").strong());
                                ui.label(format!(
                                    "基础滑点={} 最大滑点={} 冲击系数={}",
                                    self.size_aware_base_slippage,
                                    self.size_aware_max_slippage,
                                    self.size_aware_impact_coefficient,
                                ));
                                ui.end_row();

                                ui.label(egui::RichText::new("最大成交:").strong());
                                ui.label(format!("{}%", 
                                    self.size_aware_max_fill_pct.parse::<f64>()
                                        .unwrap_or(0.5) * 100.0
                                ));
                                ui.end_row();
                            }
                            FillModelType::Probabilistic => {
                                ui.label(egui::RichText::new("概率参数:").strong());
                                ui.label(format!(
                                    "成交概率={} 滑点概率={}",
                                    self.prob_fill_on_limit,
                                    self.prob_slippage_probability,
                                ));
                                ui.end_row();
                            }
                            _ => {}
                        }
                    });
            });

        ui.add_space(6.0);

        // Reset to defaults button
        ui.horizontal(|ui| {
            if ui.button("恢复默认配置").clicked() {
                self.reset_to_defaults();
            }
        });
    }

    /// Reset all configuration to defaults
    fn reset_to_defaults(&mut self) {
        self.rate = "0.0003".to_string();
        self.slippage = "0.0001".to_string();
        self.capital = "100000.0".to_string();
        self.mode = BacktestingMode::Bar;
        self.fill_model_type = FillModelType::default();
        self.two_tier_slippage_base = "0.1".to_string();
        self.two_tier_slippage_extra = "0.2".to_string();
        self.two_tier_size_threshold = "100.0".to_string();
        self.two_tier_prob_base = "1.0".to_string();
        self.two_tier_prob_large = "0.8".to_string();
        self.size_aware_base_slippage = "0.1".to_string();
        self.size_aware_max_slippage = "1.0".to_string();
        self.size_aware_impact_coefficient = "0.5".to_string();
        self.size_aware_max_fill_pct = "0.5".to_string();
        self.prob_slippage = "0.2".to_string();
        self.prob_fill_on_limit = "0.9".to_string();
        self.prob_slippage_probability = "0.5".to_string();
        self.fast_window = "10".to_string();
        self.slow_window = "20".to_string();
        self.fixed_size = "1.0".to_string();
        self.status_message = "已恢复默认配置".to_string();
    }

    /// Build a fill model from the current UI configuration
    fn build_fill_model(&self) -> Box<dyn FillModel> {
        let slippage = self.slippage.parse::<f64>().unwrap_or(0.0001);

        match self.fill_model_type {
            FillModelType::BestPrice => {
                Box::new(BestPriceFillModel::new(slippage))
            }
            FillModelType::Ideal => {
                Box::new(IdealFillModel::new())
            }
            FillModelType::TwoTier => {
                Box::new(TwoTierFillModel::new(
                    self.two_tier_slippage_base.parse::<f64>().unwrap_or(0.1),
                    self.two_tier_slippage_extra.parse::<f64>().unwrap_or(0.2),
                    self.two_tier_size_threshold.parse::<f64>().unwrap_or(100.0),
                    self.two_tier_prob_base.parse::<f64>().unwrap_or(1.0),
                    self.two_tier_prob_large.parse::<f64>().unwrap_or(0.8),
                ))
            }
            FillModelType::SizeAware => {
                Box::new(SizeAwareFillModel::new(
                    self.size_aware_base_slippage.parse::<f64>().unwrap_or(0.1),
                    self.size_aware_max_slippage.parse::<f64>().unwrap_or(1.0),
                    self.size_aware_impact_coefficient.parse::<f64>().unwrap_or(0.5),
                    self.size_aware_max_fill_pct.parse::<f64>().unwrap_or(0.5),
                ))
            }
            FillModelType::Probabilistic => {
                Box::new(ProbabilisticFillModel::new(
                    self.prob_slippage.parse::<f64>().unwrap_or(0.2),
                    self.prob_fill_on_limit.parse::<f64>().unwrap_or(0.9),
                    self.prob_slippage_probability.parse::<f64>().unwrap_or(0.5),
                ))
            }
        }
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
        
        // Deploy button (only when results are available)
        if self.results.is_some() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let deploy_button = ui.add(
                    egui::Button::new(
                        egui::RichText::new("🚀 一键部署到模拟交易").color(egui::Color32::WHITE)
                    )
                    .fill(egui::Color32::from_rgb(50, 130, 220))
                );
                if deploy_button.clicked() {
                    self.deploy_to_paper();
                }
            });
        }
    }

    /// Render status section
    fn render_status(&mut self, ui: &mut Ui) {
        ui.heading("运行状态");

        ui.horizontal(|ui| {
            ui.label("状态:");
            ui.label(&self.status_message);
        });
        
        // Show error message if backtest failed
        if let Some(ref error) = self.backtest_error {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), format!("❌ {}", error));
        }

        if self.is_running {
            ui.add(egui::ProgressBar::new(self.progress).show_percentage());
        }
    }

    /// Render results section
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // value fits in target type; usize-to-f64 cast acceptable for practical counts
    fn render_results(&mut self, ui: &mut Ui) {
        ui.heading("回测结果");

        // Mock data warning banner
        if self.using_mock_data {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::RED, "⚠️ 警告: 使用随机模拟数据回测，结果无参考价值！");
            ui.label("请配置PostgreSQL数据库或加载CSV/Parquet文件以使用真实历史数据。");
            ui.add_space(8.0);
        }

        if let Some(ref stats) = self.results {
            Grid::new("backtest_results_grid")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label("开始日期:");
                    ui.label(stats.start_date.to_string());
                    ui.end_row();

                    ui.label("结束日期:");
                    ui.label(stats.end_date.to_string());
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

            // Equity curve and drawdown charts
            if !self.daily_pnl.is_empty() {
                // --- 净值曲线 ---
                ui.heading("净值曲线");
                let available_width = ui.available_width();
                let chart_height = 200.0;
                let response = ui.allocate_response(
                    Vec2::new(available_width, chart_height),
                    egui::Sense::hover(),
                );
                let rect = response.rect;

                // Compute cumulative equity from daily PnL
                let mut equity_curve: Vec<f64> = Vec::new();
                let mut cumulative = 0.0;
                for (_, pnl) in &self.daily_pnl {
                    cumulative += pnl;
                    equity_curve.push(cumulative);
                }

                let painter = ui.painter_at(rect);
                {
                    // Background
                    painter.rect_filled(rect, 2.0, Color32::from_rgb(30, 30, 30));
                    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Inside);

                    if equity_curve.len() > 1 {
                        let min_val = equity_curve.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max_val = equity_curve.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        let range = (max_val - min_val).max(1.0);

                        let padding = 5.0;
                        let chart_rect = Rect::from_min_max(
                            Pos2::new(rect.left() + padding, rect.top() + padding),
                            Pos2::new(rect.right() - padding, rect.bottom() - padding),
                        );

                        // Zero line
                        if min_val < 0.0 && max_val > 0.0 {
                            let zero_y = chart_rect.bottom()
                                - ((0.0 - min_val) / range) as f32 * chart_rect.height();
                            painter.line_segment(
                                [
                                    Pos2::new(chart_rect.left(), zero_y),
                                    Pos2::new(chart_rect.right(), zero_y),
                                ],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
                            );
                        }

                        // Equity curve line
                        let points: Vec<Pos2> = equity_curve
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                let x = chart_rect.left()
                                    + (i as f32 / (equity_curve.len() - 1) as f32)
                                        * chart_rect.width();
                                let y = chart_rect.bottom()
                                    - ((v - min_val) / range) as f32 * chart_rect.height();
                                Pos2::new(x, y)
                            })
                            .collect();

                        if points.len() > 1 {
                            let line_color =
                                if equity_curve.last().copied().unwrap_or(0.0) >= 0.0 {
                                    Color32::from_rgb(255, 80, 80) // 红色=盈利
                                } else {
                                    Color32::from_rgb(80, 200, 80) // 绿色=亏损
                                };
                            painter.add(egui::Shape::line(points, Stroke::new(1.5, line_color)));
                        }

                        // Labels
                        painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.top()),
                            egui::Align2::LEFT_TOP,
                            format!("最高: {max_val:.2}"),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                        painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.bottom()),
                            egui::Align2::LEFT_BOTTOM,
                            format!("最低: {min_val:.2}"),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                    }
                }

                ui.add_space(10.0);

                // --- 回撤曲线 ---
                ui.heading("回撤曲线");
                let dd_response = ui.allocate_response(
                    Vec2::new(available_width, chart_height),
                    egui::Sense::hover(),
                );
                let dd_rect = dd_response.rect;

                // Compute drawdown from equity curve
                let mut drawdown_curve: Vec<f64> = Vec::new();
                let mut peak = 0.0_f64;
                for &val in &equity_curve {
                    peak = peak.max(val);
                    let dd = if peak > 0.0 { (val - peak) / peak } else { 0.0 };
                    drawdown_curve.push(dd * 100.0); // as percentage
                }

                let dd_painter = ui.painter_at(dd_rect);
                {
                    // Background
                    dd_painter.rect_filled(dd_rect, 2.0, Color32::from_rgb(30, 30, 30));
                    dd_painter.rect_stroke(
                        dd_rect,
                        2.0,
                        Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
                        egui::StrokeKind::Inside,
                    );

                    if drawdown_curve.len() > 1 {
                        let min_dd = drawdown_curve.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max_dd = drawdown_curve.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        // Range from min_dd (most negative) to 0
                        let dd_max = 0.0_f64.max(max_dd);
                        let dd_min = min_dd.min(0.0);
                        let dd_range = (dd_max - dd_min).max(0.01);

                        let padding = 5.0;
                        let chart_rect = Rect::from_min_max(
                            Pos2::new(dd_rect.left() + padding, dd_rect.top() + padding),
                            Pos2::new(dd_rect.right() - padding, dd_rect.bottom() - padding),
                        );

                        // Zero line (top area since drawdowns are negative)
                        let zero_y = chart_rect.bottom()
                            - ((dd_max - dd_min) / dd_range) as f32 * chart_rect.height();
                        if zero_y >= chart_rect.top() && zero_y <= chart_rect.bottom() {
                            dd_painter.line_segment(
                                [
                                    Pos2::new(chart_rect.left(), zero_y),
                                    Pos2::new(chart_rect.right(), zero_y),
                                ],
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
                            );
                        }

                        // Drawdown curve
                        let points: Vec<Pos2> = drawdown_curve
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                let x = chart_rect.left()
                                    + (i as f32 / (drawdown_curve.len() - 1) as f32)
                                        * chart_rect.width();
                                let y = chart_rect.bottom()
                                    - ((v - dd_min) / dd_range) as f32 * chart_rect.height();
                                Pos2::new(x, y)
                            })
                            .collect();

                        if points.len() > 1 {
                            dd_painter.add(egui::Shape::line(
                                points,
                                Stroke::new(1.5, Color32::from_rgb(80, 200, 80)),
                            ));
                        }

                        // Labels
                        dd_painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.top()),
                            egui::Align2::LEFT_TOP,
                            "0.00%".to_string(),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                        dd_painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.bottom()),
                            egui::Align2::LEFT_BOTTOM,
                            format!("最大回撤: {min_dd:.2}%"),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                    }
                }
            }
        }
    }

    /// Start backtesting
    fn start_backtesting(&mut self) {
        // Input validation
        if self.vt_symbol.trim().is_empty() {
            self.status_message = "错误: 交易品种不能为空".to_string();
            self.backtest_error = Some("交易品种不能为空".to_string());
            return;
        }
        let capital = self.capital.parse::<f64>().unwrap_or(0.0);
        if capital <= 0.0 {
            self.status_message = "错误: 初始资金必须大于0".to_string();
            self.backtest_error = Some("初始资金必须大于0".to_string());
            return;
        }
        // Validate date range
        let start_str = self.start_date_picker.to_datetime_string();
        let end_str = self.end_date_picker.to_end_datetime_string();
        let start = NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S");
        let end = NaiveDateTime::parse_from_str(&end_str, "%Y-%m-%d %H:%M:%S");
        match (start, end) {
            (Ok(s), Ok(e)) if s >= e => {
                self.status_message = "错误: 开始时间必须早于结束时间".to_string();
                self.backtest_error = Some("开始时间必须早于结束时间".to_string());
                return;
            }
            (Err(_), _) | (_, Err(_)) => {
                self.status_message = "错误: 日期格式无效".to_string();
                self.backtest_error = Some("日期格式无效".to_string());
                return;
            }
            _ => {}
        }
        
        // Clear any previous error
        self.backtest_error = None;
        
        self.is_running = true;
        self.progress = 0.0;
        self.status_message = "正在初始化...".to_string();

        // Parse parameters
        let rate = self.rate.parse::<f64>().unwrap_or(0.0003);
        let slippage = self.slippage.parse::<f64>().unwrap_or(0.0001);
        let capital = self.capital.parse::<f64>().unwrap_or(100000.0);
        let vt_symbol = self.vt_symbol.clone();
        let interval = self.interval;
        let mode = self.mode;

        // Build fill model from UI configuration
        let fill_model = self.build_fill_model();

        // Parse dates
        let start_str = self.start_date_picker.to_datetime_string();
        let end_str = self.end_date_picker.to_end_datetime_string();

        let start = NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());

        let end = NaiveDateTime::parse_from_str(&end_str, "%Y-%m-%d %H:%M:%S")
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or(Utc::now());

        // Strategy info
        #[cfg(not(feature = "python"))]
        let _ = (&self.strategy_file, &self.strategy_class, &self.strategy_name);
        #[cfg(feature = "python")]
        let (strategy_file, strategy_class, strategy_name) = (
            self.strategy_file.clone(),
            self.strategy_class.clone(),
            self.strategy_name.clone(),
        );

        let engine_arc = self.engine.clone();
        let mock_data_flag = self.using_mock_data_flag.clone();
        let error_flag = self.backtest_error_flag.clone();

        // Spawn thread
        thread::spawn(move || {
            // Create runtime
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for backtesting panel");

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

                // Apply fill model from UI configuration
                engine.set_fill_model(fill_model);

                // Load data from database using DatabaseLoader
                use crate::backtesting::database::DatabaseLoader;

                let mut loader = DatabaseLoader::new();

                // Parse symbol and exchange from vt_symbol
                let parts: Vec<&str> = vt_symbol.split('.').collect();
                let symbol_only = parts.first().unwrap_or(&"BTCUSDT").to_string();
                let exchange_str = parts.get(1).unwrap_or(&"BINANCE");

                // Parse exchange from string
                let exchange = match exchange_str.to_uppercase().as_str() {
                    "BINANCE" => Exchange::Binance,
                    "BINANCE_USDM" => Exchange::BinanceUsdm,
                    "BINANCE_COINM" => Exchange::BinanceCoinm,
                    "OKX" => Exchange::Okx,
                    "BYBIT" => Exchange::Bybit,
                    "LOCAL" => Exchange::Local,
                    other => {
                        tracing::warn!("Unknown exchange '{}', defaulting to Binance", other);
                        Exchange::Binance
                    }
                };

                // Connect to database (PostgreSQL)
                let db_url = "postgresql://localhost/market_data";
                if let Err(_e) = loader.connect(db_url).await {
                    // If PostgreSQL fails, database feature might not be enabled
                    // Fall back to generating mock data
                    tracing::warn!("数据库不可用，使用随机模拟数据 - 回测结果无参考价值");

                    // Set mock data flag
                    if let Ok(mut flag) = mock_data_flag.lock() {
                        *flag = true;
                    }

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
                            // Real data loaded - ensure mock flag is false
                            if let Ok(mut flag) = mock_data_flag.lock() {
                                *flag = false;
                            }
                            if bars.is_empty() {
                                tracing::warn!(
                                    "No data found in database for {}.{}",
                                    symbol_only, exchange_str
                                );
                            } else {
                                tracing::info!("Loaded {} bars from database", bars.len());
                            }
                            engine.set_history_data(bars);
                        }
                        Err(e) => {
                            tracing::error!("Failed to load data from database: {}", e);
                            *error_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("数据库加载失败: {}", e));
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
                    pyo3::Python::initialize();

                    // Setup sys.path so the embedded interpreter can find
                    // trade_engine module and strategy files
                    if let Err(e) = crate::python::setup_embedded_python_path() {
                        eprintln!("Failed to setup Python path: {e}");
                    }

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
                            *error_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("策略加载失败: {}", e));
                            return;
                        }
                    }
                }

                // Run backtesting
                if let Err(e) = engine.run_backtesting().await {
                    *error_flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("回测运行失败: {}", e));
                    return;
                }

                // Calculate statistics
                engine.calculate_statistics(false);

                *engine_arc.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(engine);
            });
        });
    }

    /// Check for results (call this in `ui`() loop)
    #[allow(clippy::cast_precision_loss)] // usize-to-f64 cast acceptable for practical counts
    fn check_results(&mut self) {
        // Check for background thread errors first
        if let Ok(mut error_flag) = self.backtest_error_flag.lock() {
            if let Some(error) = error_flag.take() {
                self.is_running = false;
                self.status_message = format!("回测失败: {}", error);
                self.backtest_error = Some(error);
                return;
            }
        }
        
        // Poll the engine for results if we are running
        if self.is_running {
            let engine_guard = self.engine.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(engine) = engine_guard.as_ref() {
                let logs = engine.get_logs();
                if !logs.is_empty() && logs.last().expect("logs is non-empty").contains("回测运行结束") {
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

                    // Populate trade overlay from backtest trades
                    let trades = engine.get_all_trades();
                    self.trade_overlay = TradeOverlay::from_trades(&trades);
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
        self.trade_overlay.clear();
        self.status_message = "就绪".to_string();
        self.progress = 0.0;
        self.backtest_error = None;
    }

    /// Get the trade overlay for chart visualization
    pub fn get_trade_overlay(&self) -> &TradeOverlay {
        &self.trade_overlay
    }

    /// Take the trade overlay, replacing it with an empty one
    pub fn take_trade_overlay(&mut self) -> TradeOverlay {
        std::mem::take(&mut self.trade_overlay)
    }

    /// Get the `vt_symbol` used for the backtest
    pub fn get_vt_symbol(&self) -> &str {
        &self.vt_symbol
    }

    /// Check if backtest results are available
    pub fn has_results(&self) -> bool {
        self.results.is_some()
    }

    /// Get a reference to the backtest results, if any
    pub fn get_results(&self) -> Option<&BacktestingStatistics> {
        self.results.as_ref()
    }

    /// Get the current vt_symbol for workflow integration
    pub fn get_strategy_name(&self) -> &str {
        &self.strategy_name
    }

    /// Get the current strategy class name for workflow integration
    pub fn get_strategy_class(&self) -> &str {
        &self.strategy_class
    }

    /// Get the current interval for workflow integration
    pub fn get_interval(&self) -> Interval {
        self.interval
    }

    /// Get the parsed rate value
    pub fn get_rate(&self) -> f64 {
        self.rate.parse::<f64>().unwrap_or(0.0003)
    }

    /// Get the parsed slippage value
    pub fn get_slippage(&self) -> f64 {
        self.slippage.parse::<f64>().unwrap_or(0.0001)
    }

    /// Get the parsed capital value
    pub fn get_capital(&self) -> f64 {
        self.capital.parse::<f64>().unwrap_or(100000.0)
    }

    /// Trigger a backtest start (for F5 shortcut)
    pub fn start_backtesting_external(&mut self) {
        if !self.is_running {
            self.start_backtesting();
        }
    }

    /// Set the vt_symbol for workflow integration (from other panels)
    pub fn set_vt_symbol(&mut self, symbol: &str) {
        self.vt_symbol = symbol.to_string();
    }

    /// Deploy backtest results to paper trading via workflow state
    pub fn deploy_to_paper(&mut self) {
        let Some(ref ws) = self.workflow_state else {
            return;
        };
        if self.results.is_none() {
            return;
        }
        
        let config = StrategyDeployConfig {
            strategy_name: self.strategy_name.clone(),
            strategy_class: self.strategy_class.clone(),
            vt_symbol: self.vt_symbol.clone(),
            interval: self.interval,
            rate: self.rate.parse::<f64>().unwrap_or(0.0003),
            slippage: self.slippage.parse::<f64>().unwrap_or(0.0001),
            capital: self.capital.parse::<f64>().unwrap_or(100000.0),
            mode: DeployMode::Paper,
            strategy_setting: StrategySetting::default(),
        };
        
        ws.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_action(WorkflowAction::DeployToLive(config));
        
        self.status_message = "正在部署策略...".to_string();
    }

    /// Export results to CSV/JSON file
    fn export_results(&self) {
        let Some(ref stats) = self.results else {
            return;
        };

        #[cfg(feature = "gui")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV文件", &["csv"])
                .add_filter("JSON文件", &["json"])
                .set_title("导出回测结果")
                .save_file()
            {
                let path_str = path.to_string_lossy().to_string();
                if path_str.ends_with(".json") {
                    match serde_json::to_string_pretty(stats) {
                        Ok(json) => {
                            if let Err(e) = std::fs::write(&path, json) {
                                tracing::error!("导出JSON失败: {}", e);
                            }
                        }
                        Err(e) => tracing::error!("序列化JSON失败: {}", e),
                    }
                } else {
                    let csv = format!(
                        "指标,值\n\
                         起始日期,{}\n\
                         结束日期,{}\n\
                         总交易日,{}\n\
                         盈利天数,{}\n\
                         亏损天数,{}\n\
                         期末余额,{:.2}\n\
                         最大回,{:.2}\n\
                         最大回撤百分比,{:.2}%\n\
                         总净盈亏,{:.2}\n\
                         总手续费,{:.2}\n\
                         总滑点,{:.2}\n\
                         总成交额,{:.2}\n\
                         总交易次数,{}\n\
                         日均净盈亏,{:.2}\n\
                         日均手续费,{:.2}\n\
                         日均滑点,{:.2}\n\
                         日均成交额,{:.2}\n\
                         日均交易次数,{:.2}\n\
                         日收益率,{:.4}\n\
                         收益率标准差,{:.4}\n\
                         夏普比率,{:.4}\n\
                         年化收益,{:.4}",
                        stats.start_date,
                        stats.end_date,
                        stats.total_days,
                        stats.profit_days,
                        stats.loss_days,
                        stats.end_balance,
                        stats.max_drawdown,
                        stats.max_drawdown_percent,
                        stats.total_net_pnl,
                        stats.total_commission,
                        stats.total_slippage,
                        stats.total_turnover,
                        stats.total_trade_count,
                        stats.daily_net_pnl,
                        stats.daily_commission,
                        stats.daily_slippage,
                        stats.daily_turnover,
                        stats.daily_trade_count,
                        stats.daily_return,
                        stats.return_std,
                        stats.sharpe_ratio,
                        stats.return_mean,
                    );
                    if let Err(e) = std::fs::write(&path, csv) {
                        tracing::error!("导出CSV失败: {}", e);
                    }
                }

                // Also export daily PnL data if available
                if !self.daily_pnl.is_empty() {
                    let pnl_path = if path_str.ends_with(".json") {
                        path.with_extension("pnl.json")
                    } else {
                        path.with_extension("pnl.csv")
                    };
                    let pnl_csv = {
                        let mut lines = String::from("day_index,pnl\n");
                        for (idx, pnl) in &self.daily_pnl {
                            lines.push_str(&format!("{idx},{pnl:.6}\n"));
                        }
                        lines
                    };
                    if let Err(e) = std::fs::write(&pnl_path, pnl_csv) {
                        tracing::error!("导出日盈亏数据失败: {}", e);
                    }
                }
            }
        }

        #[cfg(not(feature = "gui"))]
        {
            let csv = format!(
                "start_date,end_date,total_days,profit_days,loss_days,\
                 end_balance,max_drawdown,max_drawdown_percent,\
                 total_net_pnl,total_commission,total_slippage,total_turnover,total_trade_count,\
                 daily_net_pnl,daily_commission,daily_slippage,daily_turnover,daily_trade_count,\
                 daily_return,return_std,sharpe_ratio,return_mean\n\
                 {},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.4},{:.4},{:.4},{:.4}",
                stats.start_date,
                stats.end_date,
                stats.total_days,
                stats.profit_days,
                stats.loss_days,
                stats.end_balance,
                stats.max_drawdown,
                stats.max_drawdown_percent,
                stats.total_net_pnl,
                stats.total_commission,
                stats.total_slippage,
                stats.total_turnover,
                stats.total_trade_count,
                stats.daily_net_pnl,
                stats.daily_commission,
                stats.daily_slippage,
                stats.daily_turnover,
                stats.daily_trade_count,
                stats.daily_return,
                stats.return_std,
                stats.sharpe_ratio,
                stats.return_mean,
            );
            println!("{}", csv);
        }
    }

    /// Browse for strategy file
    #[allow(dead_code)]
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
                    self.status_message = format!("已选择: {path_str}");

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
                // Default directories to try (project-relative paths)
                let default_dirs = [
                    "./strategies",      // Project strategies directory (migrated from vnpy)
                    "./examples",        // Example strategies
                ];

                // Find first existing directory
                default_dirs
                    .iter()
                    .find(|d| Path::new(d).is_dir())
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| "./strategies".to_string())
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
                    self.status_message = format!("扫描失败: {e}");
                }
            }
        }

        #[cfg(not(feature = "python"))]
        {
            self.status_message = "Python功能未启用".to_string();
        }
    }
}

/// Interpolate heatmap color: red (t=0, worst) → yellow (t=0.5) → green (t=1, best)
fn heatmap_color(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.5 {
        // Red to Yellow
        let s = t * 2.0;
        (220, (s * 200.0) as u8, 30)
    } else {
        // Yellow to Green
        let s = (t - 0.5) * 2.0;
        ((220.0 - s * 180.0) as u8, 200, 30)
    };
    Color32::from_rgb(r, g, b)
}

/// Build heatmap data from optimization results for exactly 2 parameters
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn build_heatmap_from_results(
    results: &[crate::backtesting::OptimizationResult],
    param_names: &[String],
) -> Option<HeatmapData> {
    if param_names.len() != 2 || results.is_empty() {
        return None;
    }

    let x_name = param_names[0].clone();
    let y_name = param_names[1].clone();

    // Collect unique x and y values
    let mut x_set = std::collections::BTreeSet::new();
    let mut y_set = std::collections::BTreeSet::new();

    for result in results {
        if let Some(&xv) = result.parameters.get(&x_name) {
            x_set.insert((xv * 1e10).round() as i64);
        }
        if let Some(&yv) = result.parameters.get(&y_name) {
            y_set.insert((yv * 1e10).round() as i64);
        }
    }

    let x_values: Vec<f64> = x_set.iter().map(|&v| v as f64 / 1e10).collect();
    let y_values: Vec<f64> = y_set.iter().map(|&v| v as f64 / 1e10).collect();

    if x_values.is_empty() || y_values.is_empty() {
        return None;
    }

    // Build value lookup map
    let mut value_map = std::collections::HashMap::new();
    for result in results {
        if let (Some(&xv), Some(&yv)) = (
            result.parameters.get(&x_name),
            result.parameters.get(&y_name),
        ) {
            let x_key = (xv * 1e10).round() as i64;
            let y_key = (yv * 1e10).round() as i64;
            value_map.insert((x_key, y_key), result.target_value);
        }
    }

    // Build 2D grid: values[y_row][x_col]
    let mut values = Vec::new();
    for &yv in &y_values {
        let y_key = (yv * 1e10).round() as i64;
        let mut row = Vec::new();
        for &xv in &x_values {
            let x_key = (xv * 1e10).round() as i64;
            let val = value_map.get(&(x_key, y_key)).copied().unwrap_or(f64::NAN);
            row.push(val);
        }
        values.push(row);
    }

    Some(HeatmapData {
        x_name,
        y_name,
        x_values,
        y_values,
        values,
    })
}
