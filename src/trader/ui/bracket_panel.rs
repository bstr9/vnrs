//! Bracket/OCO/OTO Order Panel
//!
//! GUI panel for managing contingent orders: Bracket (entry + TP + SL),
//! OCO (one-cancels-other), and OTO (one-triggers-other).

use egui::{Color32, ComboBox, Grid, RichText, ScrollArea, Ui};
use std::sync::Arc;

use super::style::{COLOR_ASK, COLOR_BID, ToastManager, ToastType};
use crate::trader::bracket_order::{
    BracketOrderRequest, ContingencyType, OcoOrderRequest, OrderGroup, OrderGroupState,
    OtoOrderRequest,
};
use crate::trader::constant::{Direction, Exchange, Offset, OrderType};
use crate::trader::BracketOrderEngine;

/// Tab selection for bracket order panel
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BracketTab {
    #[default]
    Bracket,
    Oco,
    Oto,
}

/// Bracket order panel state
pub struct BracketOrderPanel {
    // Tab selection
    current_tab: BracketTab,

    // --- Common fields ---
    symbol: String,
    exchange_index: usize,
    gateway_index: usize,
    gateways: Vec<String>,

    // --- Bracket order fields ---
    bracket_direction_index: usize,
    bracket_entry_type_index: usize,
    bracket_entry_price: String,
    bracket_entry_volume: String,
    bracket_offset_index: usize,
    bracket_tp_price: String,
    bracket_sl_type_index: usize,
    bracket_sl_price: String,
    bracket_reference: String,
    bracket_tag: String,

    // --- OCO order fields ---
    oco_direction_index: usize,
    oco_volume: String,
    oco_order_a_type_index: usize,
    oco_order_a_price: String,
    oco_order_b_type_index: usize,
    oco_order_b_price: String,
    oco_offset_index: usize,
    oco_reference: String,
    oco_tag: String,

    // --- OTO order fields ---
    oto_primary_direction_index: usize,
    oto_primary_type_index: usize,
    oto_primary_price: String,
    oto_primary_volume: String,
    oto_secondary_direction_index: usize,
    oto_secondary_type_index: usize,
    oto_secondary_price: String,
    oto_secondary_volume: String,
    oto_offset_index: usize,
    oto_reference: String,
    oto_tag: String,

    // Exchanges list
    exchanges: Vec<Exchange>,

    // Pending cancel request
    pending_cancel: Option<u64>,

    // Last error message
    last_error: Option<String>,
}

impl Default for BracketOrderPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl BracketOrderPanel {
    pub fn new() -> Self {
        Self {
            current_tab: BracketTab::default(),
            symbol: String::new(),
            exchange_index: 0,
            gateway_index: 0,
            gateways: Vec::new(),
            bracket_direction_index: 0,
            bracket_entry_type_index: 0,
            bracket_entry_price: String::new(),
            bracket_entry_volume: String::new(),
            bracket_offset_index: 0,
            bracket_tp_price: String::new(),
            bracket_sl_type_index: 0,
            bracket_sl_price: String::new(),
            bracket_reference: String::new(),
            bracket_tag: String::new(),
            oco_direction_index: 0,
            oco_volume: String::new(),
            oco_order_a_type_index: 0,
            oco_order_a_price: String::new(),
            oco_order_b_type_index: 0,
            oco_order_b_price: String::new(),
            oco_offset_index: 0,
            oco_reference: String::new(),
            oco_tag: String::new(),
            oto_primary_direction_index: 0,
            oto_primary_type_index: 0,
            oto_primary_price: String::new(),
            oto_primary_volume: String::new(),
            oto_secondary_direction_index: 0,
            oto_secondary_type_index: 0,
            oto_secondary_price: String::new(),
            oto_secondary_volume: String::new(),
            oto_offset_index: 0,
            oto_reference: String::new(),
            oto_tag: String::new(),
            exchanges: vec![
                Exchange::Binance,
                Exchange::BinanceUsdm,
                Exchange::BinanceCoinm,
                Exchange::Okx,
                Exchange::Bybit,
            ],
            pending_cancel: None,
            last_error: None,
        }
    }

    /// Set available gateways
    pub fn set_gateways(&mut self, gateways: Vec<String>) {
        self.gateways = gateways;
    }

    /// Take pending cancel request
    pub fn take_cancel(&mut self) -> Option<u64> {
        self.pending_cancel.take()
    }

    /// Render the panel
    pub fn show(&mut self, ui: &mut Ui, engine: Option<&Arc<BracketOrderEngine>>, toast_manager: &mut ToastManager) {
        // Show any error from last submission
        if let Some(ref err) = self.last_error {
            ui.colored_label(Color32::RED, format!("错误: {}", err));
            ui.add_space(4.0);
        }

        // Tab selection
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.current_tab, BracketTab::Bracket, "组合单");
            ui.selectable_value(&mut self.current_tab, BracketTab::Oco, "OCO");
            ui.selectable_value(&mut self.current_tab, BracketTab::Oto, "OTO");
        });
        ui.separator();

        // Tab content
        match self.current_tab {
            BracketTab::Bracket => self.show_bracket_tab(ui, engine, toast_manager),
            BracketTab::Oco => self.show_oco_tab(ui, engine, toast_manager),
            BracketTab::Oto => self.show_oto_tab(ui, engine, toast_manager),
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // Active orders table
        self.show_active_orders(ui, engine, toast_manager);
    }

    fn show_bracket_tab(&mut self, ui: &mut Ui, engine: Option<&Arc<BracketOrderEngine>>, toast_manager: &mut ToastManager) {
        ui.heading("组合单委托");
        ui.label("入场 + 止盈 + 止损");
        ui.add_space(8.0);

        Grid::new("bracket_order_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                // Symbol
                ui.label("合约代码");
                ui.text_edit_singleline(&mut self.symbol);
                ui.end_row();

                // Exchange
                ui.label("交易所");
                let exchange_text = self
                    .exchanges
                    .get(self.exchange_index)
                    .map(|e| e.value())
                    .unwrap_or("BINANCE");
                ComboBox::from_id_salt("bracket_exchange")
                    .selected_text(exchange_text)
                    .show_ui(ui, |ui| {
                        for (i, exchange) in self.exchanges.iter().enumerate() {
                            ui.selectable_value(&mut self.exchange_index, i, exchange.value());
                        }
                    });
                ui.end_row();

                // Direction
                ui.label("方向");
                let directions = ["多", "空"];
                let dir_text = directions[self.bracket_direction_index];
                ComboBox::from_id_salt("bracket_direction")
                    .selected_text(dir_text)
                    .show_ui(ui, |ui| {
                        for (i, d) in directions.iter().enumerate() {
                            ui.selectable_value(&mut self.bracket_direction_index, i, *d);
                        }
                    });
                ui.end_row();

                // Entry type
                ui.label("入场类型");
                let entry_types = ["限价", "市价"];
                let entry_type_text = entry_types[self.bracket_entry_type_index];
                ComboBox::from_id_salt("bracket_entry_type")
                    .selected_text(entry_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in entry_types.iter().enumerate() {
                            ui.selectable_value(&mut self.bracket_entry_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Entry price
                ui.label("入场价格");
                ui.text_edit_singleline(&mut self.bracket_entry_price);
                ui.end_row();

                // Entry volume
                ui.label("委托数量");
                ui.text_edit_singleline(&mut self.bracket_entry_volume);
                ui.end_row();

                // Offset
                ui.label("开平");
                let offsets = ["自动", "开仓", "平仓", "平今", "平昨"];
                let offset_text = offsets[self.bracket_offset_index];
                ComboBox::from_id_salt("bracket_offset")
                    .selected_text(offset_text)
                    .show_ui(ui, |ui| {
                        for (i, o) in offsets.iter().enumerate() {
                            ui.selectable_value(&mut self.bracket_offset_index, i, *o);
                        }
                    });
                ui.end_row();

                // Take profit price
                ui.label("止盈价格");
                ui.text_edit_singleline(&mut self.bracket_tp_price);
                ui.end_row();

                // Stop loss type
                ui.label("止损类型");
                let sl_types = ["止损市价", "止损限价"];
                let sl_type_text = sl_types[self.bracket_sl_type_index];
                ComboBox::from_id_salt("bracket_sl_type")
                    .selected_text(sl_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in sl_types.iter().enumerate() {
                            ui.selectable_value(&mut self.bracket_sl_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Stop loss price
                ui.label("止损价格");
                ui.text_edit_singleline(&mut self.bracket_sl_price);
                ui.end_row();

                // Gateway
                ui.label("网关");
                let gateway_text = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
                ComboBox::from_id_salt("bracket_gateway")
                    .selected_text(&gateway_text)
                    .show_ui(ui, |ui| {
                        for (i, g) in self.gateways.iter().enumerate() {
                            ui.selectable_value(&mut self.gateway_index, i, g);
                        }
                    });
                ui.end_row();

                // Reference
                ui.label("引用");
                ui.text_edit_singleline(&mut self.bracket_reference);
                ui.end_row();

                // Tag
                ui.label("标签");
                ui.text_edit_singleline(&mut self.bracket_tag);
                ui.end_row();
            });

        ui.add_space(10.0);

        // Submit button
        let is_long = self.bracket_direction_index == 0;
        let btn_color = if is_long { COLOR_ASK } else { COLOR_BID };
        let btn_text = RichText::new("提交组合单").color(btn_color);
        if ui.button(btn_text).clicked() {
            if let Some(engine) = engine {
                self.submit_bracket_order(engine, toast_manager);
            } else {
                self.last_error = Some("引擎未连接".to_string());
            }
        }
    }

    fn show_oco_tab(&mut self, ui: &mut Ui, engine: Option<&Arc<BracketOrderEngine>>, toast_manager: &mut ToastManager) {
        ui.heading("OCO 委托");
        ui.label("一单成交则撤销另一单");
        ui.add_space(8.0);

        Grid::new("oco_order_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                // Symbol
                ui.label("合约代码");
                ui.text_edit_singleline(&mut self.symbol);
                ui.end_row();

                // Exchange
                ui.label("交易所");
                let exchange_text = self
                    .exchanges
                    .get(self.exchange_index)
                    .map(|e| e.value())
                    .unwrap_or("BINANCE");
                ComboBox::from_id_salt("oco_exchange")
                    .selected_text(exchange_text)
                    .show_ui(ui, |ui| {
                        for (i, exchange) in self.exchanges.iter().enumerate() {
                            ui.selectable_value(&mut self.exchange_index, i, exchange.value());
                        }
                    });
                ui.end_row();

                // Direction
                ui.label("方向");
                let directions = ["多", "空"];
                let dir_text = directions[self.oco_direction_index];
                ComboBox::from_id_salt("oco_direction")
                    .selected_text(dir_text)
                    .show_ui(ui, |ui| {
                        for (i, d) in directions.iter().enumerate() {
                            ui.selectable_value(&mut self.oco_direction_index, i, *d);
                        }
                    });
                ui.end_row();

                // Volume
                ui.label("委托数量");
                ui.text_edit_singleline(&mut self.oco_volume);
                ui.end_row();

                ui.separator();
                ui.label("委托 A");
                ui.separator();
                ui.end_row();

                // Order A type
                ui.label("类型A");
                let order_types = ["限价", "市价"];
                let order_a_type_text = order_types[self.oco_order_a_type_index];
                ComboBox::from_id_salt("oco_order_a_type")
                    .selected_text(order_a_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in order_types.iter().enumerate() {
                            ui.selectable_value(&mut self.oco_order_a_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Order A price
                ui.label("价格A");
                ui.text_edit_singleline(&mut self.oco_order_a_price);
                ui.end_row();

                ui.separator();
                ui.label("委托 B");
                ui.separator();
                ui.end_row();

                // Order B type
                ui.label("类型B");
                let order_b_type_text = order_types[self.oco_order_b_type_index];
                ComboBox::from_id_salt("oco_order_b_type")
                    .selected_text(order_b_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in order_types.iter().enumerate() {
                            ui.selectable_value(&mut self.oco_order_b_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Order B price
                ui.label("价格B");
                ui.text_edit_singleline(&mut self.oco_order_b_price);
                ui.end_row();

                ui.separator();
                ui.end_row();

                // Offset
                ui.label("开平");
                let offsets = ["自动", "开仓", "平仓", "平今", "平昨"];
                let offset_text = offsets[self.oco_offset_index];
                ComboBox::from_id_salt("oco_offset")
                    .selected_text(offset_text)
                    .show_ui(ui, |ui| {
                        for (i, o) in offsets.iter().enumerate() {
                            ui.selectable_value(&mut self.oco_offset_index, i, *o);
                        }
                    });
                ui.end_row();

                // Gateway
                ui.label("网关");
                let gateway_text = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
                ComboBox::from_id_salt("oco_gateway")
                    .selected_text(&gateway_text)
                    .show_ui(ui, |ui| {
                        for (i, g) in self.gateways.iter().enumerate() {
                            ui.selectable_value(&mut self.gateway_index, i, g);
                        }
                    });
                ui.end_row();

                // Reference
                ui.label("引用");
                ui.text_edit_singleline(&mut self.oco_reference);
                ui.end_row();

                // Tag
                ui.label("标签");
                ui.text_edit_singleline(&mut self.oco_tag);
                ui.end_row();
            });

        ui.add_space(10.0);

        // Submit button
        if ui.button("提交 OCO").clicked() {
            if let Some(engine) = engine {
                self.submit_oco_order(engine, toast_manager);
            } else {
                self.last_error = Some("引擎未连接".to_string());
            }
        }
    }

    fn show_oto_tab(&mut self, ui: &mut Ui, engine: Option<&Arc<BracketOrderEngine>>, toast_manager: &mut ToastManager) {
        ui.heading("OTO 委托");
        ui.label("主委托成交后触发次委托");
        ui.add_space(8.0);

        Grid::new("oto_order_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                // Symbol
                ui.label("合约代码");
                ui.text_edit_singleline(&mut self.symbol);
                ui.end_row();

                // Exchange
                ui.label("交易所");
                let exchange_text = self
                    .exchanges
                    .get(self.exchange_index)
                    .map(|e| e.value())
                    .unwrap_or("BINANCE");
                ComboBox::from_id_salt("oto_exchange")
                    .selected_text(exchange_text)
                    .show_ui(ui, |ui| {
                        for (i, exchange) in self.exchanges.iter().enumerate() {
                            ui.selectable_value(&mut self.exchange_index, i, exchange.value());
                        }
                    });
                ui.end_row();

                ui.separator();
                ui.label("主委托");
                ui.separator();
                ui.end_row();

                // Primary direction
                ui.label("方向");
                let directions = ["多", "空"];
                let primary_dir_text = directions[self.oto_primary_direction_index];
                ComboBox::from_id_salt("oto_primary_direction")
                    .selected_text(primary_dir_text)
                    .show_ui(ui, |ui| {
                        for (i, d) in directions.iter().enumerate() {
                            ui.selectable_value(&mut self.oto_primary_direction_index, i, *d);
                        }
                    });
                ui.end_row();

                // Primary type
                ui.label("类型");
                let order_types = ["限价", "市价"];
                let primary_type_text = order_types[self.oto_primary_type_index];
                ComboBox::from_id_salt("oto_primary_type")
                    .selected_text(primary_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in order_types.iter().enumerate() {
                            ui.selectable_value(&mut self.oto_primary_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Primary price
                ui.label("价格");
                ui.text_edit_singleline(&mut self.oto_primary_price);
                ui.end_row();

                // Primary volume
                ui.label("数量");
                ui.text_edit_singleline(&mut self.oto_primary_volume);
                ui.end_row();

                ui.separator();
                ui.label("次委托");
                ui.separator();
                ui.end_row();

                // Secondary direction
                ui.label("方向");
                let secondary_dir_text = directions[self.oto_secondary_direction_index];
                ComboBox::from_id_salt("oto_secondary_direction")
                    .selected_text(secondary_dir_text)
                    .show_ui(ui, |ui| {
                        for (i, d) in directions.iter().enumerate() {
                            ui.selectable_value(&mut self.oto_secondary_direction_index, i, *d);
                        }
                    });
                ui.end_row();

                // Secondary type
                ui.label("类型");
                let secondary_type_text = order_types[self.oto_secondary_type_index];
                ComboBox::from_id_salt("oto_secondary_type")
                    .selected_text(secondary_type_text)
                    .show_ui(ui, |ui| {
                        for (i, t) in order_types.iter().enumerate() {
                            ui.selectable_value(&mut self.oto_secondary_type_index, i, *t);
                        }
                    });
                ui.end_row();

                // Secondary price
                ui.label("价格");
                ui.text_edit_singleline(&mut self.oto_secondary_price);
                ui.end_row();

                // Secondary volume
                ui.label("数量");
                ui.text_edit_singleline(&mut self.oto_secondary_volume);
                ui.end_row();

                ui.separator();
                ui.end_row();

                // Offset
                ui.label("开平");
                let offsets = ["自动", "开仓", "平仓", "平今", "平昨"];
                let offset_text = offsets[self.oto_offset_index];
                ComboBox::from_id_salt("oto_offset")
                    .selected_text(offset_text)
                    .show_ui(ui, |ui| {
                        for (i, o) in offsets.iter().enumerate() {
                            ui.selectable_value(&mut self.oto_offset_index, i, *o);
                        }
                    });
                ui.end_row();

                // Gateway
                ui.label("网关");
                let gateway_text = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
                ComboBox::from_id_salt("oto_gateway")
                    .selected_text(&gateway_text)
                    .show_ui(ui, |ui| {
                        for (i, g) in self.gateways.iter().enumerate() {
                            ui.selectable_value(&mut self.gateway_index, i, g);
                        }
                    });
                ui.end_row();

                // Reference
                ui.label("引用");
                ui.text_edit_singleline(&mut self.oto_reference);
                ui.end_row();

                // Tag
                ui.label("标签");
                ui.text_edit_singleline(&mut self.oto_tag);
                ui.end_row();
            });

        ui.add_space(10.0);

        // Submit button
        if ui.button("提交 OTO").clicked() {
            if let Some(engine) = engine {
                self.submit_oto_order(engine, toast_manager);
            } else {
                self.last_error = Some("引擎未连接".to_string());
            }
        }
    }

    fn show_active_orders(&mut self, ui: &mut Ui, engine: Option<&Arc<BracketOrderEngine>>, toast_manager: &mut ToastManager) {
        ui.heading("活跃委托组");
        ui.add_space(8.0);

        let groups: Vec<OrderGroup> = engine
            .map(|e| e.get_all_groups())
            .unwrap_or_default();

        if groups.is_empty() {
            ui.label("暂无活跃委托组");
            return;
        }

        ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            egui_extras::TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(egui_extras::Column::auto().at_least(60.0))  // GroupID
                .column(egui_extras::Column::auto().at_least(70.0))  // Type
                .column(egui_extras::Column::auto().at_least(120.0)) // Symbol
                .column(egui_extras::Column::auto().at_least(80.0))  // State
                .column(egui_extras::Column::auto().at_least(80.0))  // Entry/OrderA
                .column(egui_extras::Column::auto().at_least(80.0))  // TP/OrderB
                .column(egui_extras::Column::auto().at_least(80.0))  // SL
                .column(egui_extras::Column::auto().at_least(50.0))  // Cancel
                .min_scrolled_height(100.0)
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("组ID"); });
                    header.col(|ui| { ui.strong("类型"); });
                    header.col(|ui| { ui.strong("合约"); });
                    header.col(|ui| { ui.strong("状态"); });
                    header.col(|ui| { ui.strong("入场/A"); });
                    header.col(|ui| { ui.strong("止盈/B"); });
                    header.col(|ui| { ui.strong("止损"); });
                    header.col(|ui| { ui.strong("操作"); });
                })
                .body(|mut body| {
                    for group in groups {
                        body.row(18.0, |mut row| {
                            // Group ID
                            row.col(|ui| {
                                ui.label(format!("#{}", group.id));
                            });

                            // Type
                            row.col(|ui| {
                                let type_text = match group.contingency_type {
                                    ContingencyType::Bracket => "组合单",
                                    ContingencyType::Oco => "OCO",
                                    ContingencyType::Oto => "OTO",
                                };
                                ui.label(type_text);
                            });

                            // Symbol
                            row.col(|ui| {
                                ui.label(&group.vt_symbol);
                            });

                            // State
                            row.col(|ui| {
                                let (state_text, state_color) = match group.state {
                                    OrderGroupState::Pending => ("待提交", Color32::GRAY),
                                    OrderGroupState::EntryActive => ("入场中", Color32::from_rgb(100, 150, 255)),
                                    OrderGroupState::SecondaryActive => ("出场中", Color32::from_rgb(255, 200, 50)),
                                    OrderGroupState::Completed => ("已完成", Color32::GREEN),
                                    OrderGroupState::Cancelled => ("已取消", Color32::GRAY),
                                    OrderGroupState::Rejected => ("已拒绝", Color32::RED),
                                };
                                ui.label(RichText::new(state_text).color(state_color));
                            });

                            // Entry/OrderA status
                            row.col(|ui| {
                                if let Some(entry) = group.orders.get("Entry") {
                                    self.show_order_status(ui, &entry.status);
                                } else if let Some(order_a) = group.orders.get("OrderA") {
                                    self.show_order_status(ui, &order_a.status);
                                } else if let Some(primary) = group.orders.get("Primary") {
                                    self.show_order_status(ui, &primary.status);
                                } else {
                                    ui.label("-");
                                }
                            });

                            // TP/OrderB status
                            row.col(|ui| {
                                if let Some(tp) = group.orders.get("TakeProfit") {
                                    self.show_order_status(ui, &tp.status);
                                } else if let Some(order_b) = group.orders.get("OrderB") {
                                    self.show_order_status(ui, &order_b.status);
                                } else if let Some(secondary) = group.orders.get("Secondary") {
                                    self.show_order_status(ui, &secondary.status);
                                } else {
                                    ui.label("-");
                                }
                            });

                            // SL status
                            row.col(|ui| {
                                if let Some(sl) = group.orders.get("StopLoss") {
                                    self.show_order_status(ui, &sl.status);
                                } else {
                                    ui.label("-");
                                }
                            });

                            // Cancel button
                            row.col(|ui| {
                                if group.is_active() {
                                    if ui.small_button("撤销").clicked() {
                                        self.pending_cancel = Some(group.id);
                                        toast_manager.add(&format!("撤销委托组 #{}", group.id), ToastType::Info);
                                    }
                                } else {
                                    ui.label("-");
                                }
                            });
                        });
                    }
                });
        });
    }

    fn show_order_status(&self, ui: &mut Ui, status: &crate::trader::constant::Status) {
        use crate::trader::constant::Status;
        let (text, color) = match status {
            Status::Submitting => ("提交中", Color32::GRAY),
            Status::NotTraded => ("未成交", Color32::from_rgb(100, 150, 255)),
            Status::PartTraded => ("部分成交", Color32::from_rgb(255, 200, 50)),
            Status::AllTraded => ("全部成交", Color32::GREEN),
            Status::Cancelled => ("已撤销", Color32::GRAY),
            Status::Rejected => ("已拒绝", Color32::RED),
        };
        ui.label(RichText::new(text).color(color));
    }

    fn submit_bracket_order(&mut self, engine: &BracketOrderEngine, toast_manager: &mut ToastManager) {
        // Parse and validate
        let symbol = self.symbol.trim().to_string();
        if symbol.is_empty() {
            self.last_error = Some("合约代码不能为空".to_string());
            return;
        }

        let entry_volume: f64 = match self.bracket_entry_volume.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("委托数量必须大于零".to_string());
                return;
            }
        };

        let entry_price: f64 = self.bracket_entry_price.trim().parse().unwrap_or(0.0);
        let entry_type = if self.bracket_entry_type_index == 0 {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        if entry_type == OrderType::Limit && entry_price <= 0.0 {
            self.last_error = Some("限价单入场价格必须大于零".to_string());
            return;
        }

        let tp_price: f64 = match self.bracket_tp_price.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("止盈价格必须大于零".to_string());
                return;
            }
        };

        let sl_price: f64 = match self.bracket_sl_price.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("止损价格必须大于零".to_string());
                return;
            }
        };

        let sl_type = if self.bracket_sl_type_index == 0 {
            OrderType::Stop
        } else {
            OrderType::StopLimit
        };

        let direction = if self.bracket_direction_index == 0 {
            Direction::Long
        } else {
            Direction::Short
        };

        let exchange = self.exchanges.get(self.exchange_index).cloned().unwrap_or(Exchange::Binance);
        let offset = match self.bracket_offset_index {
            1 => Offset::Open,
            2 => Offset::Close,
            3 => Offset::CloseToday,
            4 => Offset::CloseYesterday,
            _ => Offset::None,
        };

        let gateway_name = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
        if gateway_name.is_empty() {
            self.last_error = Some("请选择网关".to_string());
            return;
        }

        let req = BracketOrderRequest {
            symbol,
            exchange,
            direction,
            entry_price,
            entry_volume,
            entry_type,
            tp_price,
            sl_price,
            sl_type,
            offset,
            gateway_name,
            reference: self.bracket_reference.trim().to_string(),
            tag: self.bracket_tag.trim().to_string(),
        };

        match engine.add_bracket_order(req) {
            Ok(id) => {
                self.last_error = None;
                toast_manager.add(&format!("组合单 #{} 已提交", id), ToastType::Success);
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }

    fn submit_oco_order(&mut self, engine: &BracketOrderEngine, toast_manager: &mut ToastManager) {
        // Parse and validate
        let symbol = self.symbol.trim().to_string();
        if symbol.is_empty() {
            self.last_error = Some("合约代码不能为空".to_string());
            return;
        }

        let volume: f64 = match self.oco_volume.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("委托数量必须大于零".to_string());
                return;
            }
        };

        let order_a_price: f64 = match self.oco_order_a_price.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("订单A价格必须大于零".to_string());
                return;
            }
        };

        let order_b_price: f64 = match self.oco_order_b_price.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("订单B价格必须大于零".to_string());
                return;
            }
        };

        let order_a_type = if self.oco_order_a_type_index == 0 {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        let order_b_type = if self.oco_order_b_type_index == 0 {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        let direction = if self.oco_direction_index == 0 {
            Direction::Long
        } else {
            Direction::Short
        };

        let exchange = self.exchanges.get(self.exchange_index).cloned().unwrap_or(Exchange::Binance);
        let offset = match self.oco_offset_index {
            1 => Offset::Open,
            2 => Offset::Close,
            3 => Offset::CloseToday,
            4 => Offset::CloseYesterday,
            _ => Offset::None,
        };

        let gateway_name = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
        if gateway_name.is_empty() {
            self.last_error = Some("请选择网关".to_string());
            return;
        }

        let req = OcoOrderRequest {
            symbol,
            exchange,
            direction,
            volume,
            order_a_price,
            order_a_type,
            order_b_price,
            order_b_type,
            offset,
            gateway_name,
            reference: self.oco_reference.trim().to_string(),
            tag: self.oco_tag.trim().to_string(),
        };

        match engine.add_oco_order(req) {
            Ok(id) => {
                self.last_error = None;
                toast_manager.add(&format!("OCO订单 #{} 已提交", id), ToastType::Success);
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }

    fn submit_oto_order(&mut self, engine: &BracketOrderEngine, toast_manager: &mut ToastManager) {
        // Parse and validate
        let symbol = self.symbol.trim().to_string();
        if symbol.is_empty() {
            self.last_error = Some("合约代码不能为空".to_string());
            return;
        }

        let primary_volume: f64 = match self.oto_primary_volume.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("主委托数量必须大于零".to_string());
                return;
            }
        };

        let secondary_volume: f64 = match self.oto_secondary_volume.trim().parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_error = Some("次委托数量必须大于零".to_string());
                return;
            }
        };

        let primary_price: f64 = self.oto_primary_price.trim().parse().unwrap_or(0.0);
        let secondary_price: f64 = self.oto_secondary_price.trim().parse().unwrap_or(0.0);

        let primary_type = if self.oto_primary_type_index == 0 {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        let secondary_type = if self.oto_secondary_type_index == 0 {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        if primary_type == OrderType::Limit && primary_price <= 0.0 {
            self.last_error = Some("限价主委托价格必须大于零".to_string());
            return;
        }

        if secondary_type == OrderType::Limit && secondary_price <= 0.0 {
            self.last_error = Some("限价次委托价格必须大于零".to_string());
            return;
        }

        let primary_direction = if self.oto_primary_direction_index == 0 {
            Direction::Long
        } else {
            Direction::Short
        };

        let secondary_direction = if self.oto_secondary_direction_index == 0 {
            Direction::Long
        } else {
            Direction::Short
        };

        let exchange = self.exchanges.get(self.exchange_index).cloned().unwrap_or(Exchange::Binance);
        let offset = match self.oto_offset_index {
            1 => Offset::Open,
            2 => Offset::Close,
            3 => Offset::CloseToday,
            4 => Offset::CloseYesterday,
            _ => Offset::None,
        };

        let gateway_name = self.gateways.get(self.gateway_index).cloned().unwrap_or_default();
        if gateway_name.is_empty() {
            self.last_error = Some("请选择网关".to_string());
            return;
        }

        let req = OtoOrderRequest {
            symbol,
            exchange,
            primary_direction,
            primary_price,
            primary_volume,
            primary_type,
            secondary_direction,
            secondary_price,
            secondary_volume,
            secondary_type,
            offset,
            gateway_name,
            reference: self.oto_reference.trim().to_string(),
            tag: self.oto_tag.trim().to_string(),
        };

        match engine.add_oto_order(req) {
            Ok(id) => {
                self.last_error = None;
                toast_manager.add(&format!("OTO订单 #{} 已提交", id), ToastType::Success);
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }
}
