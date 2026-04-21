//! Advanced Orders Panel — GUI for StopOrderEngine + OrderEmulator.
//!
//! Two-tab panel: 止损单 (Stop Orders) and 模拟委托 (Emulated Orders).
//! Each tab has a form to create orders and a table to view/cancel active orders.

use egui::{Grid, RichText, ScrollArea, Ui};

use crate::trader::constant::{Direction, Exchange, Offset};
use crate::trader::order_emulator::{
    EmulatedOrder, EmulatedOrderRequest, EmulatedOrderType, OrderEmulator,
};
use crate::trader::stop_order::{StopOrder, StopOrderRequest, StopOrderType, StopOrderEngine};

use super::style::{
    COLOR_LONG, COLOR_SHORT, COLOR_TEXT_SECONDARY, ToastManager, ToastType,
};

// ---------------------------------------------------------------------------
// Tab selection
// ---------------------------------------------------------------------------

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedOrdersTab {
    #[default]
    StopOrders,
    EmulatedOrders,
}

// ---------------------------------------------------------------------------
// Stop-order form state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct StopOrderForm {
    symbol: String,
    exchange: Exchange,
    direction: Direction,
    stop_type: StopOrderType,
    stop_price: String,
    limit_price: String,
    volume: String,
    offset: Offset,
    trail_pct: String,
    trail_abs: String,
    gateway_name: String,
}

impl Default for StopOrderForm {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            stop_type: StopOrderType::StopMarket,
            stop_price: String::new(),
            limit_price: String::new(),
            volume: "0.01".to_string(),
            offset: Offset::None,
            trail_pct: String::new(),
            trail_abs: String::new(),
            gateway_name: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Emulated-order form state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct EmulatedOrderForm {
    symbol: String,
    exchange: Exchange,
    direction: Direction,
    order_type: EmulatedOrderType,
    volume: String,
    trigger_price: String,
    limit_price: String,
    visible_volume: String,
    iceberg_price: String,
    trail_pct: String,
    trail_abs: String,
    offset: Offset,
    gateway_name: String,
}

impl Default for EmulatedOrderForm {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: EmulatedOrderType::Iceberg,
            volume: "1.0".to_string(),
            trigger_price: String::new(),
            limit_price: String::new(),
            visible_volume: String::new(),
            iceberg_price: String::new(),
            trail_pct: String::new(),
            trail_abs: String::new(),
            offset: Offset::None,
            gateway_name: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct AdvancedOrdersPanel {
    tab: AdvancedOrdersTab,
    stop_form: StopOrderForm,
    emul_form: EmulatedOrderForm,
    /// Cached stop orders for display (refreshed each frame).
    stop_orders_cache: Vec<StopOrder>,
    /// Cached emulated orders for display (refreshed each frame).
    emul_orders_cache: Vec<EmulatedOrder>,
    /// Pending cancel-stop-order action (consumed by MainWindow).
    pending_cancel_stop: Option<u64>,
    /// Pending cancel-emulated-order action (consumed by MainWindow).
    pending_cancel_emul: Option<u64>,
    /// Pending stop order request for MainWindow to execute on the engine.
    pending_stop_request: Option<StopOrderRequest>,
    /// Pending emulated order request for MainWindow to execute on the engine.
    pending_emul_request: Option<EmulatedOrderRequest>,
}

impl Default for AdvancedOrdersPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancedOrdersPanel {
    pub fn new() -> Self {
        Self {
            tab: AdvancedOrdersTab::default(),
            stop_form: StopOrderForm::default(),
            emul_form: EmulatedOrderForm::default(),
            stop_orders_cache: Vec::new(),
            emul_orders_cache: Vec::new(),
            pending_cancel_stop: None,
            pending_cancel_emul: None,
            pending_stop_request: None,
            pending_emul_request: None,
        }
    }

    /// Refresh cached data from engines (call once per frame from MainWindow).
    pub fn refresh_data(
        &mut self,
        stop_engine: Option<&StopOrderEngine>,
        emul_engine: Option<&OrderEmulator>,
    ) {
        if let Some(se) = stop_engine {
            self.stop_orders_cache = se.get_all_stop_orders();
        }
        if let Some(oe) = emul_engine {
            self.emul_orders_cache = oe.get_all_orders();
        }
    }

    /// Set the gateway name list (we pick the first one as default).
    pub fn set_gateways(&mut self, gateways: &[String]) {
        if self.stop_form.gateway_name.is_empty() {
            self.stop_form.gateway_name = gateways.first().cloned().unwrap_or_default();
        }
        if self.emul_form.gateway_name.is_empty() {
            self.emul_form.gateway_name = gateways.first().cloned().unwrap_or_default();
        }
    }

    /// Take pending cancel-stop action.
    pub fn take_cancel_stop(&mut self) -> Option<u64> {
        self.pending_cancel_stop.take()
    }

    /// Take pending cancel-emulated action.
    pub fn take_cancel_emul(&mut self) -> Option<u64> {
        self.pending_cancel_emul.take()
    }

    // -----------------------------------------------------------------------
    // Main render
    // -----------------------------------------------------------------------

    pub fn show(&mut self, ui: &mut Ui, toast: &mut ToastManager) {
        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, AdvancedOrdersTab::StopOrders, "止损单");
            ui.selectable_value(&mut self.tab, AdvancedOrdersTab::EmulatedOrders, "模拟委托");
        });
        ui.separator();

        match self.tab {
            AdvancedOrdersTab::StopOrders => self.show_stop_orders(ui, toast),
            AdvancedOrdersTab::EmulatedOrders => self.show_emulated_orders(ui, toast),
        }
    }

    // =======================================================================
    // Stop Orders tab
    // =======================================================================

    fn show_stop_orders(&mut self, ui: &mut Ui, toast: &mut ToastManager) {
        ui.collapsing("新建止损单", |ui| {
            self.render_stop_form(ui, toast);
        });

        ui.add_space(6.0);
        ui.heading("活动止损单");
        ui.separator();
        self.render_stop_table(ui);
    }

    fn render_stop_form(&mut self, ui: &mut Ui, toast: &mut ToastManager) {
        Grid::new("stop_order_form")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label("合约代码:");
                ui.text_edit_singleline(&mut self.stop_form.symbol);
                ui.end_row();

                ui.label("交易所:");
                exchange_combo(ui, &mut self.stop_form.exchange);
                ui.end_row();

                ui.label("方向:");
                direction_combo(ui, &mut self.stop_form.direction);
                ui.end_row();

                ui.label("止损类型:");
                stop_type_combo(ui, &mut self.stop_form.stop_type);
                ui.end_row();

                // Show stop_price for types that need it
                if matches!(
                    self.stop_form.stop_type,
                    StopOrderType::StopMarket | StopOrderType::StopLimit | StopOrderType::TakeProfit
                ) {
                    ui.label("止损价:");
                    ui.text_edit_singleline(&mut self.stop_form.stop_price);
                    ui.end_row();
                }

                // Show limit_price only for StopLimit
                if self.stop_form.stop_type == StopOrderType::StopLimit {
                    ui.label("限价:");
                    ui.text_edit_singleline(&mut self.stop_form.limit_price);
                    ui.end_row();
                }

                // Show trail_pct for TrailingStopPct
                if self.stop_form.stop_type == StopOrderType::TrailingStopPct {
                    ui.label("追踪百分比:");
                    ui.text_edit_singleline(&mut self.stop_form.trail_pct);
                    ui.end_row();
                }

                // Show trail_abs for TrailingStopAbs
                if self.stop_form.stop_type == StopOrderType::TrailingStopAbs {
                    ui.label("追踪距离:");
                    ui.text_edit_singleline(&mut self.stop_form.trail_abs);
                    ui.end_row();
                }

                ui.label("数量:");
                ui.text_edit_singleline(&mut self.stop_form.volume);
                ui.end_row();

                ui.label("开平:");
                offset_combo(ui, &mut self.stop_form.offset);
                ui.end_row();

                ui.label("网关:");
                ui.text_edit_singleline(&mut self.stop_form.gateway_name);
                ui.end_row();
            });

        ui.add_space(4.0);
        if ui.button("提交止损单").clicked() {
            self.submit_stop_order(toast);
        }
    }

    fn submit_stop_order(&mut self, toast: &mut ToastManager) {
        // Basic validation
        let volume = match self.stop_form.volume.parse::<f64>() {
            Ok(v) if v > 0.0 => v,
            _ => {
                toast.add("数量必须大于0", ToastType::Error);
                return;
            }
        };

        let req = StopOrderRequest {
            symbol: self.stop_form.symbol.trim().to_string(),
            exchange: self.stop_form.exchange,
            direction: self.stop_form.direction,
            stop_type: self.stop_form.stop_type,
            stop_price: self.stop_form.stop_price.parse::<f64>().unwrap_or(0.0),
            limit_price: self.stop_form.limit_price.parse::<f64>().unwrap_or(0.0),
            volume,
            offset: self.stop_form.offset,
            trail_pct: self.stop_form.trail_pct.parse::<f64>().unwrap_or(0.0),
            trail_abs: self.stop_form.trail_abs.parse::<f64>().unwrap_or(0.0),
            gateway_name: self.stop_form.gateway_name.trim().to_string(),
            reference: String::new(),
            expires_at: None,
            tag: String::new(),
        };

        // The actual add_stop_order call will happen through the engine
        // We store the request for MainWindow to pick up and execute.
        self.pending_stop_request = Some(req);
        toast.add("止损单已提交", ToastType::Info);
    }

    fn render_stop_table(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                use egui_extras::Column;

                egui_extras::TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(40.0))   // ID
                    .column(Column::remainder().at_least(80.0))  // 合约
                    .column(Column::exact(40.0))   // 方向
                    .column(Column::exact(80.0))   // 类型
                    .column(Column::exact(70.0))   // 止损价
                    .column(Column::exact(60.0))   // 数量
                    .column(Column::exact(60.0))   // 状态
                    .column(Column::exact(50.0))   // 操作
                    .header(20.0, |mut header| {
                        header.col(|ui| { ui.strong("ID"); });
                        header.col(|ui| { ui.strong("合约"); });
                        header.col(|ui| { ui.strong("方向"); });
                        header.col(|ui| { ui.strong("类型"); });
                        header.col(|ui| { ui.strong("止损价"); });
                        header.col(|ui| { ui.strong("数量"); });
                        header.col(|ui| { ui.strong("状态"); });
                        header.col(|ui| { ui.strong("操作"); });
                    })
                    .body(|mut body| {
                        for order in &self.stop_orders_cache {
                            body.row(18.0, |mut row| {
                                row.col(|ui| { ui.label(format!("{}", order.id)); });
                                row.col(|ui| { ui.label(&order.vt_symbol()); });
                                row.col(|ui| {
                                    let color = if order.direction == Direction::Long {
                                        COLOR_LONG
                                    } else {
                                        COLOR_SHORT
                                    };
                                    ui.colored_label(color, format!("{}", order.direction));
                                });
                                row.col(|ui| { ui.label(format!("{}", order.stop_type)); });
                                row.col(|ui| { ui.label(format!("{:.2}", order.stop_price)); });
                                row.col(|ui| { ui.label(format!("{}", order.volume)); });
                                row.col(|ui| {
                                    let text = format!("{}", order.status);
                                    let color = match order.status {
                                        crate::trader::stop_order::StopOrderStatus::Pending => COLOR_LONG,
                                        crate::trader::stop_order::StopOrderStatus::Triggered => COLOR_SHORT,
                                        _ => COLOR_TEXT_SECONDARY,
                                    };
                                    ui.colored_label(color, text);
                                });
                                row.col(|ui| {
                                    if order.is_active() {
                                        if ui.button("撤").clicked() {
                                            self.pending_cancel_stop = Some(order.id);
                                        }
                                    }
                                });
                            });
                        }
                    });
            });
    }

    // =======================================================================
    // Emulated Orders tab
    // =======================================================================

    fn show_emulated_orders(&mut self, ui: &mut Ui, toast: &mut ToastManager) {
        ui.collapsing("新建模拟委托", |ui| {
            self.render_emul_form(ui, toast);
        });

        ui.add_space(6.0);
        ui.heading("活动模拟委托");
        ui.separator();
        self.render_emul_table(ui);
    }

    fn render_emul_form(&mut self, ui: &mut Ui, toast: &mut ToastManager) {
        Grid::new("emul_order_form")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label("合约代码:");
                ui.text_edit_singleline(&mut self.emul_form.symbol);
                ui.end_row();

                ui.label("交易所:");
                exchange_combo(ui, &mut self.emul_form.exchange);
                ui.end_row();

                ui.label("方向:");
                direction_combo(ui, &mut self.emul_form.direction);
                ui.end_row();

                ui.label("委托类型:");
                emul_type_combo(ui, &mut self.emul_form.order_type);
                ui.end_row();

                ui.label("数量:");
                ui.text_edit_singleline(&mut self.emul_form.volume);
                ui.end_row();

                // Trigger price for StopLimit / MIT / LIT
                if matches!(
                    self.emul_form.order_type,
                    EmulatedOrderType::StopLimit
                        | EmulatedOrderType::Mit
                        | EmulatedOrderType::Lit
                ) {
                    ui.label("触发价:");
                    ui.text_edit_singleline(&mut self.emul_form.trigger_price);
                    ui.end_row();
                }

                // Limit price for StopLimit / LIT
                if matches!(
                    self.emul_form.order_type,
                    EmulatedOrderType::StopLimit | EmulatedOrderType::Lit
                ) {
                    ui.label("限价:");
                    ui.text_edit_singleline(&mut self.emul_form.limit_price);
                    ui.end_row();
                }

                // Iceberg-specific
                if self.emul_form.order_type == EmulatedOrderType::Iceberg {
                    ui.label("可见数量:");
                    ui.text_edit_singleline(&mut self.emul_form.visible_volume);
                    ui.end_row();

                    ui.label("委托价:");
                    ui.text_edit_singleline(&mut self.emul_form.iceberg_price);
                    ui.end_row();
                }

                // TrailingStopPct
                if self.emul_form.order_type == EmulatedOrderType::TrailingStopPct {
                    ui.label("追踪百分比:");
                    ui.text_edit_singleline(&mut self.emul_form.trail_pct);
                    ui.end_row();
                }

                // TrailingStopAbs
                if self.emul_form.order_type == EmulatedOrderType::TrailingStopAbs {
                    ui.label("追踪距离:");
                    ui.text_edit_singleline(&mut self.emul_form.trail_abs);
                    ui.end_row();
                }

                ui.label("开平:");
                offset_combo(ui, &mut self.emul_form.offset);
                ui.end_row();

                ui.label("网关:");
                ui.text_edit_singleline(&mut self.emul_form.gateway_name);
                ui.end_row();
            });

        ui.add_space(4.0);
        if ui.button("提交模拟委托").clicked() {
            self.submit_emul_order(toast);
        }
    }

    fn submit_emul_order(&mut self, toast: &mut ToastManager) {
        let volume = match self.emul_form.volume.parse::<f64>() {
            Ok(v) if v > 0.0 => v,
            _ => {
                toast.add("数量必须大于0", ToastType::Error);
                return;
            }
        };

        let parse_opt_f64 = |s: &str| -> Option<f64> {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed.parse::<f64>().ok().filter(|&v| v > 0.0)
            }
        };

        let req = EmulatedOrderRequest {
            order_type: self.emul_form.order_type,
            symbol: self.emul_form.symbol.trim().to_string(),
            exchange: self.emul_form.exchange,
            direction: self.emul_form.direction,
            offset: self.emul_form.offset,
            volume,
            trail_pct: parse_opt_f64(&self.emul_form.trail_pct),
            trail_abs: parse_opt_f64(&self.emul_form.trail_abs),
            trigger_price: parse_opt_f64(&self.emul_form.trigger_price),
            limit_price: parse_opt_f64(&self.emul_form.limit_price),
            visible_volume: parse_opt_f64(&self.emul_form.visible_volume),
            iceberg_price: parse_opt_f64(&self.emul_form.iceberg_price),
            expires_at: None,
            gateway_name: self.emul_form.gateway_name.trim().to_string(),
            reference: String::new(),
        };

        self.pending_emul_request = Some(req);
        toast.add("模拟委托已提交", ToastType::Info);
    }

    fn render_emul_table(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                use egui_extras::Column;

                egui_extras::TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(40.0))   // ID
                    .column(Column::remainder().at_least(80.0))  // 合约
                    .column(Column::exact(70.0))   // 类型
                    .column(Column::exact(40.0))   // 方向
                    .column(Column::exact(60.0))   // 数量
                    .column(Column::exact(70.0))   // 价格
                    .column(Column::exact(60.0))   // 状态
                    .column(Column::exact(50.0))   // 操作
                    .header(20.0, |mut header| {
                        header.col(|ui| { ui.strong("ID"); });
                        header.col(|ui| { ui.strong("合约"); });
                        header.col(|ui| { ui.strong("类型"); });
                        header.col(|ui| { ui.strong("方向"); });
                        header.col(|ui| { ui.strong("数量"); });
                        header.col(|ui| { ui.strong("价格"); });
                        header.col(|ui| { ui.strong("状态"); });
                        header.col(|ui| { ui.strong("操作"); });
                    })
                    .body(|mut body| {
                        for order in &self.emul_orders_cache {
                            body.row(18.0, |mut row| {
                                row.col(|ui| { ui.label(format!("{}", order.id)); });
                                row.col(|ui| { ui.label(&order.vt_symbol()); });
                                row.col(|ui| {
                                    let label = emul_type_label(&order.order_type);
                                    ui.label(label);
                                });
                                row.col(|ui| {
                                    let color = if order.direction == Direction::Long {
                                        COLOR_LONG
                                    } else {
                                        COLOR_SHORT
                                    };
                                    ui.colored_label(color, format!("{}", order.direction));
                                });
                                row.col(|ui| { ui.label(format!("{}", order.volume)); });
                                row.col(|ui| {
                                    let price = order.trigger_price
                                        .or(order.iceberg_price)
                                        .or(order.limit_price)
                                        .unwrap_or(0.0);
                                    if price > 0.0 {
                                        ui.label(format!("{:.2}", price));
                                    } else {
                                        ui.label(RichText::new("市价").italics());
                                    }
                                });
                                row.col(|ui| {
                                    use crate::trader::order_emulator::EmulatedOrderStatus;
                                    let text = format!("{}", order.status);
                                    let color = match order.status {
                                        EmulatedOrderStatus::Pending => COLOR_LONG,
                                        EmulatedOrderStatus::Triggered => COLOR_SHORT,
                                        EmulatedOrderStatus::Cancelled | EmulatedOrderStatus::Expired => COLOR_TEXT_SECONDARY,
                                        EmulatedOrderStatus::Rejected => super::style::COLOR_NEGATIVE,
                                        EmulatedOrderStatus::Completed => COLOR_SHORT,
                                    };
                                    ui.colored_label(color, text);
                                });
                                row.col(|ui| {
                                    if order.is_active() {
                                        if ui.button("撤").clicked() {
                                            self.pending_cancel_emul = Some(order.id);
                                        }
                                    }
                                });
                            });
                        }
                    });
            });
    }

    // -----------------------------------------------------------------------
    // Pending requests (consumed by MainWindow)
    // -----------------------------------------------------------------------

    /// Take the pending stop order request.
    pub fn take_stop_request(&mut self) -> Option<StopOrderRequest> {
        self.pending_stop_request.take()
    }

    /// Take the pending emulated order request.
    pub fn take_emul_request(&mut self) -> Option<EmulatedOrderRequest> {
        self.pending_emul_request.take()
    }
}

// ===========================================================================
// Helper combo boxes
// ===========================================================================

fn exchange_combo(ui: &mut Ui, exchange: &mut Exchange) {
    let label = match exchange {
        Exchange::Binance => "Binance",
        Exchange::BinanceUsdm => "BinanceUsdm",
        Exchange::BinanceCoinm => "BinanceCoinm",
        Exchange::Okx => "OKX",
        Exchange::Bybit => "Bybit",
        Exchange::Local => "Local",
        _ => "Other",
    };
    egui::ComboBox::from_id_salt("exchange_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(exchange, Exchange::Binance, "Binance");
            ui.selectable_value(exchange, Exchange::BinanceUsdm, "BinanceUsdm");
            ui.selectable_value(exchange, Exchange::BinanceCoinm, "BinanceCoinm");
            ui.selectable_value(exchange, Exchange::Okx, "OKX");
            ui.selectable_value(exchange, Exchange::Bybit, "Bybit");
            ui.selectable_value(exchange, Exchange::Local, "Local");
        });
}

fn direction_combo(ui: &mut Ui, dir: &mut Direction) {
    let label = match dir {
        Direction::Long => "多 (Long)",
        Direction::Short => "空 (Short)",
        Direction::Net => "净 (Net)",
    };
    egui::ComboBox::from_id_salt("direction_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(dir, Direction::Long, "多 (Long)");
            ui.selectable_value(dir, Direction::Short, "空 (Short)");
            ui.selectable_value(dir, Direction::Net, "净 (Net)");
        });
}

fn offset_combo(ui: &mut Ui, offset: &mut Offset) {
    let label = match offset {
        Offset::None => "无",
        Offset::Open => "开",
        Offset::Close => "平",
        Offset::CloseToday => "平今",
        Offset::CloseYesterday => "平昨",
    };
    egui::ComboBox::from_id_salt("offset_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(offset, Offset::None, "无");
            ui.selectable_value(offset, Offset::Open, "开");
            ui.selectable_value(offset, Offset::Close, "平");
            ui.selectable_value(offset, Offset::CloseToday, "平今");
            ui.selectable_value(offset, Offset::CloseYesterday, "平昨");
        });
}

fn stop_type_combo(ui: &mut Ui, st: &mut StopOrderType) {
    let label = stop_type_label(*st);
    egui::ComboBox::from_id_salt("stop_type_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(st, StopOrderType::StopMarket, stop_type_label(StopOrderType::StopMarket));
            ui.selectable_value(st, StopOrderType::StopLimit, stop_type_label(StopOrderType::StopLimit));
            ui.selectable_value(st, StopOrderType::TakeProfit, stop_type_label(StopOrderType::TakeProfit));
            ui.selectable_value(st, StopOrderType::TrailingStopPct, stop_type_label(StopOrderType::TrailingStopPct));
            ui.selectable_value(st, StopOrderType::TrailingStopAbs, stop_type_label(StopOrderType::TrailingStopAbs));
        });
}

fn stop_type_label(st: StopOrderType) -> &'static str {
    match st {
        StopOrderType::StopMarket => "止损",
        StopOrderType::StopLimit => "止损限价",
        StopOrderType::TakeProfit => "止盈",
        StopOrderType::TrailingStopPct => "追踪止损%",
        StopOrderType::TrailingStopAbs => "追踪止损",
    }
}

fn emul_type_combo(ui: &mut Ui, ot: &mut EmulatedOrderType) {
    let label = emul_type_label(ot);
    egui::ComboBox::from_id_salt("emul_type_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(ot, EmulatedOrderType::Iceberg, emul_type_label(&EmulatedOrderType::Iceberg));
            ui.selectable_value(ot, EmulatedOrderType::StopLimit, emul_type_label(&EmulatedOrderType::StopLimit));
            ui.selectable_value(ot, EmulatedOrderType::Mit, emul_type_label(&EmulatedOrderType::Mit));
            ui.selectable_value(ot, EmulatedOrderType::Lit, emul_type_label(&EmulatedOrderType::Lit));
            ui.selectable_value(ot, EmulatedOrderType::TrailingStopPct, emul_type_label(&EmulatedOrderType::TrailingStopPct));
            ui.selectable_value(ot, EmulatedOrderType::TrailingStopAbs, emul_type_label(&EmulatedOrderType::TrailingStopAbs));
        });
}

fn emul_type_label(ot: &EmulatedOrderType) -> &'static str {
    match ot {
        EmulatedOrderType::Iceberg => "冰山",
        EmulatedOrderType::StopLimit => "止损限价",
        EmulatedOrderType::Mit => "触价",
        EmulatedOrderType::Lit => "触价限价",
        EmulatedOrderType::TrailingStopPct => "追踪止损%",
        EmulatedOrderType::TrailingStopAbs => "追踪止损",
    }
}
