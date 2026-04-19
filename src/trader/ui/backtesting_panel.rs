//! Backtesting UI Panel
//!
//! Provides GUI interface for backtesting configuration and result visualization

use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use egui::{Color32, Context, Grid, Id, Pos2, Rect, ScrollArea, Stroke, Ui, Vec2};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::backtesting::{BacktestingEngine, BacktestingMode, BacktestingStatistics};
use crate::chart::TradeOverlay;
use crate::trader::{Exchange, Interval};

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

    /// Create from chrono::NaiveDate
    pub fn from_date(date: chrono::NaiveDate) -> Self {
        Self::new(date.year(), date.month(), date.day())
    }

    /// Show the date picker widget
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
                                                egui::RichText::new(format!("{}", day_counter))
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
                                    .custom_formatter(|n, _| format!("{:.0}", n)),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut self.month)
                                    .speed(0.1)
                                    .range(1..=12)
                                    .custom_formatter(|n, _| format!("{:02.0}", n)),
                            );
                            ui.label("-");
                            ui.add(
                                egui::DragValue::new(&mut self.day)
                                    .speed(0.1)
                                    .range(1..=31)
                                    .custom_formatter(|n, _| format!("{:02.0}", n)),
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

    // Results
    results: Option<BacktestingStatistics>,
    daily_pnl: Vec<(f64, f64)>, // (day_index, pnl)

    // Data source warning
    using_mock_data: bool,
    using_mock_data_flag: Arc<Mutex<bool>>,

    // Trade overlay for chart visualization
    trade_overlay: TradeOverlay,

    // Engine
    engine: Arc<Mutex<Option<BacktestingEngine>>>,
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
            results: None,
            daily_pnl: Vec::new(),
            using_mock_data: false,
            using_mock_data_flag: Arc::new(Mutex::new(false)),
            trade_overlay: TradeOverlay::new(),
            engine: Arc::new(Mutex::new(None)),
        }
    }
}

impl BacktestingPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the panel
    pub fn ui(&mut self, _ctx: &Context, ui: &mut Ui) {
        // Check for background thread results
        self.check_results();

        // Auto-scan strategies on first render if not already scanned
        #[cfg(feature = "python")]
        if !self.strategies_scanned {
            self.scan_strategies_directory();
            self.strategies_scanned = true;
        }

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
                            format!("最高: {:.2}", max_val),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                        painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.bottom()),
                            egui::Align2::LEFT_BOTTOM,
                            format!("最低: {:.2}", min_val),
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
                            format!("0.00%"),
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(160, 160, 160),
                        );
                        dd_painter.text(
                            Pos2::new(chart_rect.left(), chart_rect.bottom()),
                            egui::Align2::LEFT_BOTTOM,
                            format!("最大回撤: {:.2}%", min_dd),
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

        // Clone engine arc to pass to thread
        let engine_arc = self.engine.clone();
        let mock_data_flag = self.using_mock_data_flag.clone();

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
                    "OKEX" | "OKX" | "BYBIT" | "HUOBI" => Exchange::Global,
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
                        eprintln!("Failed to setup Python path: {}", e);
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

                *engine_arc.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);
            });
        });
    }

    /// Check for results (call this in ui() loop)
    fn check_results(&mut self) {
        // Poll the engine for results if we are running
        if self.is_running {
            let engine_guard = self.engine.lock().unwrap_or_else(|e| e.into_inner());
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
    }

    /// Get the trade overlay for chart visualization
    pub fn get_trade_overlay(&self) -> &TradeOverlay {
        &self.trade_overlay
    }

    /// Take the trade overlay, replacing it with an empty one
    pub fn take_trade_overlay(&mut self) -> TradeOverlay {
        std::mem::take(&mut self.trade_overlay)
    }

    /// Get the vt_symbol used for the backtest
    pub fn get_vt_symbol(&self) -> &str {
        &self.vt_symbol
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
                            lines.push_str(&format!("{},{:.6}\n", idx, pnl));
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
                // Default directories to try (project-relative paths)
                let default_dirs = [
                    "./strategies",      // Project strategies directory (migrated from vnpy)
                    "./examples",        // Example strategies
                ];

                // Find first existing directory
                default_dirs
                    .iter()
                    .find(|d| Path::new(d).is_dir())
                    .map(|s| s.to_string())
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
