//! Trading widget for manual order entry and market depth display.

use egui::{RichText, Ui, ComboBox};

use crate::trader::constant::{Direction, Exchange, Offset, OrderType};
use crate::trader::object::{ContractData, OrderRequest, SubscribeRequest, TickData};
use crate::trader::utility::get_digits;
use super::style::*;

/// Market depth level display
#[derive(Default, Clone)]
pub struct DepthLevel {
    pub price: f64,
    pub volume: f64,
}

/// Market depth data for display
#[derive(Default, Clone)]
pub struct MarketDepth {
    pub ask_levels: [DepthLevel; 5],
    pub bid_levels: [DepthLevel; 5],
    pub last_price: f64,
    pub pre_close: f64,
}

impl MarketDepth {
    pub fn update_from_tick(&mut self, tick: &TickData) {
        self.last_price = tick.last_price;
        self.pre_close = tick.pre_close;
        
        self.bid_levels[0] = DepthLevel { price: tick.bid_price_1, volume: tick.bid_volume_1 };
        self.bid_levels[1] = DepthLevel { price: tick.bid_price_2, volume: tick.bid_volume_2 };
        self.bid_levels[2] = DepthLevel { price: tick.bid_price_3, volume: tick.bid_volume_3 };
        self.bid_levels[3] = DepthLevel { price: tick.bid_price_4, volume: tick.bid_volume_4 };
        self.bid_levels[4] = DepthLevel { price: tick.bid_price_5, volume: tick.bid_volume_5 };
        
        self.ask_levels[0] = DepthLevel { price: tick.ask_price_1, volume: tick.ask_volume_1 };
        self.ask_levels[1] = DepthLevel { price: tick.ask_price_2, volume: tick.ask_volume_2 };
        self.ask_levels[2] = DepthLevel { price: tick.ask_price_3, volume: tick.ask_volume_3 };
        self.ask_levels[3] = DepthLevel { price: tick.ask_price_4, volume: tick.ask_volume_4 };
        self.ask_levels[4] = DepthLevel { price: tick.ask_price_5, volume: tick.ask_volume_5 };
    }
    
    /// Get price change percentage
    pub fn price_change_pct(&self) -> Option<f64> {
        if self.pre_close > 0.0 {
            Some((self.last_price / self.pre_close - 1.0) * 100.0)
        } else {
            None
        }
    }
}

/// Trading widget for manual order entry
pub struct TradingWidget {
    // Symbol input
    pub symbol: String,
    pub exchange_index: usize,
    pub exchanges: Vec<Exchange>,
    
    // Contract info
    pub name: String,
    pub vt_symbol: String,
    pub price_digits: usize,
    pub contract: Option<ContractData>,
    
    // Order parameters
    pub direction_index: usize,
    pub offset_index: usize,
    pub order_type_index: usize,
    pub price: String,
    pub volume: String,
    pub gateway_index: usize,
    pub gateways: Vec<String>,
    
    // Price tracking
    pub track_price: bool,
    
    // Market depth
    pub depth: MarketDepth,
    
    // Actions
    pub pending_subscribe: Option<SubscribeRequest>,
    pub pending_order: Option<OrderRequest>,
    pub pending_cancel_all: bool,
}

impl Default for TradingWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl TradingWidget {
    pub fn new() -> Self {
        Self {
            symbol: String::new(),
            exchange_index: 0,
            exchanges: vec![
                Exchange::Binance,
                Exchange::Cffex,
                Exchange::Shfe,
                Exchange::Dce,
                Exchange::Czce,
                Exchange::Ine,
                Exchange::Sse,
                Exchange::Szse,
            ],
            name: String::new(),
            vt_symbol: String::new(),
            price_digits: 2,
            contract: None,
            direction_index: 0,
            offset_index: 0,
            order_type_index: 0,
            price: String::new(),
            volume: String::new(),
            gateway_index: 0,
            gateways: Vec::new(),
            track_price: false,
            depth: MarketDepth::default(),
            pending_subscribe: None,
            pending_order: None,
            pending_cancel_all: false,
        }
    }
    
    /// Set available gateways
    pub fn set_gateways(&mut self, gateways: Vec<String>) {
        self.gateways = gateways;
    }
    
    /// Set available exchanges
    pub fn set_exchanges(&mut self, exchanges: Vec<Exchange>) {
        self.exchanges = exchanges;
    }
    
    /// Update with contract info
    pub fn set_contract(&mut self, contract: ContractData) {
        self.name = contract.name.clone();
        self.price_digits = get_digits(contract.pricetick);
        
        // Find gateway index
        if let Some(idx) = self.gateways.iter().position(|g| g == &contract.gateway_name) {
            self.gateway_index = idx;
        }
        
        self.contract = Some(contract);
    }
    
    /// Update market depth from tick data
    pub fn update_tick(&mut self, tick: &TickData) {
        if tick.vt_symbol() != self.vt_symbol {
            return;
        }
        
        self.depth.update_from_tick(tick);
        
        // Update price if tracking
        if self.track_price {
            self.price = format!("{:.prec$}", tick.last_price, prec = self.price_digits);
        }
    }
    
    /// Set symbol from position or tick double-click
    pub fn set_symbol(&mut self, symbol: &str, exchange: Exchange) {
        self.symbol = symbol.to_string();
        if let Some(idx) = self.exchanges.iter().position(|e| *e == exchange) {
            self.exchange_index = idx;
        }
        self.update_vt_symbol();
    }
    
    /// Update vt_symbol and request subscription
    fn update_vt_symbol(&mut self) {
        if self.symbol.is_empty() {
            return;
        }
        
        let exchange = self.exchanges.get(self.exchange_index).cloned()
            .unwrap_or(Exchange::Binance);
        let new_vt_symbol = format!("{}.{}", self.symbol, exchange);
        
        if new_vt_symbol != self.vt_symbol {
            self.vt_symbol = new_vt_symbol;
            self.name.clear();
            self.depth = MarketDepth::default();
            self.price.clear();
            self.volume.clear();
            
            // Request subscription
            self.pending_subscribe = Some(SubscribeRequest {
                symbol: self.symbol.clone(),
                exchange,
            });
        }
    }
    
    /// Show the trading widget
    pub fn show(&mut self, ui: &mut Ui) {
        ui.set_min_width(280.0);
        
        // Order entry section
        ui.group(|ui| {
            ui.heading("交易");
            ui.separator();
            
            egui::Grid::new("trading_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    // Exchange
                    ui.label("交易所");
                    let exchange_text = self.exchanges.get(self.exchange_index)
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    ComboBox::from_id_salt("exchange_combo")
                        .selected_text(&exchange_text)
                        .show_ui(ui, |ui| {
                            for (i, exchange) in self.exchanges.iter().enumerate() {
                                if ui.selectable_label(i == self.exchange_index, exchange.to_string()).clicked() {
                                    self.exchange_index = i;
                                }
                            }
                        });
                    ui.end_row();
                    
                    // Symbol
                    ui.label("代码");
                    let response = ui.text_edit_singleline(&mut self.symbol);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.update_vt_symbol();
                    }
                    ui.end_row();
                    
                    // Name
                    ui.label("名称");
                    ui.label(&self.name);
                    ui.end_row();
                    
                    // Direction
                    ui.label("方向");
                    let directions = ["多", "空"];
                    ComboBox::from_id_salt("direction_combo")
                        .selected_text(directions[self.direction_index])
                        .show_ui(ui, |ui| {
                            for (i, dir) in directions.iter().enumerate() {
                                if ui.selectable_label(i == self.direction_index, *dir).clicked() {
                                    self.direction_index = i;
                                }
                            }
                        });
                    ui.end_row();
                    
                    // Offset
                    ui.label("开平");
                    let offsets = ["开仓", "平仓", "平今", "平昨"];
                    ComboBox::from_id_salt("offset_combo")
                        .selected_text(offsets[self.offset_index])
                        .show_ui(ui, |ui| {
                            for (i, offset) in offsets.iter().enumerate() {
                                if ui.selectable_label(i == self.offset_index, *offset).clicked() {
                                    self.offset_index = i;
                                }
                            }
                        });
                    ui.end_row();
                    
                    // Order type
                    ui.label("类型");
                    let order_types = ["限价", "市价", "FAK", "FOK"];
                    ComboBox::from_id_salt("order_type_combo")
                        .selected_text(order_types[self.order_type_index])
                        .show_ui(ui, |ui| {
                            for (i, ot) in order_types.iter().enumerate() {
                                if ui.selectable_label(i == self.order_type_index, *ot).clicked() {
                                    self.order_type_index = i;
                                }
                            }
                        });
                    ui.end_row();
                    
                    // Price
                    ui.label("价格");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.price);
                        ui.checkbox(&mut self.track_price, "跟踪");
                    });
                    ui.end_row();
                    
                    // Volume
                    ui.label("数量");
                    ui.text_edit_singleline(&mut self.volume);
                    ui.end_row();
                    
                    // Gateway
                    ui.label("接口");
                    let gateway_text = self.gateways.get(self.gateway_index)
                        .cloned()
                        .unwrap_or_default();
                    ComboBox::from_id_salt("gateway_combo")
                        .selected_text(&gateway_text)
                        .show_ui(ui, |ui| {
                            for (i, gateway) in self.gateways.iter().enumerate() {
                                if ui.selectable_label(i == self.gateway_index, gateway).clicked() {
                                    self.gateway_index = i;
                                }
                            }
                        });
                    ui.end_row();
                });
            
            ui.separator();
            
            // Buttons
            ui.horizontal(|ui| {
                if ui.button("委托").clicked() {
                    self.send_order();
                }
                if ui.button("全撤").clicked() {
                    self.pending_cancel_all = true;
                }
            });
        });
        
        ui.add_space(10.0);
        
        // Market depth section
        ui.group(|ui| {
            ui.heading("盘口");
            ui.separator();
            
            let prec = self.price_digits;
            
            // Ask levels (reversed, 5 to 1)
            for i in (0..5).rev() {
                let level = &self.depth.ask_levels[i];
                if level.price > 0.0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:.prec$}", level.price, prec = prec)).color(COLOR_ASK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0}", level.volume)).color(COLOR_ASK));
                        });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("--").color(COLOR_ASK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("--").color(COLOR_ASK));
                        });
                    });
                }
            }
            
            // Last price with change
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:.prec$}", self.depth.last_price, prec = prec)).strong());
                if let Some(pct) = self.depth.price_change_pct() {
                    let color = if pct >= 0.0 { COLOR_LONG } else { COLOR_SHORT };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{:+.2}%", pct)).color(color));
                    });
                }
            });
            ui.separator();
            
            // Bid levels (1 to 5)
            for i in 0..5 {
                let level = &self.depth.bid_levels[i];
                if level.price > 0.0 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:.prec$}", level.price, prec = prec)).color(COLOR_BID));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0}", level.volume)).color(COLOR_BID));
                        });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("--").color(COLOR_BID));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("--").color(COLOR_BID));
                        });
                    });
                }
            }
        });
    }
    
    /// Create and queue an order request
    fn send_order(&mut self) {
        if self.symbol.is_empty() {
            return;
        }
        
        let volume: f64 = match self.volume.parse() {
            Ok(v) if v > 0.0 => v,
            _ => return,
        };
        
        let price: f64 = self.price.parse().unwrap_or(0.0);
        
        let exchange = self.exchanges.get(self.exchange_index)
            .cloned()
            .unwrap_or(Exchange::Binance);
        
        let direction = match self.direction_index {
            0 => Direction::Long,
            _ => Direction::Short,
        };
        
        let offset = match self.offset_index {
            0 => Offset::Open,
            1 => Offset::Close,
            2 => Offset::CloseToday,
            _ => Offset::CloseYesterday,
        };
        
        let order_type = match self.order_type_index {
            0 => OrderType::Limit,
            1 => OrderType::Market,
            2 => OrderType::Fak,
            _ => OrderType::Fok,
        };
        
        self.pending_order = Some(OrderRequest {
            symbol: self.symbol.clone(),
            exchange,
            direction,
            order_type,
            volume,
            price,
            offset,
            reference: "ManualTrading".to_string(),
        });
    }
    
    /// Take pending subscribe request
    pub fn take_subscribe(&mut self) -> Option<SubscribeRequest> {
        self.pending_subscribe.take()
    }
    
    /// Take pending order request
    pub fn take_order(&mut self) -> Option<(OrderRequest, String)> {
        let order = self.pending_order.take()?;
        let gateway = self.gateways.get(self.gateway_index).cloned()?;
        Some((order, gateway))
    }
    
    /// Take cancel all flag
    pub fn take_cancel_all(&mut self) -> bool {
        let result = self.pending_cancel_all;
        self.pending_cancel_all = false;
        result
    }
}
