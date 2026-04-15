//! Main window for the trading platform.
//!
//! This module implements the main application window with dock panels
//! for various trading monitors and widgets.

use egui::{Context, Ui, TopBottomPanel, SidePanel, CentralPanel};
use egui::containers::menu::MenuBar;

use super::widget::*;
use super::trading::TradingWidget;
use super::dialogs::*;
use super::style::*;
use super::backtesting_panel::BacktestingPanel;
use std::sync::Arc;
use std::collections::HashMap;
use crate::chart::ChartWidget;
use crate::trader::object::BarData;
use crate::mcp::UICommand;
use chrono::{Utc, Timelike, Duration, Datelike};

/// Panel visibility state
#[derive(Default)]
pub struct PanelState {
    pub show_trading: bool,
    pub show_tick: bool,
    pub show_order: bool,
    pub show_active_order: bool,
    pub show_trade: bool,
    pub show_position: bool,
    pub show_account: bool,
    pub show_log: bool,
    pub show_quote: bool,
}

impl PanelState {
    pub fn new() -> Self {
        Self {
            show_trading: true,
            show_tick: true,
            show_order: true,
            show_active_order: true,
            show_trade: true,
            show_position: true,
            show_account: true,
            show_log: true,
            show_quote: false,
        }
    }
}

/// Tab selection for the central panel
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum CentralTab {
    #[default]
    Tick,
    Order,
    ActiveOrder,
    Trade,
    Quote,
    Backtesting,
}

/// Tab selection for the bottom panel
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    #[default]
    Log,
    Account,
    Position,
}

/// Main window state
pub struct MainWindow {
    // Main engine reference
    main_engine: Option<Arc<crate::trader::MainEngine>>,
    
    // Window state
    pub title: String,
    pub panels: PanelState,
    pub central_tab: CentralTab,
    pub bottom_tab: BottomTab,
    
    // Trading widget
    pub trading: TradingWidget,
    
    // Monitor widgets
    pub tick_monitor: TickMonitor,
    pub order_monitor: OrderMonitor,
    pub active_order_monitor: ActiveOrderMonitor,
    pub trade_monitor: TradeMonitor,
    pub position_monitor: PositionMonitor,
    pub account_monitor: AccountMonitor,
    pub log_monitor: LogMonitor,
    pub quote_monitor: QuoteMonitor,
    pub backtesting_panel: BacktestingPanel,
    
    // Dialogs
    pub connect_dialogs: Vec<ConnectDialog>,
    pub about_dialog: AboutDialog,
    pub global_settings: GlobalSettingsDialog,
    pub contract_manager: ContractManagerDialog,
    
    // Gateway list for menu
    pub gateway_names: Vec<String>,
    
    // Chart windows
    pub charts: HashMap<String, ChartWidget>,
    
    // Tick aggregators for each symbol
    tick_aggregators: HashMap<String, TickBarAggregator>,
    
    // Pending history data from async queries
    pub pending_history_data: Arc<tokio::sync::Mutex<HashMap<String, Vec<BarData>>>>,
    
    // Actions pending from UI
    pub pending_connect: Option<(String, std::collections::HashMap<String, serde_json::Value>)>,
    pub pending_cancel_order: Option<String>,
    pub pending_cancel_quote: Option<String>,
    pub pending_close: bool,
}

impl Default for MainWindow {
    fn default() -> Self {
        Self::new("Trade Engine")
    }
}

impl MainWindow {
    pub fn new(title: &str) -> Self {
        Self {
            main_engine: None,
            title: title.to_string(),
            panels: PanelState::new(),
            central_tab: CentralTab::default(),
            bottom_tab: BottomTab::default(),
            trading: TradingWidget::new(),
            tick_monitor: TickMonitor::new(),
            order_monitor: OrderMonitor::new(),
            active_order_monitor: ActiveOrderMonitor::new(),
            trade_monitor: TradeMonitor::new(),
            position_monitor: PositionMonitor::new(),
            account_monitor: AccountMonitor::new(),
            log_monitor: LogMonitor::new(),
            quote_monitor: QuoteMonitor::new(),
            backtesting_panel: BacktestingPanel::new(),
            connect_dialogs: Vec::new(),
            about_dialog: AboutDialog::new(),
            global_settings: GlobalSettingsDialog::new(),
            contract_manager: ContractManagerDialog::new(),
            gateway_names: Vec::new(),
            charts: HashMap::new(),
            tick_aggregators: HashMap::new(),
            pending_history_data: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            pending_connect: None,
            pending_cancel_order: None,
            pending_cancel_quote: None,
            pending_close: false,
        }
    }
    
    /// Set reference to main engine
    pub fn set_main_engine(&mut self, engine: Arc<crate::trader::MainEngine>) {
        self.main_engine = Some(engine);
    }
    
    /// Update UI data from main engine
    pub fn update_data(&mut self) {
        if let Some(ref engine) = self.main_engine {
            // Check for pending history data
            if let Ok(mut pending) = self.pending_history_data.try_lock() {
                for (vt_symbol, bars) in pending.drain() {
                    if let Some(chart) = self.charts.get_mut(&vt_symbol) {
                        tracing::info!("加载历史数据到图表: {} ({} 条)", vt_symbol, bars.len());
                        chart.update_history(bars);
                        chart.set_loading_history(false);
                    }
                }
            }
            
            // Update tick monitor and trading widget
            for tick in engine.get_all_ticks() {
                self.tick_monitor.update(&tick);
                self.trading.update_tick(&tick);
                
                // Update chart if exists using tick aggregator
                let vt_symbol = tick.vt_symbol();
                if self.charts.contains_key(&vt_symbol) {
                    // Get or create aggregator for this symbol
                    let aggregator = self.tick_aggregators
                        .entry(vt_symbol.clone())
                        .or_insert_with(|| TickBarAggregator::new(crate::trader::Interval::Minute));
                    
                    // Update with tick and check if bar is completed
                    if let Some(bar) = aggregator.update_tick(&tick) {
                        // Bar completed, update chart
                        if let Some(chart) = self.charts.get_mut(&vt_symbol) {
                            chart.update_bar(bar);
                        }
                    }
                }
            }
            
            // Update order monitor
            for order in engine.get_all_orders() {
                self.order_monitor.update(&order);
            }
            
            // Update active order monitor
            for order in engine.get_all_active_orders() {
                self.active_order_monitor.update(&order);
            }
            
            // Update trade monitor
            for trade in engine.get_all_trades() {
                self.trade_monitor.update(&trade);
            }
            
            // Update position monitor
            for position in engine.get_all_positions() {
                self.position_monitor.update(&position);
            }
            
            // Update account monitor
            for account in engine.get_all_accounts() {
                self.account_monitor.update(&account);
            }
            
            // Update log monitor - only sync new logs
            let logs = engine.get_all_logs();
            self.log_monitor.sync_logs(&logs);
            
            // Update contract manager
            let contracts: Vec<ContractRow> = engine.get_all_contracts()
                .into_iter()
                .map(|c| ContractRow {
                    vt_symbol: c.vt_symbol(),
                    symbol: c.symbol,
                    exchange: format!("{:?}", c.exchange),
                    name: c.name,
                    product: format!("{:?}", c.product),
                    size: c.size,
                    pricetick: c.pricetick,
                    min_volume: c.min_volume,
                    gateway_name: c.gateway_name,
                })
                .collect();
            self.contract_manager.set_contracts(contracts);
            
            // Update trading widget with available contracts for dropdown
            let all_contracts = engine.get_all_contracts();
            self.trading.set_contracts(all_contracts);
        }
    }
    
    /// Set available gateways
    pub fn set_gateways(&mut self, gateways: Vec<String>) {
        self.gateway_names = gateways.clone();
        self.trading.set_gateways(gateways.clone());
        
        // Create connect dialogs for each gateway
        self.connect_dialogs = gateways
            .into_iter()
            .map(|name| {
                let mut dialog = ConnectDialog::new(&name);
                // Set default settings for each gateway
                dialog.set_default_settings(Self::get_gateway_default_settings(&name));
                dialog
            })
            .collect();
    }
    
    /// Get default settings for a gateway
    fn get_gateway_default_settings(gateway_name: &str) -> std::collections::HashMap<String, serde_json::Value> {
        use serde_json::json;
        use crate::gateway::binance::BinanceConfigs;
        
        // 尝试从保存的配置加载
        let configs = BinanceConfigs::load();
        if let Some(config) = configs.get(gateway_name) {
            let mut settings = std::collections::HashMap::new();
            settings.insert("key".to_string(), json!(config.key));
            settings.insert("secret".to_string(), json!(config.secret));
            settings.insert("server".to_string(), json!(config.server));
            settings.insert("proxy_host".to_string(), json!(config.proxy_host));
            settings.insert("proxy_port".to_string(), json!(config.proxy_port));
            return settings;
        }
        
        // 如果没有保存的配置，返回空的默认配置
        match gateway_name {
            "BINANCE_USDT" | "BINANCE_SPOT" => {
                let mut settings = std::collections::HashMap::new();
                settings.insert("key".to_string(), json!(""));
                settings.insert("secret".to_string(), json!(""));
                settings.insert("server".to_string(), json!("REAL"));
                settings.insert("proxy_host".to_string(), json!(""));
                settings.insert("proxy_port".to_string(), json!(0));
                settings
            }
            _ => std::collections::HashMap::new(),
        }
    }
    
    /// Apply dark theme
    pub fn setup_style(&self, ctx: &Context) {
        apply_dark_theme(ctx);
    }
    
    /// Show the main window UI
    pub fn show(&mut self, ctx: &Context) {
        // Top menu bar
        self.show_menu_bar(ctx);
        
        // Left panel - trading widget
        if self.panels.show_trading {
            SidePanel::left("trading_panel")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.heading("交易");
                    ui.separator();
                    self.trading.show(ui);
                });
        }
        
        // Bottom panel - log, account, position
        TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .default_height(200.0)
            .show(ctx, |ui| {
                self.show_bottom_tabs(ui);
            });
        
        // Central panel - tick, orders, trades
        CentralPanel::default().show(ctx, |ui| {
            self.show_central_tabs(ui);
        });
        
        // Dialogs
        self.show_dialogs(ctx);
        
        // Chart windows
        self.show_chart_windows(ctx);
        
        // Process pending actions
        self.process_pending_actions();
    }
    
    fn show_menu_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            MenuBar::new().ui(ui, |ui| {
                // System menu
                ui.menu_button("系统", |ui| {
                    // Gateway connections
                    for i in 0..self.gateway_names.len() {
                        let name = &self.gateway_names[i];
                        if ui.button(format!("连接 {}", name)).clicked() {
                            if let Some(dialog) = self.connect_dialogs.get_mut(i) {
                                dialog.open();
                            }
                            ui.close();
                        }
                    }
                    
                    ui.separator();
                    
                    if ui.button("退出").clicked() {
                        self.pending_close = true;
                        ui.close();
                    }
                });
                
                // View menu
                ui.menu_button("视图", |ui| {
                    ui.checkbox(&mut self.panels.show_trading, "交易面板");
                    ui.checkbox(&mut self.panels.show_tick, "行情");
                    ui.checkbox(&mut self.panels.show_order, "委托");
                    ui.checkbox(&mut self.panels.show_active_order, "活动委托");
                    ui.checkbox(&mut self.panels.show_trade, "成交");
                    ui.checkbox(&mut self.panels.show_position, "持仓");
                    ui.checkbox(&mut self.panels.show_account, "资金");
                    ui.checkbox(&mut self.panels.show_log, "日志");
                    ui.checkbox(&mut self.panels.show_quote, "报价");
                });
                
                // Settings
                if ui.button("配置").clicked() {
                    // Load and pass current settings to dialog
                    use crate::trader::setting::SETTINGS;
                    let current_settings = SETTINGS.get_all();
                    let settings_map: std::collections::HashMap<String, serde_json::Value> = current_settings
                        .into_iter()
                        .map(|(k, v)| {
                            let json_val = match v {
                                crate::trader::setting::SettingValue::String(s) => serde_json::Value::String(s),
                                crate::trader::setting::SettingValue::Int(i) => serde_json::Value::Number(i.into()),
                                crate::trader::setting::SettingValue::Float(f) => serde_json::json!(f),
                                crate::trader::setting::SettingValue::Bool(b) => serde_json::Value::Bool(b),
                            };
                            (k, json_val)
                        })
                        .collect();
                    self.global_settings.open(settings_map);
                }
                
                // Help menu
                ui.menu_button("帮助", |ui| {
                    if ui.button("查询合约").clicked() {
                        self.contract_manager.open();
                        ui.close();
                    }
                    
                    ui.separator();
                    
                    if ui.button("关于").clicked() {
                        self.about_dialog.open();
                        ui.close();
                    }
                });
            });
        });
    }
    
    fn show_central_tabs(&mut self, ui: &mut Ui) {
        // Tab buttons
        ui.horizontal(|ui| {
            if self.panels.show_tick {
                ui.selectable_value(&mut self.central_tab, CentralTab::Tick, "行情");
            }
            if self.panels.show_order {
                ui.selectable_value(&mut self.central_tab, CentralTab::Order, "委托");
            }
            if self.panels.show_active_order {
                ui.selectable_value(&mut self.central_tab, CentralTab::ActiveOrder, "活动");
            }
            if self.panels.show_trade {
                ui.selectable_value(&mut self.central_tab, CentralTab::Trade, "成交");
            }
            if self.panels.show_quote {
                ui.selectable_value(&mut self.central_tab, CentralTab::Quote, "报价");
            }
            ui.selectable_value(&mut self.central_tab, CentralTab::Backtesting, "回测");
        });
        
        ui.separator();
        
        // Tab content
        match self.central_tab {
            CentralTab::Tick => {
                if let Some(vt_symbol) = self.tick_monitor.show(ui) {
                    // Open chart window for the clicked symbol
                    self.open_chart(&vt_symbol);
                }
            }
            CentralTab::Order => {
                if let Some(vt_orderid) = self.order_monitor.show(ui) {
                    self.pending_cancel_order = Some(vt_orderid);
                }
            }
            CentralTab::ActiveOrder => {
                if let Some(vt_orderid) = self.active_order_monitor.show(ui) {
                    self.pending_cancel_order = Some(vt_orderid);
                }
            }
            CentralTab::Trade => {
                self.trade_monitor.show(ui);
            }
            CentralTab::Quote => {
                if let Some(vt_quoteid) = self.quote_monitor.show(ui) {
                    self.pending_cancel_quote = Some(vt_quoteid);
                }
            }
            CentralTab::Backtesting => {
                let ctx = ui.ctx().clone();
                self.backtesting_panel.ui(&ctx, ui);
            }
        }
    }
    
    fn show_bottom_tabs(&mut self, ui: &mut Ui) {
        // Tab buttons
        ui.horizontal(|ui| {
            if self.panels.show_log {
                ui.selectable_value(&mut self.bottom_tab, BottomTab::Log, "日志");
            }
            if self.panels.show_account {
                ui.selectable_value(&mut self.bottom_tab, BottomTab::Account, "资金");
            }
            if self.panels.show_position {
                ui.selectable_value(&mut self.bottom_tab, BottomTab::Position, "持仓");
            }
        });
        
        ui.separator();
        
        // Tab content
        match self.bottom_tab {
            BottomTab::Log => {
                self.log_monitor.show(ui);
            }
            BottomTab::Account => {
                self.account_monitor.show(ui);
            }
            BottomTab::Position => {
                if let Some(position) = self.position_monitor.show(ui) {
                    // Set trading widget for quick close - find exchange from position data
                    // The exchange string needs to be matched to the enum
                    let exchange = match position.exchange.as_str() {
                        "BINANCE" => crate::trader::constant::Exchange::Binance,
                        "CFFEX" => crate::trader::constant::Exchange::Cffex,
                        "SHFE" => crate::trader::constant::Exchange::Shfe,
                        "DCE" => crate::trader::constant::Exchange::Dce,
                        "CZCE" => crate::trader::constant::Exchange::Czce,
                        "INE" => crate::trader::constant::Exchange::Ine,
                        "SSE" => crate::trader::constant::Exchange::Sse,
                        "SZSE" => crate::trader::constant::Exchange::Szse,
                        other => {
                            tracing::warn!("Unknown exchange '{}', defaulting to Binance", other);
                            crate::trader::constant::Exchange::Binance
                        }
                    };
                    self.trading.set_symbol(&position.symbol, exchange);
                }
            }
        }
    }
    
    fn show_dialogs(&mut self, ctx: &Context) {
        // Connect dialogs
        for dialog in &mut self.connect_dialogs {
            dialog.show(ctx);
            
            if dialog.take_connect() {
                let settings = dialog.get_settings();
                self.pending_connect = Some((dialog.gateway_name.clone(), settings));
            }
        }
        
        // About dialog
        self.about_dialog.show(ctx);
        
        // Global settings dialog
        self.global_settings.show(ctx);
        
        // Save global settings if user confirmed
        if self.global_settings.take_save() {
            use crate::trader::setting::{SETTINGS, SettingValue};
            let settings_map = self.global_settings.get_settings();
            let mut new_settings = std::collections::HashMap::new();
            
            for (key, value) in settings_map {
                let setting_val = match value {
                    serde_json::Value::String(s) => SettingValue::String(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            SettingValue::Int(i)
                        } else if let Some(f) = n.as_f64() {
                            SettingValue::Float(f)
                        } else {
                            continue;
                        }
                    }
                    serde_json::Value::Bool(b) => SettingValue::Bool(b),
                    _ => continue,
                };
                new_settings.insert(key, setting_val);
            }
            
            SETTINGS.update(new_settings);
            if let Err(e) = SETTINGS.save() {
                tracing::warn!("保存配置失败: {}", e);
            } else {
                tracing::info!("配置已保存");
            }
        }
        
        // Contract manager dialog
        self.contract_manager.show(ctx);
    }
    
    fn process_pending_actions(&mut self) {
        // Cancel all from trading widget
        if self.trading.take_cancel_all() {
            // This will be handled by the app
        }
    }
    
    /// Calculate appropriate history query duration based on interval
    /// More granular intervals need shorter windows, coarser intervals need longer windows
    fn history_duration_for_interval(interval: crate::trader::Interval) -> Duration {
        match interval {
            crate::trader::Interval::Second => Duration::hours(4),
            crate::trader::Interval::Minute => Duration::days(3),
            crate::trader::Interval::Minute15 => Duration::days(14),
            crate::trader::Interval::Hour => Duration::days(30),
            crate::trader::Interval::Hour4 => Duration::days(90),
            crate::trader::Interval::Daily => Duration::days(365),
            crate::trader::Interval::Weekly => Duration::days(730),
            crate::trader::Interval::Tick => Duration::hours(1),
        }
    }
    
    /// Open or focus chart window for a symbol
    pub fn open_chart(&mut self, vt_symbol: &str) {
        if !self.charts.contains_key(vt_symbol) {
            // Parse vt_symbol to get symbol and exchange
            let parts: Vec<&str> = vt_symbol.split('.').collect();
            if parts.len() != 2 {
                tracing::warn!("Invalid vt_symbol format: {}", vt_symbol);
                return;
            }
            
            let symbol = parts[0];
            let exchange_str = parts[1];
            
            // Find the appropriate gateway
            let gateway_name = if exchange_str.contains("BINANCE") || exchange_str == "Binance" {
                if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                    Some("BINANCE_SPOT")
                } else if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                    Some("BINANCE_USDT")
                } else {
                    None
                }
            } else {
                None
            };
            
            if gateway_name.is_none() {
                tracing::warn!("No gateway available for {}", vt_symbol);
                return;
            }
            
            // Create new chart widget
            let mut chart = ChartWidget::new();
            chart.set_price_decimals(2);
            chart.set_show_volume(true);
            
            let interval = chart.get_interval();
            let duration = Self::history_duration_for_interval(interval);
            
            self.charts.insert(vt_symbol.to_string(), chart);
            tracing::info!("打开K线图: {}", vt_symbol);
            
            // Query historical data from main engine
            if let Some(ref engine) = self.main_engine {
                let gw_name = gateway_name.unwrap_or_default().to_string();
                let sym = symbol.to_string();
                let vt_sym = vt_symbol.to_string();
                
                // Parse exchange
                let exchange = crate::trader::utility::extract_vt_symbol(vt_symbol)
                    .map(|(_, e)| e)
                    .unwrap_or(crate::trader::Exchange::Binance);
                
                let req = crate::trader::HistoryRequest {
                    symbol: sym,
                    exchange,
                    start: Utc::now() - duration,
                    end: Some(Utc::now()),
                    interval: Some(interval),
                };
                
                let engine_clone = engine.clone();
                let pending_data = self.pending_history_data.clone();
                
                // Spawn async task to query history
                tokio::spawn(async move {
                    match engine_clone.query_history(req, &gw_name).await {
                        Ok(bars) => {
                            tracing::info!("查询到历史数据: {} 条, symbol: {}", bars.len(), vt_sym);
                            // Store bars in pending data for UI thread to pick up
                            let mut pending = pending_data.lock().await;
                            pending.insert(vt_sym.clone(), bars);
                        }
                        Err(e) => {
                            tracing::warn!("查询历史数据失败: {}, symbol: {}", e, vt_sym);
                        }
                    }
                });
            }
        }
    }
    
    /// Show all chart windows
    fn show_chart_windows(&mut self, ctx: &Context) {
        let mut to_remove = Vec::new();
        let mut interval_changes: Vec<(String, crate::trader::Interval)> = Vec::new();
        let mut need_more_history: Vec<(String, chrono::DateTime<chrono::Utc>, crate::trader::Interval)> = Vec::new();
        
        for (vt_symbol, chart) in &mut self.charts {
            let mut is_open = true;
            egui::Window::new(format!("K线图 - {}", vt_symbol))
                .id(egui::Id::new(format!("chart_{}", vt_symbol)))
                .default_size([800.0, 600.0])
                .open(&mut is_open)
                .show(ctx, |ui| {
                    let (_, event) = chart.show(ui, Some(vt_symbol));
                    if let Some(evt) = event {
                        if evt.interval_changed {
                            interval_changes.push((
                                evt.symbol.unwrap_or_else(|| vt_symbol.clone()),
                                evt.new_interval,
                            ));
                        }
                        if evt.need_more_history {
                            if let Some(earliest) = chart.get_earliest_bar_time() {
                                let interval = chart.get_interval();
                                need_more_history.push((vt_symbol.clone(), earliest, interval));
                                chart.set_loading_history(true);
                            }
                        }
                    }
                });
            
            if !is_open {
                to_remove.push(vt_symbol.clone());
            }
        }
        
        for vt_symbol in to_remove {
            self.charts.remove(&vt_symbol);
            tracing::info!("关闭K线图: {}", vt_symbol);
        }

        for (vt_symbol, interval) in interval_changes {
            if let Some(chart) = self.charts.get_mut(&vt_symbol) {
                chart.clear_data();
            }
            if let Some(aggregator) = self.tick_aggregators.get_mut(&vt_symbol) {
                aggregator.set_interval(interval);
            }

            if let Some(ref engine) = self.main_engine {
                let (sym, exchange) = crate::trader::utility::extract_vt_symbol(&vt_symbol)
                    .unwrap_or((vt_symbol.clone(), crate::trader::Exchange::Binance));
                let duration = Self::history_duration_for_interval(interval);
                let req = crate::trader::HistoryRequest {
                    symbol: sym,
                    exchange,
                    start: chrono::Utc::now() - duration,
                    end: Some(chrono::Utc::now()),
                    interval: Some(interval),
                };
                
                // Find appropriate gateway based on exchange
                let gateway_name = if exchange == crate::trader::Exchange::Binance {
                    if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                        "BINANCE_SPOT".to_string()
                    } else if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                        "BINANCE_USDT".to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                
                let engine_clone = engine.clone();
                let pending_data = self.pending_history_data.clone();
                let vt_sym = vt_symbol.clone();
                tokio::spawn(async move {
                    match engine_clone.query_history(req, &gateway_name).await {
                        Ok(bars) => {
                            tracing::info!("周期切换查询到历史数据: {} 条, symbol: {}", bars.len(), vt_sym);
                            let mut data = pending_data.lock().await;
                            data.insert(vt_sym, bars);
                        }
                        Err(e) => {
                            tracing::warn!("周期切换查询历史数据失败: {}", e);
                        }
                    }
                });
            }
        }

        // Handle requests for more historical data (drag-to-load)
        for (vt_symbol, earliest_time, interval) in need_more_history {
            if let Some(ref engine) = self.main_engine {
                let (sym, exchange) = crate::trader::utility::extract_vt_symbol(&vt_symbol)
                    .unwrap_or((vt_symbol.clone(), crate::trader::Exchange::Binance));
                let duration = Self::history_duration_for_interval(interval);
                let req = crate::trader::HistoryRequest {
                    symbol: sym,
                    exchange,
                    start: earliest_time - duration,
                    end: Some(earliest_time),
                    interval: Some(interval),
                };

                let gateway_name = if exchange == crate::trader::Exchange::Binance {
                    if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                        "BINANCE_SPOT".to_string()
                    } else if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                        "BINANCE_USDT".to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let engine_clone = engine.clone();
                let pending_data = self.pending_history_data.clone();
                let vt_sym = vt_symbol.clone();
                tokio::spawn(async move {
                    match engine_clone.query_history(req, &gateway_name).await {
                        Ok(bars) => {
                            tracing::info!("加载更多历史数据: {} 条, symbol: {}", bars.len(), vt_sym);
                            let mut data = pending_data.lock().await;
                            data.insert(vt_sym, bars);
                        }
                        Err(e) => {
                            tracing::warn!("加载更多历史数据失败: {}", e);
                        }
                    }
                });
            }
        }
    }
    
    /// Take pending connect action
    pub fn take_connect(&mut self) -> Option<(String, std::collections::HashMap<String, serde_json::Value>)> {
        self.pending_connect.take()
    }
    
    /// Take pending subscribe action
    pub fn take_subscribe(&mut self) -> Option<(crate::trader::SubscribeRequest, String)> {
        if let Some(req) = self.trading.take_subscribe() {
            // Find the appropriate gateway for this exchange
            let gateway_name = match req.exchange {
                crate::trader::Exchange::Binance => {
                    // Check if it's spot or usdt based on gateway availability
                    if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                        "BINANCE_SPOT".to_string()
                    } else if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                        "BINANCE_USDT".to_string()
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            return Some((req, gateway_name));
        }
        None
    }
    
    /// Take pending order action
    pub fn take_order(&mut self) -> Option<(crate::trader::OrderRequest, String)> {
        self.trading.take_order()
    }
    
    /// Take pending cancel order action
    pub fn take_cancel_order(&mut self) -> Option<String> {
        self.pending_cancel_order.take()
    }
    
    /// Take pending cancel quote action
    pub fn take_cancel_quote(&mut self) -> Option<String> {
        self.pending_cancel_quote.take()
    }
    
    /// Check if close is requested
    pub fn should_close(&mut self) -> bool {
        let result = self.pending_close;
        self.pending_close = false;
        result
    }
    
    /// Handle MCP command from AI assistant
    pub fn handle_mcp_command(&mut self, cmd: UICommand) {
        match cmd {
            UICommand::SwitchSymbol { symbol } => {
                // Set the symbol in trading widget - parse vt_symbol if present
                if let Some((sym, exchange)) = crate::trader::utility::extract_vt_symbol(&symbol) {
                    self.trading.set_symbol(&sym, exchange);
                } else {
                    self.trading.set_symbol(&symbol, crate::trader::Exchange::Binance);
                }
                // Open chart for this symbol
                self.open_chart(&symbol);
                tracing::info!("MCP: 切换到品种 {}", symbol);
            }
            
            UICommand::SwitchInterval { interval } => {
                // Find the active chart and switch interval
                if let Some((_, chart)) = self.charts.iter_mut().next() {
                    if let Ok(interval_enum) = parse_interval(&interval) {
                        chart.set_interval(interval_enum);
                    }
                }
                tracing::info!("MCP: 切换周期到 {}", interval);
            }
            
            UICommand::AddIndicator { indicator_type, period } => {
                // Add indicator to active chart
                if let Some((_, chart)) = self.charts.iter_mut().next() {
                    let indicator = create_indicator(&indicator_type, period.unwrap_or(20));
                    chart.add_indicator(indicator);
                }
                tracing::info!("MCP: 添加指标 {} (周期: {:?})", indicator_type, period);
            }
            
            UICommand::RemoveIndicator { index } => {
                if let Some((_, chart)) = self.charts.iter_mut().next() {
                    chart.remove_indicator(index);
                }
                tracing::info!("MCP: 删除指标 {}", index);
            }
            
            UICommand::ClearIndicators => {
                if let Some((_, chart)) = self.charts.iter_mut().next() {
                    chart.clear_indicators();
                }
                tracing::info!("MCP: 清除所有指标");
            }
            
            UICommand::NavigateTo { tab } => {
                // Switch to the specified tab
                match tab.as_str() {
                    "tick" => self.central_tab = CentralTab::Tick,
                    "order" => self.central_tab = CentralTab::Order,
                    "active_order" => self.central_tab = CentralTab::ActiveOrder,
                    "trade" => self.central_tab = CentralTab::Trade,
                    "backtesting" => self.central_tab = CentralTab::Backtesting,
                    "log" => self.bottom_tab = BottomTab::Log,
                    "account" => self.bottom_tab = BottomTab::Account,
                    "position" => self.bottom_tab = BottomTab::Position,
                    _ => {}
                }
                tracing::info!("MCP: 切换到标签页 {}", tab);
            }
            
            UICommand::ShowNotification { message, level } => {
                // Log the notification (toast UI can be added later)
                match level.as_str() {
                    "error" => tracing::error!("MCP 通知: {}", message),
                    "warn" => tracing::warn!("MCP 通知: {}", message),
                    _ => tracing::info!("MCP 通知: {}", message),
                }
            }
            
            // Backend commands - these go through the pending system
            UICommand::Connect { gateway_name, settings } => {
                if let serde_json::Value::Object(map) = settings {
                    let mut hm = std::collections::HashMap::new();
                    for (k, v) in map {
                        hm.insert(k, v);
                    }
                    self.pending_connect = Some((gateway_name, hm));
                }
                tracing::info!("MCP: 连接网关");
            }
            
            UICommand::Subscribe { symbol, exchange, gateway_name } => {
                let exchange_enum = parse_exchange(&exchange);
                let req = crate::trader::SubscribeRequest::new(symbol, exchange_enum);
                self.trading.set_subscribe_request(req, &gateway_name);
                tracing::info!("MCP: 订阅行情");
            }
            
            UICommand::SendOrder { symbol, exchange, direction, order_type, volume, price, offset, gateway_name } => {
                let exchange_enum = parse_exchange(&exchange);
                let direction_enum = parse_direction(&direction);
                let order_type_enum = parse_order_type(&order_type);
                let offset_enum = offset
                    .as_deref()
                    .map(parse_offset)
                    .unwrap_or(crate::trader::Offset::None);
                
                let req = crate::trader::OrderRequest {
                    symbol,
                    exchange: exchange_enum,
                    direction: direction_enum,
                    order_type: order_type_enum,
                    volume,
                    price: price.unwrap_or(0.0),
                    offset: offset_enum,
                    reference: "MCP".to_string(),
                };
                self.trading.set_order_request(req, &gateway_name);
                tracing::info!("MCP: 下单请求");
            }
            
            UICommand::CancelOrder { order_id, symbol: _, exchange: _, gateway_name: _ } => {
                self.pending_cancel_order = Some(order_id);
                tracing::info!("MCP: 撤单请求");
            }
        }
    }
}

/// Parse interval string to Interval enum
fn parse_interval(s: &str) -> Result<crate::trader::Interval, String> {
    match s.to_lowercase().as_str() {
        "1s" | "second" => Ok(crate::trader::Interval::Second),
        "1m" | "minute" => Ok(crate::trader::Interval::Minute),
        "15m" => Ok(crate::trader::Interval::Minute15),
        "1h" | "hour" => Ok(crate::trader::Interval::Hour),
        "4h" => Ok(crate::trader::Interval::Hour4),
        "1d" | "day" | "daily" => Ok(crate::trader::Interval::Daily),
        "1w" | "week" | "weekly" => Ok(crate::trader::Interval::Weekly),
        _ => Err(format!("Unknown interval: {}", s)),
    }
}

/// Parse exchange string to Exchange enum
fn parse_exchange(s: &str) -> crate::trader::Exchange {
    match s.to_uppercase().as_str() {
        "BINANCE" | "BINANCE_SPOT" => crate::trader::Exchange::Binance,
        "BINANCE_USDM" | "BINANCE_USDT" => crate::trader::Exchange::BinanceUsdm,
        "BINANCE_COINM" => crate::trader::Exchange::BinanceCoinm,
        _ => crate::trader::Exchange::Local,
    }
}

/// Parse direction string to Direction enum
fn parse_direction(s: &str) -> crate::trader::Direction {
    match s.to_lowercase().as_str() {
        "long" | "buy" | "多" => crate::trader::Direction::Long,
        "short" | "sell" | "空" => crate::trader::Direction::Short,
        _ => crate::trader::Direction::Net,
    }
}

/// Parse order type string to OrderType enum
fn parse_order_type(s: &str) -> crate::trader::OrderType {
    match s.to_lowercase().as_str() {
        "limit" | "限价" => crate::trader::OrderType::Limit,
        "market" | "市价" => crate::trader::OrderType::Market,
        "fak" => crate::trader::OrderType::Fak,
        "fok" => crate::trader::OrderType::Fok,
        "stop" => crate::trader::OrderType::Stop,
        _ => crate::trader::OrderType::Limit,
    }
}

/// Parse offset string to Offset enum
fn parse_offset(s: &str) -> crate::trader::Offset {
    match s.to_lowercase().as_str() {
        "open" | "开仓" | "开" => crate::trader::Offset::Open,
        "close" | "平仓" | "平" => crate::trader::Offset::Close,
        "closetoday" | "平今" => crate::trader::Offset::CloseToday,
        "closeyesterday" | "平昨" => crate::trader::Offset::CloseYesterday,
        _ => crate::trader::Offset::None,
    }
}

/// Create an indicator Box from type string and period
fn create_indicator(indicator_type: &str, period: usize) -> Box<dyn crate::chart::Indicator> {
    use crate::chart::*;
    use egui::Color32;
    
    let main_loc = IndicatorLocation::Main;
    let sub_loc = IndicatorLocation::Sub;
    
    match indicator_type.to_uppercase().as_str() {
        "MA" | "SMA" => Box::new(MA::new(period, Color32::YELLOW, main_loc)),
        "EMA" => Box::new(EMA::new(period, Color32::from_rgb(0, 200, 255), main_loc)),
        "WMA" => Box::new(WMA::new(period, Color32::from_rgb(255, 150, 0), main_loc)),
        "BOLL" | "BOLLINGER" => Box::new(BOLL::new(period, 2.0, main_loc)),
        "VWAP" => Box::new(VWAP::new(Color32::from_rgb(0, 255, 200), main_loc)),
        "AVL" => Box::new(AVL::new(Color32::WHITE, main_loc)),
        "TRIX" => Box::new(TRIX::new(period, 9, Color32::from_rgb(200, 100, 255), Color32::from_rgb(255, 100, 0), sub_loc)),
        "SAR" => Box::new(SAR::new(0.02, 0.2, Color32::from_rgb(0, 255, 0), main_loc)),
        "SUPER" | "SUPERTREND" => Box::new(SUPER::new(period, 3.0, Color32::GREEN, Color32::RED, main_loc)),
        _ => Box::new(MA::new(period, Color32::YELLOW, main_loc)),
    }
}

/// Tick to Bar Aggregator
/// Aggregates tick data into bars with proper OHLCV calculation
struct TickBarAggregator {
    interval: crate::trader::Interval,
    current_bar_start: Option<chrono::DateTime<chrono::Utc>>,
    open_price: f64,
    high_price: f64,
    low_price: f64,
    close_price: f64,
    volume: f64,
    turnover: f64,
    open_interest: f64,
    symbol: String,
    exchange: crate::trader::Exchange,
    gateway_name: String,
    last_volume: f64,
    last_turnover: f64,
}

impl TickBarAggregator {
    fn new(interval: crate::trader::Interval) -> Self {
        Self {
            interval,
            current_bar_start: None,
            open_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            close_price: 0.0,
            volume: 0.0,
            turnover: 0.0,
            open_interest: 0.0,
            symbol: String::new(),
            exchange: crate::trader::Exchange::Binance,
            gateway_name: String::new(),
            last_volume: 0.0,
            last_turnover: 0.0,
        }
    }
    
    fn set_interval(&mut self, interval: crate::trader::Interval) {
        self.interval = interval;
        self.current_bar_start = None;
    }

    fn update_tick(&mut self, tick: &crate::trader::object::TickData) -> Option<BarData> {
        let bar_start = self.get_bar_start_time(&tick.datetime);
        
        // Check if we need to complete the current bar
        let completed_bar = if let Some(current_start) = self.current_bar_start {
            if bar_start > current_start {
                // New bar period, complete the old one
                Some(self.build_bar(current_start))
            } else {
                None
            }
        } else {
            None
        };
        
        // Start new bar or update current bar
        if self.current_bar_start != Some(bar_start) {
            // New bar
            self.current_bar_start = Some(bar_start);
            self.symbol = tick.symbol.clone();
            self.exchange = tick.exchange;
            self.gateway_name = tick.gateway_name.clone();
            self.open_price = tick.last_price;
            self.high_price = tick.last_price;
            self.low_price = tick.last_price;
            self.close_price = tick.last_price;
            self.volume = 0.0;
            self.turnover = 0.0;
            self.open_interest = tick.open_interest;
            self.last_volume = tick.volume;
            self.last_turnover = tick.turnover;
        } else {
            // Update current bar
            self.high_price = self.high_price.max(tick.last_price);
            self.low_price = self.low_price.min(tick.last_price);
            self.close_price = tick.last_price;
            self.open_interest = tick.open_interest;
            
            // Calculate volume change (difference from last tick)
            let volume_change = tick.volume - self.last_volume;
            if volume_change > 0.0 {
                self.volume += volume_change;
            }
            
            let turnover_change = tick.turnover - self.last_turnover;
            if turnover_change > 0.0 {
                self.turnover += turnover_change;
            }
            
            self.last_volume = tick.volume;
            self.last_turnover = tick.turnover;
        }
        
        completed_bar
    }
    
    fn get_bar_start_time(&self, dt: &chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
        match self.interval {
            crate::trader::Interval::Second => {
                dt.with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Minute => {
                dt.with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Minute15 => {
                let minute = dt.minute();
                let rounded_minute = (minute / 15) * 15;
                dt.with_minute(rounded_minute).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Hour => {
                dt.with_minute(0).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Hour4 => {
                let hour = dt.hour();
                let rounded_hour = (hour / 4) * 4;
                dt.with_hour(rounded_hour).unwrap_or(*dt)
                    .with_minute(0).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Daily => {
                dt.with_hour(0).unwrap_or(*dt)
                    .with_minute(0).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Weekly => {
                let days_from_monday = dt.weekday().num_days_from_monday();
                let week_start = *dt - Duration::days(days_from_monday as i64);
                week_start
                    .with_hour(0).unwrap_or(week_start)
                    .with_minute(0).unwrap_or(week_start)
                    .with_second(0).unwrap_or(week_start)
                    .with_nanosecond(0).unwrap_or(week_start)
            }
            crate::trader::Interval::Tick => *dt,
        }
    }
    
    fn build_bar(&self, bar_start: chrono::DateTime<chrono::Utc>) -> BarData {
        BarData {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            datetime: bar_start,
            interval: Some(self.interval),
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            close_price: self.close_price,
            volume: self.volume,
            turnover: self.turnover,
            open_interest: self.open_interest,
            gateway_name: self.gateway_name.clone(),
            extra: None,
        }
    }
}
