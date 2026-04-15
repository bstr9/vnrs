//! Chart widget with cursor, zoom, and pan support.

use egui::{Color32, Key, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::fs;

use super::base::{
    calculate_axis_ticks, format_price, format_volume, AXIS_X_HEIGHT, AXIS_Y_WIDTH, CURSOR_COLOR,
    GREY_COLOR, INFO_BOX_HEIGHT, INFO_BOX_WIDTH, MARGIN, MIN_BAR_COUNT, WHITE_COLOR,
};
use super::indicator::{
    validate_expression, CustomIndicator, Indicator, IndicatorLocation, IndicatorType, AVL, BOLL,
    EMA, MA, SAR, SUPER, TRIX, VWAP, WMA,
};
use super::item::{CandleItem, ChartItem, TradeOverlay, VolumeItem};
use super::manager::BarManager;
use crate::trader::object::BarData;
use crate::trader::Interval;

/// Event emitted by ChartWidget
#[derive(Clone, Debug)]
pub struct ChartEvent {
    /// Interval was changed by user
    pub interval_changed: bool,
    /// The new interval
    pub new_interval: Interval,
    /// User scrolled/dragged to the left edge and needs more historical data
    pub need_more_history: bool,
    /// The symbol (vt_symbol) for which data should be requested
    pub symbol: Option<String>,
}

/// Main chart widget
pub struct ChartWidget {
    /// Data manager
    pub manager: BarManager,
    /// Candlestick item
    candle_item: CandleItem,
    /// Volume item
    volume_item: VolumeItem,
    /// Trade overlay
    pub trade_overlay: TradeOverlay,
    /// Cursor state
    cursor: ChartCursor,
    /// Index of the rightmost visible bar
    right_ix: usize,
    /// Number of visible bars
    bar_count: usize,
    /// Price decimal places
    price_decimals: usize,
    /// Show volume chart
    show_volume: bool,
    /// Volume chart height ratio (0.0 - 1.0)
    volume_height_ratio: f32,
    /// Current time interval
    interval: Interval,
    /// Show interval selector
    show_interval_selector: bool,
    /// Show indicator selector
    show_indicator_selector: bool,
    /// Active indicators
    indicators: Vec<Box<dyn Indicator>>,
    /// Auto-scale Y axis
    auto_scale_y: bool,
    /// Show indicator config dialog
    show_indicator_config: bool,
    /// Indicator config state
    indicator_config: IndicatorConfig,
    /// Show time range selector
    show_time_selector: bool,
    /// Time range filter (start, end) - None means show all
    time_range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    /// Show indicator manager panel
    show_indicator_manager: bool,
    /// Custom indicator name input
    custom_indicator_name: String,
    /// Custom indicator expression input
    custom_indicator_expr: String,
    /// Custom indicator error message
    custom_indicator_error: Option<String>,
    /// Whether a history load request is in progress (prevents duplicate requests)
    loading_history: bool,
}

/// Indicator configuration state
#[derive(Clone)]
struct IndicatorConfig {
    indicator_type: IndicatorType,
    period: usize,
    multiplier: f64,
    signal_period: usize,
    color: Color32,
    signal_color: Color32,
    location: IndicatorLocation,
    line_width: f32,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self {
            indicator_type: IndicatorType::MA,
            period: 20,
            multiplier: 2.0,
            signal_period: 9,
            color: Color32::YELLOW,
            signal_color: Color32::from_rgb(255, 100, 0),
            location: IndicatorLocation::Main,
            line_width: 1.5,
        }
    }
}

impl Default for ChartWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartWidget {
    /// Create a new chart widget
    pub fn new() -> Self {
        Self {
            manager: BarManager::new(),
            candle_item: CandleItem::new(),
            volume_item: VolumeItem::new(),
            trade_overlay: TradeOverlay::new(),
            cursor: ChartCursor::new(),
            right_ix: 0,
            bar_count: MIN_BAR_COUNT,
            price_decimals: 4,
            show_volume: true,
            volume_height_ratio: 0.25,
            interval: Interval::Minute,
            show_interval_selector: false,
            show_indicator_selector: false,
            indicators: Vec::new(),
            auto_scale_y: true,
            show_indicator_config: false,
            indicator_config: IndicatorConfig::default(),
            show_time_selector: false,
            time_range: None,
            show_indicator_manager: false,
            custom_indicator_name: String::new(),
            custom_indicator_expr: String::new(),
            custom_indicator_error: None,
            loading_history: false,
        }
    }

    /// Set the price decimal places
    pub fn set_price_decimals(&mut self, decimals: usize) {
        self.price_decimals = decimals;
    }

    /// Set whether to show volume chart
    pub fn set_show_volume(&mut self, show: bool) {
        self.show_volume = show;
    }

    /// Set the volume chart height ratio
    pub fn set_volume_height_ratio(&mut self, ratio: f32) {
        self.volume_height_ratio = ratio.clamp(0.1, 0.5);
    }

    /// Set the current interval
    pub fn set_interval(&mut self, interval: Interval) {
        self.interval = interval;
    }

    /// Get the current interval
    pub fn get_interval(&self) -> Interval {
        self.interval
    }

    /// Mark that a history load is in progress
    pub fn set_loading_history(&mut self, loading: bool) {
        self.loading_history = loading;
    }

    /// Get the datetime of the earliest bar in the manager
    pub fn get_earliest_bar_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.manager.get_earliest_time()
    }

    /// Add an indicator
    pub fn add_indicator(&mut self, indicator: Box<dyn Indicator>) {
        self.indicators.push(indicator);
        self.recalculate_indicators();
    }

    /// Remove all indicators
    pub fn clear_indicators(&mut self) {
        self.indicators.clear();
    }

    /// Clear all bar data (used before loading new interval data)
    pub fn clear_data(&mut self) {
        self.manager.clear();
        self.right_ix = 0;
        self.loading_history = false;
    }

    /// Remove a specific indicator by index
    pub fn remove_indicator(&mut self, index: usize) {
        if index < self.indicators.len() {
            self.indicators.remove(index);
        }
    }

    /// Get the number of active indicators
    pub fn indicator_count(&self) -> usize {
        self.indicators.len()
    }

    /// Get indicator name by index
    pub fn indicator_name(&self, index: usize) -> Option<&str> {
        self.indicators.get(index).map(|ind| ind.name())
    }

    /// Recalculate all indicators
    fn recalculate_indicators(&mut self) {
        let bars = self.manager.get_all_bars();
        for indicator in &mut self.indicators {
            indicator.calculate(bars);
        }
    }

    /// Update with historical bar data
    pub fn update_history(&mut self, history: Vec<BarData>) {
        self.manager.update_history(history);
        self.move_to_right();
        self.recalculate_indicators();
    }

    /// Update with a single bar
    pub fn update_bar(&mut self, bar: BarData) {
        self.manager.update_bar(bar);
        self.recalculate_indicators();

        // Auto-scroll if near the right edge
        if self.right_ix >= self.manager.get_count().saturating_sub(self.bar_count / 2) {
            self.move_to_right();
        }
    }

    /// Clear all data
    pub fn clear_all(&mut self) {
        self.manager.clear_all();
        self.trade_overlay.clear();
        self.cursor.clear();
        self.right_ix = 0;
    }

    /// Move chart to the rightmost position
    pub fn move_to_right(&mut self) {
        self.right_ix = self.manager.get_count();
    }

    /// Get the visible bar range
    fn get_visible_range(&self) -> (usize, usize) {
        let max_ix = self.right_ix.min(self.manager.get_count());
        let min_ix = max_ix.saturating_sub(self.bar_count);
        (min_ix, max_ix.saturating_sub(1))
    }

    /// Handle keyboard input
    fn handle_keyboard(&mut self, ui: &Ui) {
        let count = self.manager.get_count();

        if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.right_ix = self.right_ix.saturating_sub(1).max(self.bar_count);
            self.cursor.move_left(&self.manager);
        }

        if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
            self.right_ix = (self.right_ix + 1).min(count);
            self.cursor.move_right(&self.manager);
        }

        if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
            self.bar_count = (self.bar_count as f64 / 1.2) as usize;
            self.bar_count = self.bar_count.max(MIN_BAR_COUNT);
        }

        if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
            self.bar_count = (self.bar_count as f64 * 1.2) as usize;
            self.bar_count = self.bar_count.min(count);
        }

        if ui.input(|i| i.key_pressed(Key::Home)) {
            self.right_ix = self.bar_count;
        }

        if ui.input(|i| i.key_pressed(Key::End)) {
            self.move_to_right();
        }
    }

    /// Handle mouse wheel for zooming
    fn handle_scroll(&mut self, ui: &Ui) {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta);
        if scroll_delta.y != 0.0 {
            let count = self.manager.get_count();
            if scroll_delta.y > 0.0 {
                self.bar_count = (self.bar_count as f64 * 1.1) as usize;
                self.bar_count = self.bar_count.min(count);
            } else {
                self.bar_count = (self.bar_count as f64 / 1.1) as usize;
                self.bar_count = self.bar_count.max(MIN_BAR_COUNT);
            }
        }
    }

    /// Handle mouse drag for panning
    /// Returns true if user dragged to the left edge and needs more historical data
    fn handle_drag(&mut self, response: &Response, candle_rect: Rect) -> bool {
        let mut need_more = false;
        if response.dragged() {
            let delta = response.drag_delta();
            if delta.x != 0.0 {
                let bar_pixel_width = candle_rect.width() / self.bar_count as f32;
                let bar_delta = (-delta.x / bar_pixel_width) as isize;

                let count = self.manager.get_count();
                let new_right = (self.right_ix as isize + bar_delta).max(0) as usize;
                self.right_ix = new_right.clamp(self.bar_count, count);

                // Detect if user is trying to scroll past the left edge of available data
                let left_ix = if self.right_ix >= self.bar_count {
                    self.right_ix - self.bar_count
                } else {
                    0
                };
                // If the leftmost visible bar is within 10 bars of the start, and we're dragging left
                if left_ix < 10 && bar_delta > 0 && !self.loading_history {
                    need_more = true;
                }
            }
        }
        need_more
    }

    /// Show the chart widget
    pub fn show(&mut self, ui: &mut Ui, symbol: Option<&str>) -> (Response, Option<ChartEvent>) {
        let mut event = None;
        // Draw toolbar first
        egui::TopBottomPanel::top("chart_toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("周期:");
                if ui.button(self.interval.display_name()).clicked() {
                    self.show_interval_selector = !self.show_interval_selector;
                }

                ui.separator();

                if ui.button("技术指标").clicked() {
                    self.show_indicator_selector = !self.show_indicator_selector;
                }

                ui.separator();

                if ui.button("配置指标").clicked() {
                    self.show_indicator_config = !self.show_indicator_config;
                }

                ui.separator();

                if ui.button("指标管理").clicked() {
                    self.show_indicator_manager = !self.show_indicator_manager;
                }

                ui.separator();

                if ui.button("时间范围").clicked() {
                    self.show_time_selector = !self.show_time_selector;
                }

                ui.separator();

                ui.checkbox(&mut self.auto_scale_y, "自动缩放");

                // Save/Load config buttons (only show if symbol is provided)
                if let Some(sym) = symbol {
                    ui.separator();

                    if ui.button("💾 保存配置").clicked() {
                        if let Err(e) = self.save_config_to_file(sym) {
                            tracing::error!("保存配置失败: {}", e);
                        } else {
                            tracing::info!("配置已保存: {}", sym);
                        }
                    }

                    if ui.button("📂 加载配置").clicked() {
                        if let Err(e) = self.load_config_from_file(sym) {
                            tracing::warn!("加载配置失败: {}", e);
                        } else {
                            tracing::info!("配置已加载: {}", sym);
                        }
                    }
                }
            });

            // Show interval selector popup
            if self.show_interval_selector {
                egui::Window::new("选择周期")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        for interval in Interval::all() {
                            if ui
                                .selectable_label(
                                    self.interval == interval,
                                    interval.display_name(),
                                )
                                .clicked()
                            {
                                self.interval = interval;
                                self.show_interval_selector = false;
                                self.clear_all();
                                event = Some(ChartEvent {
                                    interval_changed: true,
                                    new_interval: interval,
                                    need_more_history: false,
                                    symbol: symbol.map(|s| s.to_string()),
                                });
                            }
                        }
                    });
            }

            // Show indicator selector popup
            if self.show_indicator_selector {
                egui::Window::new("添加指标")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        for ind_type in IndicatorType::all() {
                            if ui.button(ind_type.display_name()).clicked() {
                                // Add indicator based on type
                                match ind_type {
                                    IndicatorType::MA => {
                                        self.add_indicator(Box::new(MA::new(
                                            20,
                                            Color32::YELLOW,
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::EMA => {
                                        self.add_indicator(Box::new(EMA::new(
                                            20,
                                            Color32::from_rgb(0, 255, 255),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::WMA => {
                                        self.add_indicator(Box::new(WMA::new(
                                            20,
                                            Color32::from_rgb(255, 100, 255),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::BOLL => {
                                        self.add_indicator(Box::new(BOLL::new(
                                            20,
                                            2.0,
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::VWAP => {
                                        self.add_indicator(Box::new(VWAP::new(
                                            Color32::from_rgb(100, 255, 100),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::SAR => {
                                        self.add_indicator(Box::new(SAR::new(
                                            0.02,
                                            0.2,
                                            Color32::from_rgb(255, 0, 255),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::AVL => {
                                        self.add_indicator(Box::new(AVL::new(
                                            Color32::from_rgb(255, 200, 0),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                    IndicatorType::TRIX => {
                                        self.add_indicator(Box::new(TRIX::new(
                                            12,
                                            9,
                                            Color32::from_rgb(0, 150, 255),
                                            Color32::from_rgb(255, 100, 0),
                                            IndicatorLocation::Sub,
                                        )));
                                    }
                                    IndicatorType::SUPER => {
                                        self.add_indicator(Box::new(SUPER::new(
                                            10,
                                            3.0,
                                            Color32::from_rgb(0, 255, 0),
                                            Color32::from_rgb(255, 0, 0),
                                            IndicatorLocation::Main,
                                        )));
                                    }
                                }
                                self.show_indicator_selector = false;
                            }
                        }

                        if ui.button("清除所有指标").clicked() {
                            self.clear_indicators();
                            self.show_indicator_selector = false;
                        }
                    });
            }

            // Show indicator configuration dialog
            if self.show_indicator_config {
                egui::Window::new("配置指标")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("指标类型:");
                            egui::ComboBox::from_id_salt("indicator_type")
                                .selected_text(self.indicator_config.indicator_type.display_name())
                                .show_ui(ui, |ui| {
                                    for ind_type in IndicatorType::all() {
                                        ui.selectable_value(
                                            &mut self.indicator_config.indicator_type,
                                            ind_type,
                                            ind_type.display_name(),
                                        );
                                    }
                                });
                        });

                        ui.separator();

                        // Show different parameters based on indicator type
                        match self.indicator_config.indicator_type {
                            IndicatorType::MA | IndicatorType::EMA | IndicatorType::WMA => {
                                ui.horizontal(|ui| {
                                    ui.label("周期:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.period)
                                            .range(1..=200),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("线宽:");
                                    ui.add(egui::Slider::new(
                                        &mut self.indicator_config.line_width,
                                        0.5..=5.0,
                                    ));
                                });
                            }
                            IndicatorType::BOLL => {
                                ui.horizontal(|ui| {
                                    ui.label("周期:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.period)
                                            .range(1..=200),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("标准差倍数:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.multiplier)
                                            .range(0.5..=5.0)
                                            .speed(0.1),
                                    );
                                });
                            }
                            IndicatorType::VWAP => {
                                ui.horizontal(|ui| {
                                    ui.label("颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("线宽:");
                                    ui.add(egui::Slider::new(
                                        &mut self.indicator_config.line_width,
                                        0.5..=5.0,
                                    ));
                                });
                            }
                            IndicatorType::AVL => {
                                ui.horizontal(|ui| {
                                    ui.label("颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("线宽:");
                                    ui.add(egui::Slider::new(
                                        &mut self.indicator_config.line_width,
                                        0.5..=5.0,
                                    ));
                                });
                            }
                            IndicatorType::TRIX => {
                                ui.horizontal(|ui| {
                                    ui.label("周期:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.period)
                                            .range(1..=200),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("信号线周期:");
                                    ui.add(
                                        egui::DragValue::new(
                                            &mut self.indicator_config.signal_period,
                                        )
                                        .range(1..=100),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("TRIX颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("信号线颜色:");
                                    ui.color_edit_button_srgba(
                                        &mut self.indicator_config.signal_color,
                                    );
                                });
                            }
                            IndicatorType::SAR => {
                                ui.horizontal(|ui| {
                                    ui.label("加速因子:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.multiplier)
                                            .range(0.001..=0.2)
                                            .speed(0.001),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                            }
                            IndicatorType::SUPER => {
                                ui.horizontal(|ui| {
                                    ui.label("ATR周期:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.period)
                                            .range(1..=100),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("ATR倍数:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.indicator_config.multiplier)
                                            .range(0.5..=10.0)
                                            .speed(0.1),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("上升颜色:");
                                    ui.color_edit_button_srgba(&mut self.indicator_config.color);
                                });
                                ui.horizontal(|ui| {
                                    ui.label("下降颜色:");
                                    ui.color_edit_button_srgba(
                                        &mut self.indicator_config.signal_color,
                                    );
                                });
                            }
                        }

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("显示位置:");
                            ui.radio_value(
                                &mut self.indicator_config.location,
                                IndicatorLocation::Main,
                                "主图",
                            );
                            ui.radio_value(
                                &mut self.indicator_config.location,
                                IndicatorLocation::Sub,
                                "副图",
                            );
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            if ui.button("添加").clicked() {
                                // Create indicator based on config
                                let indicator: Box<dyn Indicator> =
                                    match self.indicator_config.indicator_type {
                                        IndicatorType::MA => Box::new(MA::new(
                                            self.indicator_config.period,
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::EMA => Box::new(EMA::new(
                                            self.indicator_config.period,
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::WMA => Box::new(WMA::new(
                                            self.indicator_config.period,
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::BOLL => Box::new(BOLL::new(
                                            self.indicator_config.period,
                                            self.indicator_config.multiplier,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::VWAP => Box::new(VWAP::new(
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::AVL => Box::new(AVL::new(
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::TRIX => Box::new(TRIX::new(
                                            self.indicator_config.period,
                                            self.indicator_config.signal_period,
                                            self.indicator_config.color,
                                            self.indicator_config.signal_color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::SAR => Box::new(SAR::new(
                                            self.indicator_config.multiplier,
                                            0.2,
                                            self.indicator_config.color,
                                            self.indicator_config.location,
                                        )),
                                        IndicatorType::SUPER => Box::new(SUPER::new(
                                            self.indicator_config.period,
                                            self.indicator_config.multiplier,
                                            self.indicator_config.color,
                                            self.indicator_config.signal_color,
                                            self.indicator_config.location,
                                        )),
                                    };
                                self.add_indicator(indicator);
                                self.show_indicator_config = false;
                            }

                            if ui.button("取消").clicked() {
                                self.show_indicator_config = false;
                            }
                        });
                    });
            }

            // Show time range selector
            if self.show_time_selector {
                egui::Window::new("时间范围选择")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.label("选择要显示的时间范围:");
                        ui.separator();

                        // Quick time range buttons
                        ui.horizontal(|ui| {
                            if ui.button("最近1小时").clicked() {
                                let now = chrono::Utc::now();
                                let start = now - chrono::Duration::hours(1);
                                self.time_range = Some((start, now));
                                self.show_time_selector = false;
                            }
                            if ui.button("最近6小时").clicked() {
                                let now = chrono::Utc::now();
                                let start = now - chrono::Duration::hours(6);
                                self.time_range = Some((start, now));
                                self.show_time_selector = false;
                            }
                            if ui.button("最近24小时").clicked() {
                                let now = chrono::Utc::now();
                                let start = now - chrono::Duration::hours(24);
                                self.time_range = Some((start, now));
                                self.show_time_selector = false;
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui.button("最近7天").clicked() {
                                let now = chrono::Utc::now();
                                let start = now - chrono::Duration::days(7);
                                self.time_range = Some((start, now));
                                self.show_time_selector = false;
                            }
                            if ui.button("最近30天").clicked() {
                                let now = chrono::Utc::now();
                                let start = now - chrono::Duration::days(30);
                                self.time_range = Some((start, now));
                                self.show_time_selector = false;
                            }
                            if ui.button("全部时间").clicked() {
                                self.time_range = None;
                                self.show_time_selector = false;
                            }
                        });

                        ui.separator();

                        // Display current range
                        if let Some((start, end)) = self.time_range {
                            ui.label(format!(
                                "当前范围: {} 至 {}",
                                start.format("%Y-%m-%d %H:%M:%S"),
                                end.format("%Y-%m-%d %H:%M:%S")
                            ));
                        } else {
                            ui.label("当前范围: 全部时间");
                        }

                        ui.separator();

                        if ui.button("关闭").clicked() {
                            self.show_time_selector = false;
                        }
                    });
            }

            // Show indicator manager panel
            if self.show_indicator_manager {
                egui::Window::new("指标管理")
                    .collapsible(false)
                    .resizable(true)
                    .default_width(320.0)
                    .show(ui.ctx(), |ui| {
                        // --- Active indicators list ---
                        ui.heading("已添加指标");
                        ui.separator();

                        if self.indicators.is_empty() {
                            ui.label("暂无指标");
                        } else {
                            // Collect names first to avoid borrowing issues
                            let names: Vec<String> = self
                                .indicators
                                .iter()
                                .map(|ind| ind.name().to_string())
                                .collect();
                            for (i, name) in names.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}. {}", i + 1, name));
                                    if ui.small_button("删除").clicked() {
                                        self.remove_indicator(i);
                                    }
                                });
                            }
                        }

                        ui.separator();

                        if ui.button("清除所有指标").clicked() {
                            self.clear_indicators();
                        }

                        ui.add_space(8.0);

                        // --- Custom indicator builder ---
                        ui.heading("自定义指标");
                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("名称:");
                            ui.text_edit_singleline(&mut self.custom_indicator_name);
                        });

                        ui.horizontal(|ui| {
                            ui.label("公式:");
                            ui.text_edit_singleline(&mut self.custom_indicator_expr);
                        });

                        ui.horizontal(|ui| {
                            ui.label("变量: open, high, low, close, volume");
                        });
                        ui.horizontal(|ui| {
                            ui.label("示例: (high + low) / 2, close * 1.02");
                        });

                        if let Some(ref err) = self.custom_indicator_error {
                            ui.colored_label(Color32::from_rgb(255, 80, 80), err);
                        }

                        ui.horizontal(|ui| {
                            if ui.button("添加自定义指标").clicked() {
                                self.custom_indicator_error = None;
                                let name = self.custom_indicator_name.trim().to_string();
                                let expr = self.custom_indicator_expr.trim().to_string();

                                if name.is_empty() {
                                    self.custom_indicator_error =
                                        Some("请输入指标名称".to_string());
                                } else if expr.is_empty() {
                                    self.custom_indicator_error = Some("请输入表达式".to_string());
                                } else if let Err(e) = validate_expression(&expr) {
                                    self.custom_indicator_error =
                                        Some(format!("表达式错误: {}", e));
                                } else {
                                    // Pick a color based on indicator count for variety
                                    let colors = [
                                        Color32::from_rgb(0, 200, 255),
                                        Color32::from_rgb(255, 128, 0),
                                        Color32::from_rgb(128, 255, 0),
                                        Color32::from_rgb(255, 0, 200),
                                        Color32::from_rgb(0, 255, 200),
                                        Color32::from_rgb(200, 200, 0),
                                    ];
                                    let color = colors[self.indicator_count() % colors.len()];
                                    self.add_indicator(Box::new(CustomIndicator::new(
                                        name,
                                        expr,
                                        color,
                                        IndicatorLocation::Main,
                                    )));
                                    self.custom_indicator_name.clear();
                                    self.custom_indicator_expr.clear();
                                }
                            }

                            if ui.button("关闭").clicked() {
                                self.show_indicator_manager = false;
                                self.custom_indicator_error = None;
                            }
                        });
                    });
            }
        });

        let available_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(available_size, Sense::click_and_drag());

        // Focus handling
        if response.clicked() {
            response.request_focus();
        }

        let has_focus = response.has_focus();

        // Handle input
        if has_focus {
            self.handle_keyboard(ui);
        }
        self.handle_scroll(ui);

        let rect = response.rect;

        // Calculate layout rectangles
        let chart_area = Rect::from_min_max(
            Pos2::new(rect.left() + MARGIN, rect.top() + MARGIN),
            Pos2::new(
                rect.right() - MARGIN - AXIS_Y_WIDTH,
                rect.bottom() - MARGIN - AXIS_X_HEIGHT,
            ),
        );

        // Check if we have sub indicators
        let has_sub_indicators = self
            .indicators
            .iter()
            .any(|ind| ind.location() == IndicatorLocation::Sub);

        // Calculate area distribution
        let (candle_rect, volume_rect, sub_chart_rect) = if has_sub_indicators {
            // Main chart (60%), Volume (15%), Sub chart (25%)
            let main_height = chart_area.height() * 0.60;
            let volume_height = if self.show_volume {
                chart_area.height() * 0.15
            } else {
                0.0
            };
            let _sub_height = chart_area.height() - main_height - volume_height;

            let candle_rect = Rect::from_min_max(
                chart_area.min,
                Pos2::new(chart_area.max.x, chart_area.min.y + main_height),
            );

            let volume_rect = if self.show_volume {
                Some(Rect::from_min_max(
                    Pos2::new(chart_area.min.x, chart_area.min.y + main_height),
                    Pos2::new(
                        chart_area.max.x,
                        chart_area.min.y + main_height + volume_height,
                    ),
                ))
            } else {
                None
            };

            let sub_chart_rect = Some(Rect::from_min_max(
                Pos2::new(
                    chart_area.min.x,
                    chart_area.min.y + main_height + volume_height,
                ),
                chart_area.max,
            ));

            (candle_rect, volume_rect, sub_chart_rect)
        } else if self.show_volume {
            // Main chart (75%), Volume (25%), No sub chart
            let volume_height = chart_area.height() * self.volume_height_ratio;
            let candle_height = chart_area.height() - volume_height;

            let candle_rect = Rect::from_min_max(
                chart_area.min,
                Pos2::new(chart_area.max.x, chart_area.min.y + candle_height),
            );
            let volume_rect = Rect::from_min_max(
                Pos2::new(chart_area.min.x, chart_area.min.y + candle_height),
                chart_area.max,
            );
            (candle_rect, Some(volume_rect), None)
        } else {
            // Only main chart
            (chart_area, None, None)
        };

        // Handle drag
        if self.handle_drag(&response, candle_rect) {
            // User dragged to the left edge - request more history
            event = Some(ChartEvent {
                interval_changed: false,
                new_interval: self.interval,
                need_more_history: true,
                symbol: None,
            });
        }

        // Get visible range
        let (min_ix, max_ix) = self.get_visible_range();

        if self.manager.get_count() == 0 {
            // Draw empty state
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "暂无数据",
                egui::FontId::proportional(20.0),
                GREY_COLOR,
            );
            return (response, event);
        }

        // Get price range (auto-scale if enabled)
        let (mut price_min, mut price_max) = if self.auto_scale_y {
            self.candle_item
                .get_y_range(&self.manager, Some(min_ix), Some(max_ix))
        } else {
            // Use full range
            self.manager.get_price_range(None, None)
        };

        // Include indicator ranges in price calculation
        for indicator in &self.indicators {
            if indicator.location() == IndicatorLocation::Main {
                if let Some((ind_min, ind_max)) = indicator.get_y_range(min_ix, max_ix) {
                    price_min = price_min.min(ind_min);
                    price_max = price_max.max(ind_max);
                }
            }
        }

        let price_padding = (price_max - price_min) * 0.05;
        let price_min = price_min - price_padding;
        let price_max = price_max + price_padding;

        // Draw candle chart
        self.candle_item.draw(
            ui,
            &self.manager,
            candle_rect,
            min_ix,
            max_ix,
            price_min,
            price_max,
        );

        // Draw indicators on main chart
        self.draw_indicators(
            ui,
            candle_rect,
            min_ix,
            max_ix,
            price_min,
            price_max,
            IndicatorLocation::Main,
        );

        // Draw trade overlay
        self.trade_overlay.draw(
            ui,
            &self.manager,
            candle_rect,
            min_ix,
            max_ix,
            price_min,
            price_max,
        );

        // Draw candle chart border
        painter.rect_stroke(
            candle_rect,
            0.0,
            Stroke::new(1.0, GREY_COLOR),
            StrokeKind::Inside,
        );

        // Draw Y-axis (price)
        self.draw_y_axis(ui, candle_rect, price_min, price_max, true);

        // Draw volume chart
        if let Some(vol_rect) = volume_rect {
            let (vol_min, vol_max) =
                self.volume_item
                    .get_y_range(&self.manager, Some(min_ix), Some(max_ix));
            let vol_max = vol_max * 1.1; // Add padding

            self.volume_item.draw(
                ui,
                &self.manager,
                vol_rect,
                min_ix,
                max_ix,
                vol_min,
                vol_max,
            );

            // Draw volume chart border
            painter.rect_stroke(
                vol_rect,
                0.0,
                Stroke::new(1.0, GREY_COLOR),
                StrokeKind::Inside,
            );

            // Draw Y-axis (volume)
            self.draw_y_axis(ui, vol_rect, vol_min, vol_max, false);
        }

        // Draw sub-chart with indicators
        if let Some(sub_rect) = sub_chart_rect {
            // Calculate sub indicator range
            let mut sub_min = f64::INFINITY;
            let mut sub_max = f64::NEG_INFINITY;

            for indicator in &self.indicators {
                if indicator.location() == IndicatorLocation::Sub {
                    if let Some((ind_min, ind_max)) = indicator.get_y_range(min_ix, max_ix) {
                        sub_min = sub_min.min(ind_min);
                        sub_max = sub_max.max(ind_max);
                    }
                }
            }

            // Add padding
            if sub_min.is_finite() && sub_max.is_finite() {
                let padding = (sub_max - sub_min) * 0.1;
                sub_min -= padding;
                sub_max += padding;

                // Draw zero line for oscillators
                let zero_y = sub_rect.bottom()
                    - ((0.0 - sub_min) / (sub_max - sub_min) * sub_rect.height() as f64) as f32;
                if zero_y >= sub_rect.top() && zero_y <= sub_rect.bottom() {
                    painter.line_segment(
                        [
                            Pos2::new(sub_rect.left(), zero_y),
                            Pos2::new(sub_rect.right(), zero_y),
                        ],
                        Stroke::new(1.0, Color32::from_gray(80)),
                    );
                }

                // Draw sub indicators
                self.draw_indicators(
                    ui,
                    sub_rect,
                    min_ix,
                    max_ix,
                    sub_min,
                    sub_max,
                    IndicatorLocation::Sub,
                );

                // Draw sub-chart border
                painter.rect_stroke(
                    sub_rect,
                    0.0,
                    Stroke::new(1.0, GREY_COLOR),
                    StrokeKind::Inside,
                );

                // Draw Y-axis for sub chart
                self.draw_y_axis(ui, sub_rect, sub_min, sub_max, false);
            } else {
                // Draw empty sub chart
                painter.rect_stroke(
                    sub_rect,
                    0.0,
                    Stroke::new(1.0, GREY_COLOR),
                    StrokeKind::Inside,
                );
                painter.text(
                    sub_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "副图 - 添加技术指标",
                    egui::FontId::proportional(14.0),
                    GREY_COLOR,
                );
            }
        }

        // Draw X-axis (datetime)
        self.draw_x_axis(ui, chart_area, min_ix, max_ix);

        // Handle cursor
        if let Some(hover_pos) = response.hover_pos() {
            self.cursor.update_position(
                hover_pos,
                &self.manager,
                candle_rect,
                volume_rect,
                min_ix,
                max_ix,
                price_min,
                price_max,
            );

            self.cursor.draw(
                ui,
                &self.manager,
                &self.candle_item,
                &self.volume_item,
                candle_rect,
                volume_rect,
                min_ix,
                max_ix,
                price_min,
                price_max,
                self.price_decimals,
            );
        }

        (response, event)
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_indicators(
        &self,
        ui: &Ui,
        chart_rect: Rect,
        min_ix: usize,
        max_ix: usize,
        price_min: f64,
        price_max: f64,
        location: IndicatorLocation,
    ) {
        let painter = ui.painter();

        for indicator in &self.indicators {
            if indicator.location() != location {
                continue;
            }

            for series_idx in 0..indicator.series_count() {
                if let Some(config) = indicator.get_line_config(series_idx) {
                    let mut points = Vec::new();

                    for ix in min_ix..=max_ix {
                        if let Some(value) = indicator.get_value(ix, series_idx) {
                            let bar_count = max_ix - min_ix + 1;
                            let bar_pixel_width = chart_rect.width() / bar_count as f32;
                            let x = chart_rect.left()
                                + (ix - min_ix) as f32 * bar_pixel_width
                                + bar_pixel_width * 0.5;

                            let price_range = price_max - price_min;
                            let normalized = if price_range.abs() > 1e-10 {
                                (value - price_min) / price_range
                            } else {
                                0.5
                            };
                            let y = chart_rect.bottom() - (normalized as f32 * chart_rect.height());

                            points.push(Pos2::new(x, y));
                        }
                    }

                    // Draw line connecting all points
                    if points.len() > 1 {
                        painter.add(egui::Shape::line(
                            points,
                            config.style.to_stroke(config.width, config.color),
                        ));
                    }
                }
            }
        }
    }

    /// Draw Y-axis with tick labels
    fn draw_y_axis(
        &self,
        ui: &mut Ui,
        chart_rect: Rect,
        min_val: f64,
        max_val: f64,
        is_price: bool,
    ) {
        let painter = ui.painter();
        let axis_rect = Rect::from_min_max(
            Pos2::new(chart_rect.right(), chart_rect.top()),
            Pos2::new(chart_rect.right() + AXIS_Y_WIDTH, chart_rect.bottom()),
        );

        let ticks = calculate_axis_ticks(min_val, max_val, 5);

        for tick in ticks {
            let normalized = (tick - min_val) / (max_val - min_val);
            let y = chart_rect.bottom() - (normalized as f32 * chart_rect.height());

            // Draw tick line
            painter.line_segment(
                [
                    Pos2::new(chart_rect.right(), y),
                    Pos2::new(chart_rect.right() + 4.0, y),
                ],
                Stroke::new(1.0, GREY_COLOR),
            );

            // Draw label
            let label = if is_price {
                format_price(tick, self.price_decimals)
            } else {
                format_volume(tick)
            };

            painter.text(
                Pos2::new(axis_rect.left() + 6.0, y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(11.0),
                WHITE_COLOR,
            );
        }
    }

    /// Draw X-axis with datetime labels
    fn draw_x_axis(&self, ui: &mut Ui, chart_area: Rect, min_ix: usize, max_ix: usize) {
        let painter = ui.painter();

        let bar_count = max_ix - min_ix + 1;
        let num_ticks = (chart_area.width() / 120.0) as usize;
        let num_ticks = num_ticks.max(2);
        let tick_step = bar_count / num_ticks;

        for i in 0..=num_ticks {
            let ix = min_ix + (i * tick_step).min(bar_count - 1);

            if let Some(dt) = self.manager.get_datetime(ix as f64) {
                let normalized = (ix - min_ix) as f32 / bar_count as f32;
                let x = chart_area.left() + normalized * chart_area.width();
                let y = chart_area.bottom();

                // Draw tick line
                painter.line_segment(
                    [Pos2::new(x, y), Pos2::new(x, y + 4.0)],
                    Stroke::new(1.0, GREY_COLOR),
                );

                // Draw label
                let label = dt.format("%m-%d\n%H:%M").to_string();
                painter.text(
                    Pos2::new(x, y + 6.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    egui::FontId::proportional(10.0),
                    WHITE_COLOR,
                );
            }
        }
    }
}

/// Chart cursor for showing crosshairs and info
pub struct ChartCursor {
    /// Current X position (bar index)
    x: usize,
    /// Current Y position (price or volume)
    y: f64,
    /// Current screen position
    screen_pos: Pos2,
    /// Whether cursor is in candle area
    in_candle_area: bool,
    /// Whether cursor is in volume area
    in_volume_area: bool,
    /// Whether cursor is visible
    visible: bool,
}

impl Default for ChartCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartCursor {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0.0,
            screen_pos: Pos2::ZERO,
            in_candle_area: false,
            in_volume_area: false,
            visible: false,
        }
    }

    pub fn clear(&mut self) {
        self.x = 0;
        self.y = 0.0;
        self.visible = false;
    }

    pub fn move_left(&mut self, manager: &BarManager) {
        if self.x > 0 {
            self.x -= 1;
            if let Some(bar) = manager.get_bar(self.x as f64) {
                self.y = bar.close_price;
            }
        }
    }

    pub fn move_right(&mut self, manager: &BarManager) {
        if self.x < manager.get_count().saturating_sub(1) {
            self.x += 1;
            if let Some(bar) = manager.get_bar(self.x as f64) {
                self.y = bar.close_price;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_position(
        &mut self,
        pos: Pos2,
        _manager: &BarManager,
        candle_rect: Rect,
        volume_rect: Option<Rect>,
        min_ix: usize,
        max_ix: usize,
        price_min: f64,
        price_max: f64,
    ) {
        self.screen_pos = pos;
        self.in_candle_area = candle_rect.contains(pos);
        self.in_volume_area = volume_rect.is_some_and(|r| r.contains(pos));
        self.visible = self.in_candle_area || self.in_volume_area;

        if !self.visible {
            return;
        }

        // Calculate bar index from X position
        let bar_count = max_ix - min_ix + 1;
        let bar_pixel_width = candle_rect.width() / bar_count as f32;
        let relative_x = pos.x - candle_rect.left();
        let bar_offset = (relative_x / bar_pixel_width) as usize;
        self.x = (min_ix + bar_offset).min(max_ix);

        // Calculate Y value
        if self.in_candle_area {
            let normalized = 1.0 - (pos.y - candle_rect.top()) / candle_rect.height();
            self.y = price_min + (normalized as f64) * (price_max - price_min);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        ui: &mut Ui,
        manager: &BarManager,
        candle_item: &CandleItem,
        volume_item: &VolumeItem,
        candle_rect: Rect,
        volume_rect: Option<Rect>,
        min_ix: usize,
        max_ix: usize,
        _price_min: f64,
        _price_max: f64,
        price_decimals: usize,
    ) {
        if !self.visible {
            return;
        }

        let painter = ui.painter();
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 128));

        // Calculate bar center X position
        let bar_count = max_ix - min_ix + 1;
        let bar_pixel_width = candle_rect.width() / bar_count as f32;
        let bar_x =
            candle_rect.left() + (self.x - min_ix) as f32 * bar_pixel_width + bar_pixel_width * 0.5;

        // Draw vertical line through both chart areas
        let top = candle_rect.top();
        let bottom = volume_rect.map_or(candle_rect.bottom(), |r| r.bottom());
        painter.line_segment([Pos2::new(bar_x, top), Pos2::new(bar_x, bottom)], stroke);

        // Draw horizontal line in the current area
        if self.in_candle_area {
            painter.line_segment(
                [
                    Pos2::new(candle_rect.left(), self.screen_pos.y),
                    Pos2::new(candle_rect.right(), self.screen_pos.y),
                ],
                stroke,
            );

            // Draw price label on Y-axis
            let label = format_price(self.y, price_decimals);
            let label_pos = Pos2::new(candle_rect.right() + 4.0, self.screen_pos.y);

            let text_size = ui.fonts_mut(|f| f.glyph_width(&egui::FontId::proportional(11.0), ' '))
                * label.len() as f32;
            let label_rect = Rect::from_min_size(
                Pos2::new(label_pos.x, label_pos.y - 8.0),
                Vec2::new(text_size + 8.0, 16.0),
            );
            painter.rect_filled(label_rect, 2.0, CURSOR_COLOR);
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(11.0),
                Color32::BLACK,
            );
        }

        // Draw datetime label on X-axis
        if let Some(dt) = manager.get_datetime(self.x as f64) {
            let label = dt.format("%Y-%m-%d %H:%M").to_string();
            let label_pos = Pos2::new(bar_x, bottom + 4.0);

            let text_size = ui.fonts_mut(|f| f.glyph_width(&egui::FontId::proportional(11.0), ' '))
                * label.len() as f32;
            let label_rect = Rect::from_min_size(
                Pos2::new(bar_x - text_size * 0.5, label_pos.y),
                Vec2::new(text_size, 16.0),
            );
            painter.rect_filled(label_rect, 2.0, CURSOR_COLOR);
            painter.text(
                Pos2::new(bar_x, label_pos.y + 8.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                Color32::BLACK,
            );
        }

        // Draw info box
        let info_text = candle_item.get_info_text(manager, self.x);
        let volume_info = volume_item.get_info_text(manager, self.x);
        let full_info = if volume_info.is_empty() {
            info_text
        } else {
            format!("{}\n\n{}", info_text, volume_info)
        };

        if !full_info.is_empty() {
            // Position info box on the opposite side of the cursor
            let info_x = if self.screen_pos.x < candle_rect.center().x {
                candle_rect.right() - INFO_BOX_WIDTH - 4.0
            } else {
                candle_rect.left() + 4.0
            };

            let info_rect = Rect::from_min_size(
                Pos2::new(info_x, candle_rect.top() + 4.0),
                Vec2::new(INFO_BOX_WIDTH, INFO_BOX_HEIGHT),
            );

            painter.rect_filled(
                info_rect,
                4.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 200),
            );
            painter.rect_stroke(
                info_rect,
                4.0,
                Stroke::new(1.0, GREY_COLOR),
                StrokeKind::Inside,
            );

            painter.text(
                Pos2::new(info_rect.left() + 8.0, info_rect.top() + 8.0),
                egui::Align2::LEFT_TOP,
                full_info,
                egui::FontId::proportional(11.0),
                WHITE_COLOR,
            );
        }
    }
}

/// Serializable indicator configuration for saving/loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIndicatorConfig {
    pub indicator_type: String,
    pub period: Option<usize>,
    pub multiplier: Option<f64>,
    pub signal_period: Option<usize>,
    pub color: [u8; 4], // RGBA
    pub signal_color: Option<[u8; 4]>,
    pub location: String, // "Main" or "Sub"
    pub line_width: Option<f32>,
}

/// Chart configuration for saving/loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartConfig {
    pub indicators: Vec<SerializableIndicatorConfig>,
    pub show_volume: bool,
    pub auto_scale_y: bool,
    pub interval: String,
}

impl ChartWidget {
    /// Export current chart configuration
    pub fn export_config(&self) -> ChartConfig {
        let indicators = self
            .indicators
            .iter()
            .map(|ind| {
                let name = ind.name();
                let location = match ind.location() {
                    IndicatorLocation::Main => "Main",
                    IndicatorLocation::Sub => "Sub",
                };

                // Try to extract parameters from indicator name or use defaults
                SerializableIndicatorConfig {
                    indicator_type: name.to_string(),
                    period: Some(20), // Default, actual value depends on indicator
                    multiplier: Some(2.0), // Default
                    signal_period: Some(9), // Default
                    color: if let Some(config) = ind.get_line_config(0) {
                        config.color.to_array()
                    } else {
                        [255, 255, 0, 255]
                    },
                    signal_color: ind.get_line_config(1).map(|c| c.color.to_array()),
                    location: location.to_string(),
                    line_width: ind.get_line_config(0).map(|c| c.width),
                }
            })
            .collect();

        ChartConfig {
            indicators,
            show_volume: self.show_volume,
            auto_scale_y: self.auto_scale_y,
            interval: format!("{:?}", self.interval),
        }
    }

    /// Import chart configuration
    pub fn import_config(&mut self, config: ChartConfig) {
        self.show_volume = config.show_volume;
        self.auto_scale_y = config.auto_scale_y;

        // Clear existing indicators
        self.clear_indicators();

        // Recreate indicators from config
        for ind_config in config.indicators {
            let location = if ind_config.location == "Main" {
                IndicatorLocation::Main
            } else {
                IndicatorLocation::Sub
            };

            let color = Color32::from_rgba_unmultiplied(
                ind_config.color[0],
                ind_config.color[1],
                ind_config.color[2],
                ind_config.color[3],
            );

            let signal_color = ind_config
                .signal_color
                .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                .unwrap_or(Color32::from_rgb(255, 100, 0));

            let indicator: Box<dyn Indicator> = match ind_config.indicator_type.as_str() {
                "MA" => Box::new(MA::new(ind_config.period.unwrap_or(20), color, location)),
                "EMA" => Box::new(EMA::new(ind_config.period.unwrap_or(20), color, location)),
                "WMA" => Box::new(WMA::new(ind_config.period.unwrap_or(20), color, location)),
                "BOLL" => Box::new(BOLL::new(
                    ind_config.period.unwrap_or(20),
                    ind_config.multiplier.unwrap_or(2.0),
                    location,
                )),
                "VWAP" => Box::new(VWAP::new(color, location)),
                "AVL" => Box::new(AVL::new(color, location)),
                "TRIX" => Box::new(TRIX::new(
                    ind_config.period.unwrap_or(12),
                    ind_config.signal_period.unwrap_or(9),
                    color,
                    signal_color,
                    location,
                )),
                "SAR" => Box::new(SAR::new(
                    ind_config.multiplier.unwrap_or(0.02),
                    0.2,
                    color,
                    location,
                )),
                "SUPER" => Box::new(SUPER::new(
                    ind_config.period.unwrap_or(10),
                    ind_config.multiplier.unwrap_or(3.0),
                    color,
                    signal_color,
                    location,
                )),
                _ => continue, // Skip unknown indicators
            };

            self.add_indicator(indicator);
        }
    }

    /// Save configuration to file
    pub fn save_config_to_file(&self, symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.export_config();
        let config_dir = dirs::config_dir()
            .ok_or("Failed to get config directory")?
            .join(".rstrader")
            .join("chart_configs");

        fs::create_dir_all(&config_dir)?;

        let filename = format!("{}_chart.json", symbol.replace("/", "_"));
        let filepath = config_dir.join(filename);

        let json = serde_json::to_string_pretty(&config)?;
        fs::write(filepath, json)?;

        Ok(())
    }

    /// Load configuration from file
    pub fn load_config_from_file(
        &mut self,
        symbol: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Failed to get config directory")?
            .join(".rstrader")
            .join("chart_configs");

        let filename = format!("{}_chart.json", symbol.replace("/", "_"));
        let filepath = config_dir.join(filename);

        if !filepath.exists() {
            return Err("Configuration file does not exist".into());
        }

        let json = fs::read_to_string(filepath)?;
        let config: ChartConfig = serde_json::from_str(&json)?;

        self.import_config(config);

        Ok(())
    }
}
