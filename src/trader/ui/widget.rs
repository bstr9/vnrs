//! Widget components for the trading UI.
//!
//! This module provides various table-based widgets for displaying trading data
//! including ticks, orders, trades, positions, accounts, and logs.

use chrono::{DateTime, Local, Utc};
use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;

use super::style::*;
use crate::trader::constant::{Direction, Status};
use crate::trader::event::*;
use crate::trader::object::{
    AccountData, LogData, OrderData, PositionData, QuoteData, TickData, TradeData,
};

/// Reusable sort state for table columns
#[derive(Clone, Default)]
pub struct SortState {
    pub column: Option<usize>,
    pub ascending: bool,
}

impl SortState {
    pub fn new() -> Self {
        Self {
            column: None,
            ascending: true,
        }
    }

    /// Toggle sort on a column click. If same column, flip direction.
    pub fn toggle(&mut self, col: usize) {
        if self.column == Some(col) {
            self.ascending = !self.ascending;
        } else {
            self.column = Some(col);
            self.ascending = true;
        }
    }

    /// Returns the sort indicator string (▲ or ▼) for a column
    pub fn indicator(&self, col: usize) -> &'static str {
        if self.column == Some(col) {
            if self.ascending {
                " ▲"
            } else {
                " ▼"
            }
        } else {
            ""
        }
    }

    /// Apply sort direction to a comparison result
    pub fn apply_order(&self, cmp: std::cmp::Ordering) -> std::cmp::Ordering {
        if self.ascending {
            cmp
        } else {
            cmp.reverse()
        }
    }
}

/// Render a clickable sortable header cell
fn sortable_header(ui: &mut Ui, display: &str, col: usize, sort: &mut SortState) {
    let label = format!("{}{}", display, sort.indicator(col));
    let response = ui.strong(&label).interact(egui::Sense::click());
    if response.clicked() {
        sort.toggle(col);
    }
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
        );
    }
}

/// Cell content with optional color
#[derive(Clone)]
pub struct CellContent {
    pub text: String,
    pub color: Option<Color32>,
}

impl CellContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
        }
    }

    pub fn with_color(text: impl Into<String>, color: Color32) -> Self {
        Self {
            text: text.into(),
            color: Some(color),
        }
    }
}

/// Header definition for monitor tables
#[derive(Clone)]
pub struct HeaderDef {
    pub key: &'static str,
    pub display: &'static str,
    pub update: bool,
}

impl HeaderDef {
    pub const fn new(key: &'static str, display: &'static str, update: bool) -> Self {
        Self {
            key,
            display,
            update,
        }
    }
}

/// Format datetime for display
fn format_time(dt: &DateTime<Utc>) -> String {
    let local: DateTime<Local> = dt.with_timezone(&Local);
    local.format("%H:%M:%S%.3f").to_string()
}

/// Format price based on pricetick. Defaults to 4 decimal places if pricetick not provided.
/// Formula: decimals = -log10(pricetick).ceil()
pub fn format_price(price: f64, pricetick: Option<f64>) -> String {
    let decimals = pricetick
        .filter(|&p| p > 0.0)
        .map(|p| (-p.log10().ceil() as usize).max(0).min(8))
        .unwrap_or(4);
    format!("{:.1$}", price, decimals)
}

// ============================================================================
// Tick Monitor
// ============================================================================

/// Tick data row for display
#[derive(Clone)]
pub struct TickRow {
    pub symbol: String,
    pub exchange: String,
    pub name: String,
    pub last_price: f64,
    pub volume: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub bid_price_1: f64,
    pub bid_volume_1: f64,
    pub ask_price_1: f64,
    pub ask_volume_1: f64,
    pub datetime: String,
    pub gateway_name: String,
}

impl From<&TickData> for TickRow {
    fn from(tick: &TickData) -> Self {
        Self {
            symbol: tick.symbol.clone(),
            exchange: tick.exchange.to_string(),
            name: tick.name.clone(),
            last_price: tick.last_price,
            volume: tick.volume,
            open_price: tick.open_price,
            high_price: tick.high_price,
            low_price: tick.low_price,
            bid_price_1: tick.bid_price_1,
            bid_volume_1: tick.bid_volume_1,
            ask_price_1: tick.ask_price_1,
            ask_volume_1: tick.ask_volume_1,
            datetime: format_time(&tick.datetime),
            gateway_name: tick.gateway_name.clone(),
        }
    }
}

/// Tick monitor widget for displaying market data
pub struct TickMonitor {
    pub data: HashMap<String, TickRow>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for TickMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl TickMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_TICK;

    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("symbol", "代码", false),
                HeaderDef::new("exchange", "交易所", false),
                HeaderDef::new("name", "名称", true),
                HeaderDef::new("last_price", "最新价", true),
                HeaderDef::new("volume", "成交量", true),
                HeaderDef::new("open_price", "开盘价", true),
                HeaderDef::new("high_price", "最高价", true),
                HeaderDef::new("low_price", "最低价", true),
                HeaderDef::new("bid_price_1", "买1价", true),
                HeaderDef::new("bid_volume_1", "买1量", true),
                HeaderDef::new("ask_price_1", "卖1价", true),
                HeaderDef::new("ask_volume_1", "卖1量", true),
                HeaderDef::new("datetime", "时间", true),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();
        self.data.insert(vt_symbol, TickRow::from(tick));
    }

    /// Sort tick rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&TickRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| a.symbol.cmp(&b.symbol));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.name.cmp(&b.name))),
            3 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.last_price
                        .partial_cmp(&b.last_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            4 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.volume
                        .partial_cmp(&b.volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            5 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.open_price
                        .partial_cmp(&b.open_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            6 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.high_price
                        .partial_cmp(&b.high_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.low_price
                        .partial_cmp(&b.low_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.bid_price_1
                        .partial_cmp(&b.bid_price_1)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            9 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.bid_volume_1
                        .partial_cmp(&b.bid_volume_1)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            10 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.ask_price_1
                        .partial_cmp(&b.ask_price_1)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            11 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.ask_volume_1
                        .partial_cmp(&b.ask_volume_1)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            12 => rows.sort_by(|a, b| sort.apply_order(a.datetime.cmp(&b.datetime))),
            13 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| a.symbol.cmp(&b.symbol)),
        }
    }

    /// Show the tick monitor and return clicked symbol if any
    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        let available_height = ui.available_height();
        let mut clicked_symbol: Option<String> = None;

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.values().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        // Make the row clickable
                        let vt_symbol = format!("{}.{}", row.symbol, row.exchange);

                        table_row.col(|ui| {
                            if ui.selectable_label(false, &row.symbol).clicked() {
                                clicked_symbol = Some(vt_symbol.clone());
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.last_price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.volume));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.open_price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.high_price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.low_price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format_price(row.bid_price_1, None)).color(COLOR_BID),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", row.bid_volume_1)).color(COLOR_BID),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format_price(row.ask_price_1, None)).color(COLOR_ASK),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", row.ask_volume_1)).color(COLOR_ASK),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(&row.datetime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });

        clicked_symbol
    }
}

// ============================================================================
// Order Monitor
// ============================================================================

/// Order data row for display
#[derive(Clone)]
pub struct OrderRow {
    pub vt_orderid: String,
    pub orderid: String,
    pub reference: String,
    pub symbol: String,
    pub exchange: String,
    pub order_type: String,
    pub direction: Direction,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub traded: f64,
    pub status: Status,
    pub datetime: String,
    pub gateway_name: String,
    pub is_active: bool,
}

impl From<&OrderData> for OrderRow {
    fn from(order: &OrderData) -> Self {
        Self {
            vt_orderid: order.vt_orderid(),
            orderid: order.orderid.clone(),
            reference: order.reference.clone(),
            symbol: order.symbol.clone(),
            exchange: order.exchange.to_string(),
            order_type: order.order_type.to_string(),
            direction: order.direction.unwrap_or(Direction::Long),
            offset: order.offset.to_string(),
            price: order.price,
            volume: order.volume,
            traded: order.traded,
            status: order.status,
            datetime: order
                .datetime
                .map(|dt| format_time(&dt))
                .unwrap_or_default(),
            gateway_name: order.gateway_name.clone(),
            is_active: order.is_active(),
        }
    }
}

/// Order monitor widget
pub struct OrderMonitor {
    pub data: HashMap<String, OrderRow>,
    pub selected: Option<String>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for OrderMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_ORDER;

    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            selected: None,
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("orderid", "委托号", false),
                HeaderDef::new("reference", "来源", false),
                HeaderDef::new("symbol", "代码", false),
                HeaderDef::new("exchange", "交易所", false),
                HeaderDef::new("order_type", "类型", false),
                HeaderDef::new("direction", "方向", false),
                HeaderDef::new("offset", "开平", false),
                HeaderDef::new("price", "价格", false),
                HeaderDef::new("volume", "总数量", true),
                HeaderDef::new("traded", "已成交", true),
                HeaderDef::new("status", "状态", true),
                HeaderDef::new("datetime", "时间", true),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, order: &OrderData) {
        let vt_orderid = order.vt_orderid();
        self.data.insert(vt_orderid, OrderRow::from(order));
    }

    /// Sort order rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&OrderRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| b.datetime.cmp(&a.datetime));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.orderid.cmp(&b.orderid))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.reference.cmp(&b.reference))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            3 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            4 => rows.sort_by(|a, b| sort.apply_order(a.order_type.cmp(&b.order_type))),
            5 => rows.sort_by(|a, b| {
                sort.apply_order(a.direction.to_string().cmp(&b.direction.to_string()))
            }),
            6 => rows.sort_by(|a, b| sort.apply_order(a.offset.cmp(&b.offset))),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.volume
                        .partial_cmp(&b.volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            9 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.traded
                        .partial_cmp(&b.traded)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            10 => rows
                .sort_by(|a, b| sort.apply_order(a.status.to_string().cmp(&b.status.to_string()))),
            11 => rows.sort_by(|a, b| sort.apply_order(a.datetime.cmp(&b.datetime))),
            12 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| b.datetime.cmp(&a.datetime)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        let mut cancel_orderid = None;
        let available_height = ui.available_height();

        ui.label("双击委托行撤单");

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.values().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            if ui.selectable_label(false, &row.orderid).double_clicked()
                                && row.is_active
                            {
                                cancel_orderid = Some(row.vt_orderid.clone());
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.reference);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.symbol);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.order_type);
                        });
                        table_row.col(|ui| {
                            let color = get_direction_color(row.direction == Direction::Long);
                            ui.label(RichText::new(row.direction.to_string()).color(color));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.offset);
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.volume));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.traded));
                        });
                        table_row.col(|ui| {
                            ui.label(row.status.to_string());
                        });
                        table_row.col(|ui| {
                            ui.label(&row.datetime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });

        cancel_orderid
    }
}

/// Active order monitor - shows only active orders
pub struct ActiveOrderMonitor {
    inner: OrderMonitor,
    pub sort: SortState,
}

impl Default for ActiveOrderMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveOrderMonitor {
    pub fn new() -> Self {
        Self {
            inner: OrderMonitor::new(),
            sort: SortState::new(),
        }
    }

    pub fn update(&mut self, order: &OrderData) {
        self.inner.update(order);
    }

    /// Sort active order rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&OrderRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| b.datetime.cmp(&a.datetime));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.orderid.cmp(&b.orderid))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.reference.cmp(&b.reference))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            3 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            4 => rows.sort_by(|a, b| sort.apply_order(a.order_type.cmp(&b.order_type))),
            5 => rows.sort_by(|a, b| {
                sort.apply_order(a.direction.to_string().cmp(&b.direction.to_string()))
            }),
            6 => rows.sort_by(|a, b| sort.apply_order(a.offset.cmp(&b.offset))),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.volume
                        .partial_cmp(&b.volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            9 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.traded
                        .partial_cmp(&b.traded)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            10 => rows
                .sort_by(|a, b| sort.apply_order(a.status.to_string().cmp(&b.status.to_string()))),
            11 => rows.sort_by(|a, b| sort.apply_order(a.datetime.cmp(&b.datetime))),
            12 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| b.datetime.cmp(&a.datetime)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        let mut cancel_orderid = None;
        let available_height = ui.available_height();

        ui.label("双击委托行撤单（仅显示活动委托）");

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.inner.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.inner.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.inner.data.values().filter(|r| r.is_active).collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            if ui.selectable_label(false, &row.orderid).double_clicked() {
                                cancel_orderid = Some(row.vt_orderid.clone());
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.reference);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.symbol);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.order_type);
                        });
                        table_row.col(|ui| {
                            let color = get_direction_color(row.direction == Direction::Long);
                            ui.label(RichText::new(row.direction.to_string()).color(color));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.offset);
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.volume));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.traded));
                        });
                        table_row.col(|ui| {
                            ui.label(row.status.to_string());
                        });
                        table_row.col(|ui| {
                            ui.label(&row.datetime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });

        cancel_orderid
    }
}

// ============================================================================
// Trade Monitor
// ============================================================================

/// Trade data row for display
#[derive(Clone)]
pub struct TradeRow {
    pub tradeid: String,
    pub orderid: String,
    pub symbol: String,
    pub exchange: String,
    pub direction: Direction,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub datetime: String,
    pub gateway_name: String,
}

impl From<&TradeData> for TradeRow {
    fn from(trade: &TradeData) -> Self {
        Self {
            tradeid: trade.tradeid.clone(),
            orderid: trade.orderid.clone(),
            symbol: trade.symbol.clone(),
            exchange: trade.exchange.to_string(),
            direction: trade.direction.unwrap_or(Direction::Long),
            offset: trade.offset.to_string(),
            price: trade.price,
            volume: trade.volume,
            datetime: trade
                .datetime
                .map(|dt| format_time(&dt))
                .unwrap_or_default(),
            gateway_name: trade.gateway_name.clone(),
        }
    }
}

/// Trade monitor widget
pub struct TradeMonitor {
    pub data: Vec<TradeRow>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for TradeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_TRADE;

    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("tradeid", "成交号", false),
                HeaderDef::new("orderid", "委托号", false),
                HeaderDef::new("symbol", "代码", false),
                HeaderDef::new("exchange", "交易所", false),
                HeaderDef::new("direction", "方向", false),
                HeaderDef::new("offset", "开平", false),
                HeaderDef::new("price", "价格", false),
                HeaderDef::new("volume", "数量", false),
                HeaderDef::new("datetime", "时间", false),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, trade: &TradeData) {
        self.data.insert(0, TradeRow::from(trade));
    }

    /// Sort trade rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&TradeRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => return, // Keep insertion order (newest first)
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.tradeid.cmp(&b.tradeid))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.orderid.cmp(&b.orderid))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            3 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            4 => rows.sort_by(|a, b| {
                sort.apply_order(a.direction.to_string().cmp(&b.direction.to_string()))
            }),
            5 => rows.sort_by(|a, b| sort.apply_order(a.offset.cmp(&b.offset))),
            6 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.volume
                        .partial_cmp(&b.volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| sort.apply_order(a.datetime.cmp(&b.datetime))),
            9 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => {}
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let available_height = ui.available_height();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.iter().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            ui.label(&row.tradeid);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.orderid);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.symbol);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            let color = get_direction_color(row.direction == Direction::Long);
                            ui.label(RichText::new(row.direction.to_string()).color(color));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.offset);
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.price, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.volume));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.datetime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });
    }
}

// ============================================================================
// Position Monitor
// ============================================================================

/// Position data row for display
#[derive(Clone)]
pub struct PositionRow {
    pub vt_positionid: String,
    pub symbol: String,
    pub exchange: String,
    pub direction: Direction,
    pub volume: f64,
    pub yd_volume: f64,
    pub frozen: f64,
    pub price: f64,
    pub pnl: f64,
    pub gateway_name: String,
}

impl From<&PositionData> for PositionRow {
    fn from(pos: &PositionData) -> Self {
        Self {
            vt_positionid: pos.vt_positionid(),
            symbol: pos.symbol.clone(),
            exchange: pos.exchange.to_string(),
            direction: pos.direction,
            volume: pos.volume,
            yd_volume: pos.yd_volume,
            frozen: pos.frozen,
            price: pos.price,
            pnl: pos.pnl,
            gateway_name: pos.gateway_name.clone(),
        }
    }
}

/// Position monitor widget
pub struct PositionMonitor {
    pub data: HashMap<String, PositionRow>,
    pub selected: Option<String>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for PositionMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_POSITION;

    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            selected: None,
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("symbol", "代码", false),
                HeaderDef::new("exchange", "交易所", false),
                HeaderDef::new("direction", "方向", false),
                HeaderDef::new("volume", "数量", true),
                HeaderDef::new("yd_volume", "昨仓", true),
                HeaderDef::new("frozen", "冻结", true),
                HeaderDef::new("price", "均价", true),
                HeaderDef::new("pnl", "盈亏", true),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, position: &PositionData) {
        let vt_positionid = position.vt_positionid();
        self.data.insert(vt_positionid, PositionRow::from(position));
    }

    /// Sort position rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&PositionRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| a.symbol.cmp(&b.symbol));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            2 => rows.sort_by(|a, b| {
                sort.apply_order(a.direction.to_string().cmp(&b.direction.to_string()))
            }),
            3 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.volume
                        .partial_cmp(&b.volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            4 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.yd_volume
                        .partial_cmp(&b.yd_volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            5 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.frozen
                        .partial_cmp(&b.frozen)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            6 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.pnl
                        .partial_cmp(&b.pnl)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| a.symbol.cmp(&b.symbol)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<PositionRow> {
        let mut selected_position = None;
        let available_height = ui.available_height();

        ui.label("双击持仓行快速平仓");

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.values().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            if ui.selectable_label(false, &row.symbol).double_clicked() {
                                selected_position = Some(row.clone());
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            let color = get_direction_color(row.direction == Direction::Long);
                            ui.label(RichText::new(row.direction.to_string()).color(color));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.volume));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.yd_volume));
                        });
                        table_row.col(|ui| {
                            ui.label(format!("{:.0}", row.frozen));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.price, None));
                        });
                        table_row.col(|ui| {
                            let color = get_pnl_color(row.pnl);
                            ui.label(RichText::new(format_price(row.pnl, None)).color(color));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });

        selected_position
    }
}

// ============================================================================
// Account Monitor
// ============================================================================

/// Account data row for display
#[derive(Clone)]
pub struct AccountRow {
    pub vt_accountid: String,
    pub accountid: String,
    pub balance: f64,
    pub frozen: f64,
    pub available: f64,
    pub gateway_name: String,
}

impl From<&AccountData> for AccountRow {
    fn from(account: &AccountData) -> Self {
        Self {
            vt_accountid: account.vt_accountid(),
            accountid: account.accountid.clone(),
            balance: account.balance,
            frozen: account.frozen,
            available: account.available(),
            gateway_name: account.gateway_name.clone(),
        }
    }
}

/// Account monitor widget
pub struct AccountMonitor {
    pub data: HashMap<String, AccountRow>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for AccountMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_ACCOUNT;

    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("accountid", "账号", false),
                HeaderDef::new("balance", "余额", true),
                HeaderDef::new("frozen", "冻结", true),
                HeaderDef::new("available", "可用", true),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, account: &AccountData) {
        let vt_accountid = account.vt_accountid();
        self.data.insert(vt_accountid, AccountRow::from(account));
    }

    /// Sort account rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&AccountRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| a.accountid.cmp(&b.accountid));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.accountid.cmp(&b.accountid))),
            1 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.balance
                        .partial_cmp(&b.balance)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            2 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.frozen
                        .partial_cmp(&b.frozen)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            3 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.available
                        .partial_cmp(&b.available)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            4 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| a.accountid.cmp(&b.accountid)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let available_height = ui.available_height();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(80.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.values().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            ui.label(&row.accountid);
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.balance, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.frozen, None));
                        });
                        table_row.col(|ui| {
                            ui.label(format_price(row.available, None));
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });
    }
}

// ============================================================================
// Log Monitor
// ============================================================================

/// Log data row for display
#[derive(Clone)]
pub struct LogRow {
    pub time: String,
    pub msg: String,
    pub gateway_name: String,
    pub level: i32,
}

impl From<&LogData> for LogRow {
    fn from(log: &LogData) -> Self {
        Self {
            time: format_time(&log.time),
            msg: log.msg.clone(),
            gateway_name: log.gateway_name.clone(),
            level: log.level,
        }
    }
}

/// Log monitor widget
pub struct LogMonitor {
    pub data: Vec<LogRow>,
    pub max_rows: usize,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
    /// Number of logs already synced from engine
    synced_count: usize,
}

impl Default for LogMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl LogMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_LOG;

    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            max_rows: 1000,
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("time", "时间", false),
                HeaderDef::new("msg", "信息", false),
                HeaderDef::new("gateway_name", "接口", false),
            ],
            synced_count: 0,
        }
    }

    pub fn update(&mut self, log: &LogData) {
        self.data.insert(0, LogRow::from(log));
        if self.data.len() > self.max_rows {
            self.data.truncate(self.max_rows);
        }
    }

    /// Sync new logs from engine (only adds new ones)
    pub fn sync_logs(&mut self, logs: &[LogData]) {
        let new_count = logs.len();
        if new_count > self.synced_count {
            // logs are ordered newest first, so take the difference
            let new_logs = new_count - self.synced_count;
            for log in logs.iter().take(new_logs) {
                self.data.insert(0, LogRow::from(log));
            }
            self.synced_count = new_count;

            if self.data.len() > self.max_rows {
                self.data.truncate(self.max_rows);
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let available_height = ui.available_height();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(80.0))
            .column(Column::remainder().at_least(200.0))
            .column(Column::auto().at_least(60.0))
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                // Build index list for sorting
                let mut indices: Vec<usize> = (0..self.data.len()).collect();
                if let Some(col) = self.sort.column {
                    let sort = &self.sort;
                    match col {
                        0 => indices.sort_by(|&a, &b| {
                            sort.apply_order(self.data[a].time.cmp(&self.data[b].time))
                        }),
                        1 => indices.sort_by(|&a, &b| {
                            sort.apply_order(self.data[a].msg.cmp(&self.data[b].msg))
                        }),
                        2 => indices.sort_by(|&a, &b| {
                            sort.apply_order(
                                self.data[a].gateway_name.cmp(&self.data[b].gateway_name),
                            )
                        }),
                        _ => {}
                    }
                }

                for idx in indices {
                    let row = &self.data[idx];
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            ui.label(&row.time);
                        });
                        table_row.col(|ui| {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(&row.msg);
                                },
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });
    }
}

// ============================================================================
// Quote Monitor
// ============================================================================

/// Quote data row for display
#[derive(Clone)]
pub struct QuoteRow {
    pub vt_quoteid: String,
    pub quoteid: String,
    pub reference: String,
    pub symbol: String,
    pub exchange: String,
    pub bid_offset: String,
    pub bid_volume: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub ask_volume: f64,
    pub ask_offset: String,
    pub status: Status,
    pub datetime: String,
    pub gateway_name: String,
}

impl From<&QuoteData> for QuoteRow {
    fn from(quote: &QuoteData) -> Self {
        Self {
            vt_quoteid: quote.vt_quoteid(),
            quoteid: quote.quoteid.clone(),
            reference: quote.reference.clone(),
            symbol: quote.symbol.clone(),
            exchange: quote.exchange.to_string(),
            bid_offset: quote.bid_offset.to_string(),
            bid_volume: quote.bid_volume as f64,
            bid_price: quote.bid_price,
            ask_price: quote.ask_price,
            ask_volume: quote.ask_volume as f64,
            ask_offset: quote.ask_offset.to_string(),
            status: quote.status,
            datetime: quote
                .datetime
                .map(|dt| format_time(&dt))
                .unwrap_or_default(),
            gateway_name: quote.gateway_name.clone(),
        }
    }
}

/// Quote monitor widget
pub struct QuoteMonitor {
    pub data: HashMap<String, QuoteRow>,
    pub sort: SortState,
    headers: Vec<HeaderDef>,
}

impl Default for QuoteMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl QuoteMonitor {
    pub const EVENT_TYPE: &'static str = EVENT_QUOTE;

    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            sort: SortState::new(),
            headers: vec![
                HeaderDef::new("quoteid", "报价号", false),
                HeaderDef::new("reference", "来源", false),
                HeaderDef::new("symbol", "代码", false),
                HeaderDef::new("exchange", "交易所", false),
                HeaderDef::new("bid_offset", "买开平", false),
                HeaderDef::new("bid_volume", "买量", false),
                HeaderDef::new("bid_price", "买价", false),
                HeaderDef::new("ask_price", "卖价", false),
                HeaderDef::new("ask_volume", "卖量", false),
                HeaderDef::new("ask_offset", "卖开平", false),
                HeaderDef::new("status", "状态", true),
                HeaderDef::new("datetime", "时间", true),
                HeaderDef::new("gateway_name", "接口", false),
            ],
        }
    }

    pub fn update(&mut self, quote: &QuoteData) {
        let vt_quoteid = quote.vt_quoteid();
        self.data.insert(vt_quoteid, QuoteRow::from(quote));
    }

    /// Sort quote rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&QuoteRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| b.datetime.cmp(&a.datetime));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.quoteid.cmp(&b.quoteid))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.reference.cmp(&b.reference))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.symbol.cmp(&b.symbol))),
            3 => rows.sort_by(|a, b| sort.apply_order(a.exchange.cmp(&b.exchange))),
            4 => rows.sort_by(|a, b| sort.apply_order(a.bid_offset.cmp(&b.bid_offset))),
            5 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.bid_volume
                        .partial_cmp(&b.bid_volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            6 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.bid_price
                        .partial_cmp(&b.bid_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            7 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.ask_price
                        .partial_cmp(&b.ask_price)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            8 => rows.sort_by(|a, b| {
                sort.apply_order(
                    a.ask_volume
                        .partial_cmp(&b.ask_volume)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            }),
            9 => rows.sort_by(|a, b| sort.apply_order(a.ask_offset.cmp(&b.ask_offset))),
            10 => rows
                .sort_by(|a, b| sort.apply_order(a.status.to_string().cmp(&b.status.to_string()))),
            11 => rows.sort_by(|a, b| sort.apply_order(a.datetime.cmp(&b.datetime))),
            12 => rows.sort_by(|a, b| sort.apply_order(a.gateway_name.cmp(&b.gateway_name))),
            _ => rows.sort_by(|a, b| b.datetime.cmp(&a.datetime)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        let mut cancel_quoteid = None;
        let available_height = ui.available_height();

        ui.label("双击报价行撤销");

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(60.0), self.headers.len())
            .min_scrolled_height(available_height)
            .header(20.0, |mut header| {
                for (i, h) in self.headers.iter().enumerate() {
                    header.col(|ui| {
                        sortable_header(ui, h.display, i, &mut self.sort);
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<_> = self.data.values().collect();
                self.sort_rows(&mut rows);

                for row in rows {
                    body.row(18.0, |mut table_row| {
                        table_row.col(|ui| {
                            if ui.selectable_label(false, &row.quoteid).double_clicked() {
                                cancel_quoteid = Some(row.vt_quoteid.clone());
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.reference);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.symbol);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.exchange);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.bid_offset);
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", row.bid_volume)).color(COLOR_BID),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format_price(row.bid_price, None)).color(COLOR_BID),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format_price(row.ask_price, None)).color(COLOR_ASK),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(
                                RichText::new(format!("{:.0}", row.ask_volume)).color(COLOR_ASK),
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(&row.ask_offset);
                        });
                        table_row.col(|ui| {
                            ui.label(row.status.to_string());
                        });
                        table_row.col(|ui| {
                            ui.label(&row.datetime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.gateway_name);
                        });
                    });
                }
            });

        cancel_quoteid
    }
}
