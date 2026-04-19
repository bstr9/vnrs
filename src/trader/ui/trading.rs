//! Trading widget for manual order entry and market depth display.

use egui::{Color32, ComboBox, RichText, Ui};

use super::style::*;
use crate::trader::constant::{Direction, Exchange, Offset, OrderType};
use crate::trader::object::{ContractData, OrderRequest, SubscribeRequest, TickData};
use crate::trader::utility::get_digits;

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

        self.bid_levels[0] = DepthLevel {
            price: tick.bid_price_1,
            volume: tick.bid_volume_1,
        };
        self.bid_levels[1] = DepthLevel {
            price: tick.bid_price_2,
            volume: tick.bid_volume_2,
        };
        self.bid_levels[2] = DepthLevel {
            price: tick.bid_price_3,
            volume: tick.bid_volume_3,
        };
        self.bid_levels[3] = DepthLevel {
            price: tick.bid_price_4,
            volume: tick.bid_volume_4,
        };
        self.bid_levels[4] = DepthLevel {
            price: tick.bid_price_5,
            volume: tick.bid_volume_5,
        };

        self.ask_levels[0] = DepthLevel {
            price: tick.ask_price_1,
            volume: tick.ask_volume_1,
        };
        self.ask_levels[1] = DepthLevel {
            price: tick.ask_price_2,
            volume: tick.ask_volume_2,
        };
        self.ask_levels[2] = DepthLevel {
            price: tick.ask_price_3,
            volume: tick.ask_volume_3,
        };
        self.ask_levels[3] = DepthLevel {
            price: tick.ask_price_4,
            volume: tick.ask_volume_4,
        };
        self.ask_levels[4] = DepthLevel {
            price: tick.ask_price_5,
            volume: tick.ask_volume_5,
        };
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

    // Available contracts for autocomplete
    pub contracts: Vec<ContractData>,
    /// Filtered contracts based on user input
    pub contract_filter: String,
    /// Whether the dropdown is open
    pub show_dropdown: bool,

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
    /// Last order error message (set when order submission fails validation)
    pub last_order_error: Option<String>,
    /// Whether to focus the symbol input on next frame
    pub focus_symbol_input: bool,
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
                Exchange::BinanceUsdm,
                Exchange::BinanceCoinm,
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
            contracts: Vec::new(),
            contract_filter: String::new(),
            show_dropdown: false,
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
            last_order_error: None,
            focus_symbol_input: false,
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

    /// Set available contracts for dropdown selection
    pub fn set_contracts(&mut self, contracts: Vec<ContractData>) {
        self.contracts = contracts;
    }

    /// Update with contract info
    pub fn set_contract(&mut self, contract: ContractData) {
        self.name = contract.name.clone();
        self.price_digits = get_digits(contract.pricetick);

        // Find gateway index
        if let Some(idx) = self
            .gateways
            .iter()
            .position(|g| g == &contract.gateway_name)
        {
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

    /// Set subscribe request from MCP command
    pub fn set_subscribe_request(&mut self, req: SubscribeRequest, gateway_name: &str) {
        // Update gateway index if found
        if let Some(idx) = self.gateways.iter().position(|g| g == gateway_name) {
            self.gateway_index = idx;
        }
        self.pending_subscribe = Some(req);
    }

    /// Set order request from MCP command
    pub fn set_order_request(&mut self, req: OrderRequest, gateway_name: &str) {
        // Update gateway index if found
        if let Some(idx) = self.gateways.iter().position(|g| g == gateway_name) {
            self.gateway_index = idx;
        }
        self.pending_order = Some(req);
    }

    /// Update vt_symbol and request subscription
    fn update_vt_symbol(&mut self) {
        if self.symbol.is_empty() {
            return;
        }

        let exchange = self
            .exchanges
            .get(self.exchange_index)
            .cloned()
            .unwrap_or(Exchange::Binance);

        // Normalize symbol to lowercase for consistent matching with tick data
        let normalized_symbol = self.symbol.to_lowercase();
        let new_vt_symbol = format!("{}.{}", normalized_symbol, exchange);

        if new_vt_symbol != self.vt_symbol {
            self.vt_symbol = new_vt_symbol;
            self.name.clear();
            self.depth = MarketDepth::default();
            self.price.clear();
            self.volume.clear();

            // Request subscription
            self.pending_subscribe = Some(SubscribeRequest {
                symbol: normalized_symbol,
                exchange,
            });
        }
    }

    /// Show the trading widget
    pub fn show(&mut self, ui: &mut Ui) {
        ui.set_min_width(280.0);

        // Order entry section
        // Track popup state to render it outside the Group/Grid (avoids clipping)
        let mut popup_info: Option<(egui::Rect, Vec<ContractData>)> = None;

        ui.group(|ui| {
            ui.heading("交易");
            ui.separator();

            egui::Grid::new("trading_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    // Exchange
                    ui.label("交易所");
                    let exchange_text = self
                        .exchanges
                        .get(self.exchange_index)
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    ComboBox::from_id_salt("exchange_combo")
                        .selected_text(&exchange_text)
                        .show_ui(ui, |ui| {
                            for (i, exchange) in self.exchanges.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        i == self.exchange_index,
                                        exchange.to_string(),
                                    )
                                    .clicked()
                                {
                                    self.exchange_index = i;
                                }
                            }
                        });
                    ui.end_row();

                    // Symbol - Text input with autocomplete dropdown
                    ui.label("代码");

                    // Text input for symbol
                    let symbol_id = egui::Id::new("symbol_text_edit");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.symbol)
                            .id(symbol_id)
                            .hint_text(if self.contracts.is_empty() {
                                "请先连接接口..."
                            } else {
                                "输入合约代码..."
                            }),
                    );

                    // Focus symbol input if requested
                    if self.focus_symbol_input {
                        response.request_focus();
                        self.focus_symbol_input = false;
                    }

                    // Check if user is typing (filter contracts)
                    let input_lower = self.symbol.to_lowercase();
                    let filtered_contracts: Vec<_> = if input_lower.is_empty() {
                        // Show first 20 popular contracts when empty
                        self.contracts.iter().take(20).collect()
                    } else {
                        // Filter contracts that match input (case-insensitive prefix match)
                        self.contracts
                            .iter()
                            .filter(|c| c.symbol.to_lowercase().starts_with(&input_lower))
                            .take(20)
                            .collect()
                    };

                    // Show dropdown when text input has focus and there are matching contracts.
                    // Use a flag to keep the popup open while interacting with it.
                    if response.has_focus() && !filtered_contracts.is_empty() {
                        self.show_dropdown = true;
                    }
                    
                    // Save popup info to render AFTER the group (avoids clipping by parent containers)
                    if self.show_dropdown && !filtered_contracts.is_empty() {
                        popup_info = Some((
                            response.rect,
                            filtered_contracts.into_iter().cloned().collect(),
                        ));
                    }

                    // Subscribe on Enter key
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.update_vt_symbol();
                        self.show_dropdown = false;
                    }

                    // Escape to close dropdown
                    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.show_dropdown = false;
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
                                if ui
                                    .selectable_label(i == self.direction_index, *dir)
                                    .clicked()
                                {
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
                                if ui
                                    .selectable_label(i == self.offset_index, *offset)
                                    .clicked()
                                {
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
                                if ui
                                    .selectable_label(i == self.order_type_index, *ot)
                                    .clicked()
                                {
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
                    let gateway_text = self
                        .gateways
                        .get(self.gateway_index)
                        .cloned()
                        .unwrap_or_default();
                    ComboBox::from_id_salt("gateway_combo")
                        .selected_text(&gateway_text)
                        .show_ui(ui, |ui| {
                            for (i, gateway) in self.gateways.iter().enumerate() {
                                if ui
                                    .selectable_label(i == self.gateway_index, gateway)
                                    .clicked()
                                {
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

            // Contract count indicator
            if !self.contracts.is_empty() {
                ui.label(
                    RichText::new(format!("已加载 {} 个合约", self.contracts.len()))
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
        });

        // Render autocomplete popup OUTSIDE the group closure to avoid clipping issues.
        // The popup floats above all other UI using egui::Area with Foreground order.
        if let Some((widget_rect, filtered_contracts)) = popup_info {
            let area_id = egui::Id::new("contract_autocomplete_area");
            let input_lower = self.symbol.to_lowercase();

            let area_response = egui::Area::new(area_id)
                .pivot(egui::Align2::LEFT_TOP)
                .fixed_pos(widget_rect.left_bottom())
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(260.0);
                        ui.set_max_height(200.0);

                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for contract in &filtered_contracts {
                                    let label = format!(
                                        "{} ({})",
                                        contract.symbol.to_uppercase(),
                                        contract.name
                                    );

                                    // Highlight if exact match
                                    let is_exact =
                                        contract.symbol.to_lowercase() == input_lower;

                                    if ui.selectable_label(is_exact, label).clicked() {
                                        // User selected this contract
                                        self.symbol = contract.symbol.clone();
                                        self.name = contract.name.clone();
                                        self.price_digits = get_digits(contract.pricetick);

                                        // Find matching exchange index
                                        if let Some(idx) = self
                                            .exchanges
                                            .iter()
                                            .position(|e| *e == contract.exchange)
                                        {
                                            self.exchange_index = idx;
                                        }

                                        // Find matching gateway index
                                        if let Some(idx) = self
                                            .gateways
                                            .iter()
                                            .position(|g| g == &contract.gateway_name)
                                        {
                                            self.gateway_index = idx;
                                        }

                                        self.contract = Some(contract.clone());

                                        // Update vt_symbol and request subscription
                                        let new_vt_symbol = contract.vt_symbol();
                                        if new_vt_symbol != self.vt_symbol {
                                            self.vt_symbol = new_vt_symbol;
                                            self.depth = MarketDepth::default();
                                            self.price.clear();
                                            self.volume.clear();

                                            self.pending_subscribe = Some(SubscribeRequest {
                                                symbol: contract.symbol.clone(),
                                                exchange: contract.exchange,
                                            });
                                        }

                                        // Close dropdown and refocus text input
                                        self.show_dropdown = false;
                                        self.focus_symbol_input = true;
                                    }
                                }
                            });
                    });
                });

            // If the user clicks outside the dropdown area, close it
            if area_response.response.clicked_elsewhere() {
                self.show_dropdown = false;
            }
        }

        ui.add_space(10.0);

        // Market depth section
        ui.group(|ui| {
            ui.heading("盘口");
            ui.separator();

            let prec = self.price_digits;

            // Calculate max volume across all 10 levels for bar scaling
            let max_volume = {
                let ask_max = self.depth.ask_levels[0..5]
                    .iter()
                    .map(|l| l.volume)
                    .fold(0.0_f64, f64::max);
                let bid_max = self.depth.bid_levels[0..5]
                    .iter()
                    .map(|l| l.volume)
                    .fold(0.0_f64, f64::max);
                ask_max.max(bid_max).max(1.0) // avoid division by zero
            };

            // Bar colors (semi-transparent)
            let ask_bar_color = Color32::from_rgba_unmultiplied(160, 255, 160, 80);
            let bid_bar_color = Color32::from_rgba_unmultiplied(255, 174, 201, 80);

            // Ask levels (reversed, 5 to 1)
            for i in (0..5).rev() {
                let level = &self.depth.ask_levels[i];
                if level.price > 0.0 {
                    ui.horizontal(|ui| {
                        let row_width = ui.available_width();
                        let bar_width = (level.volume / max_volume) as f32 * row_width;
                        let rect = ui.available_rect_before_wrap();
                        // Draw bar from right to left (background)
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(rect.right() - bar_width, rect.top()),
                                egui::pos2(rect.right(), rect.bottom()),
                            ),
                            0.0,
                            ask_bar_color,
                        );
                        ui.label(
                            RichText::new(format!("{:.prec$}", level.price, prec = prec))
                                .color(COLOR_ASK),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", level.volume)).color(COLOR_ASK),
                            );
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
                ui.label(
                    RichText::new(format!("{:.prec$}", self.depth.last_price, prec = prec))
                        .strong(),
                );
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
                        let row_width = ui.available_width();
                        let bar_width = (level.volume / max_volume) as f32 * row_width;
                        let rect = ui.available_rect_before_wrap();
                        // Draw bar from right to left (background)
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(rect.right() - bar_width, rect.top()),
                                egui::pos2(rect.right(), rect.bottom()),
                            ),
                            0.0,
                            bid_bar_color,
                        );
                        ui.label(
                            RichText::new(format!("{:.prec$}", level.price, prec = prec))
                                .color(COLOR_BID),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", level.volume)).color(COLOR_BID),
                            );
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
            self.last_order_error = Some("代码不能为空".to_string());
            return;
        }

        let volume: f64 = match self.volume.parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                self.last_order_error = Some("数量无效".to_string());
                return;
            }
        };

        let price: f64 = self.price.parse().unwrap_or(0.0);

        let exchange = self
            .exchanges
            .get(self.exchange_index)
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

        self.last_order_error = None;
        self.pending_order = Some(OrderRequest {
            symbol: self.symbol.clone(),
            exchange,
            direction,
            order_type,
            volume,
            price,
            offset,
            reference: "ManualTrading".to_string(),
            post_only: false,
            reduce_only: false,
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
