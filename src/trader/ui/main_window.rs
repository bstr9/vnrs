//! Main window for the trading platform.
//!
//! This module implements the main application window with dock panels
//! for various trading monitors and widgets.

use egui::{Context, Ui, TopBottomPanel, SidePanel, CentralPanel, RichText, Color32};
use egui::containers::menu::MenuBar;

use super::widget::*;
use super::trading::TradingWidget;
use super::dialogs::*;
use super::style::*;
use super::style::{ToastManager, ToastType};
use super::backtesting_panel::BacktestingPanel;
use super::dashboard::{DashboardPanel, DashboardAction};
use super::strategy_panel::StrategyPanel;
use super::indicator_panel::IndicatorPanel;
use super::bracket_panel::BracketOrderPanel;
use super::advanced_orders_panel::AdvancedOrdersPanel;
use super::rpc_panel::RpcPanel;
#[cfg(feature = "alpha")]
use super::alpha_panel::AlphaPanel;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::chart::ChartWidget;
use crate::trader::object::BarData;
use crate::trader::toast::Toast;
use crate::trader::alert::AlertLevel;
use crate::mcp::UICommand;
use chrono::{Utc, Timelike, Duration, Datelike};
use crate::strategy::{StrategyEngine, StrategyState};

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
    Dashboard,
    Tick,
    Order,
    ActiveOrder,
    Trade,
    Quote,
    Strategy,
    Backtesting,
    Indicator,
    Alert,
    AdvancedOrders,
    BracketOrder,
    RpcMonitor,
    #[cfg(feature = "alpha")]
    AlphaResearch,
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
    
    // Theme
    pub dark_mode: bool,
    
    // Focus request for symbol input
    pub focus_symbol_input: bool,
    
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
    pub strategy_panel: StrategyPanel,
    pub dashboard_panel: DashboardPanel,
    pub indicator_panel: IndicatorPanel,
    pub advanced_orders_panel: AdvancedOrdersPanel,
    pub bracket_panel: BracketOrderPanel,
    pub rpc_panel: RpcPanel,
    #[cfg(feature = "alpha")]
    pub alpha_panel: super::alpha_panel::AlphaPanel,
    
    // Dialogs
    pub connect_dialogs: Vec<ConnectDialog>,
    pub about_dialog: AboutDialog,
    pub global_settings: GlobalSettingsDialog,
    pub contract_manager: ContractManagerDialog,
    
    // Toast notifications
    pub toast_manager: ToastManager,
    
    // Gateway list for menu
    pub gateway_names: Vec<String>,
    
    // Chart windows
    pub charts: HashMap<String, ChartWidget>,
    
    // Tick aggregators for each symbol
    tick_aggregators: HashMap<String, TickBarAggregator>,
    
    // Pending history data from async queries (initial load / interval change → jump to right)
    pub pending_history_data: Arc<tokio::sync::Mutex<HashMap<String, Vec<BarData>>>>,
    // Pending prepended history data (drag-to-load → preserve scroll position)
    pending_history_prepend: Arc<tokio::sync::Mutex<HashMap<String, Vec<BarData>>>>,
    
    // Strategy engine reference for async data sync
    strategy_engine: Option<Arc<StrategyEngine>>,
    // Cached strategy data for sync access from UI thread: (name, state, total_pnl)
    strategy_cache: Arc<Mutex<Vec<(String, StrategyState, f64)>>>,
    // Frame counter for periodic strategy data refresh
    strategy_update_counter: u32,
    // Frame counter for periodic dashboard data logging (every ~10 frames = 1 second)
    dashboard_log_counter: u32,
    
    // Alert history synced from backend ToastManager
    alert_history: Vec<Toast>,
    // Track last seen toast ID to detect new alerts
    last_alert_id: u64,
    
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
            dark_mode: true,
            focus_symbol_input: false,
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
            strategy_panel: StrategyPanel::new(),
            dashboard_panel: DashboardPanel::new(),
            indicator_panel: IndicatorPanel::new(),
            bracket_panel: BracketOrderPanel::new(),
            advanced_orders_panel: AdvancedOrdersPanel::new(),
            rpc_panel: RpcPanel::new(),
            #[cfg(feature = "alpha")]
            alpha_panel: AlphaPanel::new(),
            connect_dialogs: Vec::new(),
            about_dialog: AboutDialog::new(),
            global_settings: GlobalSettingsDialog::new(),
            contract_manager: ContractManagerDialog::new(),
            toast_manager: ToastManager::new(),
            gateway_names: Vec::new(),
            charts: HashMap::new(),
            tick_aggregators: HashMap::new(),
            pending_history_data: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            pending_history_prepend: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            strategy_engine: None,
            strategy_cache: Arc::new(Mutex::new(Vec::new())),
            strategy_update_counter: 0,
            dashboard_log_counter: 0,
            alert_history: Vec::new(),
            last_alert_id: 0,
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
    
    /// Set reference to strategy engine for data sync
    pub fn set_strategy_engine(&mut self, engine: Arc<StrategyEngine>) {
        self.strategy_engine = Some(engine);
    }
    
    /// Update UI data from main engine
    pub fn update_data(&mut self) {
        if let Some(ref engine) = self.main_engine {
            // Check for pending history data (initial load / interval change → jump to right)
            if let Ok(mut pending) = self.pending_history_data.try_lock() {
                for (vt_symbol, bars) in pending.drain() {
                    if let Some(chart) = self.charts.get_mut(&vt_symbol) {
                        tracing::info!("加载历史数据到图表: {} ({} 条)", vt_symbol, bars.len());
                        chart.update_history(bars);
                        chart.set_loading_history(false);
                    }
                }
            }

            // Check for pending prepended history data (drag-to-load → preserve scroll position)
            if let Ok(mut pending) = self.pending_history_prepend.try_lock() {
                for (vt_symbol, bars) in pending.drain() {
                    if let Some(chart) = self.charts.get_mut(&vt_symbol) {
                        tracing::info!("追加历史数据到图表: {} ({} 条)", vt_symbol, bars.len());
                        chart.update_history_prepend(bars);
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
                    // Use the chart's current interval instead of hardcoded Minute
                    let interval = self.charts.get(&vt_symbol)
                        .map(|c| c.get_interval())
                        .unwrap_or(crate::trader::Interval::Minute);
                    let aggregator = self.tick_aggregators
                        .entry(vt_symbol.clone())
                        .or_insert_with(|| TickBarAggregator::new(interval));
                    
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
            
            // Refresh advanced orders panel data
            self.advanced_orders_panel.refresh_data(
                Some(engine.stop_order_engine()),
                Some(engine.order_emulator()),
            );
        }
        
        // Update dashboard panel data (separate borrow scope - clone Arc to avoid borrow conflict)
        if let Some(engine_arc) = self.main_engine.clone() {
            self.update_dashboard_data(&engine_arc);
        }
        
        // Sync alert history from backend ToastManager
        if let Some(ref engine) = self.main_engine {
            let backend_toasts = engine.toast_manager().get_all_toasts();
            
            // Detect new alerts by checking for IDs greater than last_alert_id
            let max_id = backend_toasts.iter().map(|t| t.id).max().unwrap_or(0);
            if max_id > self.last_alert_id {
                // Find new alerts
                for toast in &backend_toasts {
                    if toast.id > self.last_alert_id {
                        // Show toast notification for new alerts
                        let toast_type = match toast.level {
                            AlertLevel::Info => ToastType::Info,
                            AlertLevel::Warning => ToastType::Warning,
                            AlertLevel::Critical => ToastType::Error,
                        };
                        self.toast_manager.add(&format!("{}: {}", toast.title, toast.body), toast_type);
                    }
                }
                self.last_alert_id = max_id;
            }
            
            self.alert_history = backend_toasts;
        }
        
        // Update strategy data periodically (every 30 frames ~ every 3 seconds)
        self.strategy_update_counter += 1;
        if self.strategy_update_counter >= 30 {
            self.strategy_update_counter = 0;
            
            // Spawn background task to read strategy data from async engine
            if let Some(ref strategy_engine) = self.strategy_engine {
                let cache = self.strategy_cache.clone();
                let engine = strategy_engine.clone();
                tokio::spawn(async move {
                    let names = engine.get_all_strategy_names();
                    let mut data = Vec::new();
                    for name in names {
                        if let Some(info) = engine.get_strategy_info(&name) {
                            let state_str = info.get("state").cloned().unwrap_or_default();
                            let state = match state_str.as_str() {
                                "Inited" => StrategyState::Inited,
                                "Trading" => StrategyState::Trading,
                                "Stopped" => StrategyState::Stopped,
                                _ => StrategyState::NotInited,
                            };
                            // Get total PnL (realized + unrealized) for this strategy
                            let total_pnl = engine.get_strategy_total_pnl(&name);
                            data.push((name, state, total_pnl));
                        }
                    }
                    if let Ok(mut cache) = cache.lock() {
                        *cache = data;
                    }
                });
            }
        }
        
        // Apply cached strategy data to UI panels (non-blocking read)
        if let Ok(cache) = self.strategy_cache.lock() {
            use super::dashboard::{StrategySummary, StrategyStateDisplay};
            
            // Update dashboard strategies card
            let strategies: Vec<StrategySummary> = cache.iter().map(|(name, state, total_pnl)| {
                let display_state = match state {
                    StrategyState::Trading => StrategyStateDisplay::Running,
                    StrategyState::Inited => StrategyStateDisplay::Inited,
                    _ => StrategyStateDisplay::Stopped,
                };
                StrategySummary {
                    name: name.clone(),
                    state: display_state,
                    today_pnl: *total_pnl,
                }
            }).collect();
            self.dashboard_panel.update_strategies(strategies);
            
            // Update strategy panel
            let rows: Vec<super::strategy_panel::StrategyRow> = cache.iter().map(|(name, state, _pnl)| {
                let state_str = match state {
                    StrategyState::NotInited => "NotInited",
                    StrategyState::Inited => "Inited",
                    StrategyState::Trading => "Trading",
                    StrategyState::Stopped => "Stopped",
                    StrategyState::Error => "Error",
                };
                super::strategy_panel::StrategyRow {
                    name: name.clone(),
                    state: state_str.to_string(),
                    strategy_type: "CTA".to_string(),
                    symbols: String::new(),
                }
            }).collect();
            self.strategy_panel.update_strategies(rows);
        }
    }
    
    /// Update dashboard panel with current engine data
    fn update_dashboard_data(&mut self, engine: &crate::trader::MainEngine) {
        use super::dashboard::{PositionSummary, GatewayStatus, NotificationItem};
        use chrono::TimeZone;
        
        // Log dashboard data every ~10 frames (~1 second) to avoid log spam
        self.dashboard_log_counter += 1;
        let should_log = self.dashboard_log_counter >= 10;
        if should_log {
            self.dashboard_log_counter = 0;
        }
        
        // Conditional log macro
        macro_rules! dlog {
            ($($arg:tt)*) => {
                if should_log {
                    tracing::info!($($arg)*);
                }
            };
        }
        
        if should_log { tracing::info!("========== Dashboard Data Update =========="); }
        
        // Account summary - process all assets
        let accounts = engine.get_all_accounts();
        dlog!("[Account] 共 {} 个账户", accounts.len());
        
        // Build asset balances list
        use super::dashboard::AssetBalance;
        let mut assets: Vec<AssetBalance> = accounts.iter()
            .map(|a| AssetBalance {
                asset: a.accountid.clone(), // accountid contains the asset name (e.g., "USDT", "BTC")
                balance: a.balance,
                available: a.available(),
                frozen: a.frozen,
            })
            .collect();
        
        // Log all assets
        for asset in &assets {
            dlog!(
                "[Account] 资产: {} balance={:.6}, frozen={:.6}, available={:.6}",
                asset.asset, asset.balance, asset.frozen, asset.available
            );
        }
        
        // Find USDT as primary currency for margin trading display
        let usdt_account = assets.iter().find(|a| a.asset == "USDT");
        let primary_asset = usdt_account.or_else(|| assets.iter().find(|a| a.balance > 0.0));
        
        let (total_balance, available, frozen, currency) = if let Some(primary) = primary_asset {
            (primary.balance, primary.available, primary.frozen, primary.asset.clone())
        } else {
            (0.0, 0.0, 0.0, "USDT".to_string())
        };
        
        dlog!(
            "[Account] 主显示: currency={}, balance={:.6}, available={:.6}, frozen={:.6}",
            currency, total_balance, available, frozen
        );
        
        // Sort assets: primary currency first, then by balance descending
        assets.sort_by(|a, b| {
            if a.asset == currency { return std::cmp::Ordering::Less; }
            if b.asset == currency { return std::cmp::Ordering::Greater; }
            b.balance.partial_cmp(&a.balance).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        self.dashboard_panel.update_account(total_balance, available, frozen, &currency, assets);
        
        // Positions summary
        let all_positions = engine.get_all_positions();
        let positions: Vec<PositionSummary> = all_positions.iter()
            .filter(|p| p.volume > 0.0)
            .map(|p| {
                PositionSummary {
                    vt_symbol: p.vt_symbol(),
                    direction: format!("{}", p.direction),
                    volume: p.volume,
                    avg_price: p.price,
                    pnl: p.pnl,
                    pnl_percent: if p.price > 0.0 { p.pnl / (p.price * p.volume) * 100.0 } else { 0.0 },
                }
            })
            .collect();
        dlog!(
            "[Positions] 总持仓数={}, 有量持仓={}",
            all_positions.len(),
            positions.len()
        );
        for (i, pos) in positions.iter().enumerate() {
            dlog!(
                "[Positions] #{}: symbol={}, dir={}, vol={:.4}, price={:.2}, pnl={:.4}, pnl%={:.2}%",
                i + 1,
                pos.vt_symbol,
                pos.direction,
                pos.volume,
                pos.avg_price,
                pos.pnl,
                pos.pnl_percent
            );
        }
        self.dashboard_panel.update_positions(positions);
        
        // Today's PnL - use position floating PnL for total, trade data for counts
        let today_pnl: f64 = all_positions.iter()
            .map(|p| p.pnl)
            .sum();
        dlog!("[TodayPnL] 持仓浮动盈亏合计={:.4}", today_pnl);

        // Count today's trades for win/loss statistics
        let today_start = NaiveDateTime::new(
            Utc::now().date_naive(),
            NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default(),
        );
        let today_start_utc = Utc.from_utc_datetime(&today_start);
        let all_trades = engine.get_all_trades();
        let today_trades: Vec<_> = all_trades.iter()
            .filter(|t| t.datetime.is_some_and(|dt| dt >= today_start_utc))
            .collect();
        let trade_count = today_trades.len();
        dlog!(
            "[TodayPnL] 总成交数={}, 今日成交数={}",
            all_trades.len(),
            trade_count
        );
        for (i, trade) in today_trades.iter().enumerate() {
            dlog!(
                "[TodayPnL] trade#{}: symbol={}, dir={:?}, vol={:.4}, price={:.2}, time={:?}",
                i + 1,
                trade.vt_symbol(),
                trade.direction,
                trade.volume,
                trade.price,
                trade.datetime
            );
        }

        // Derive win/loss from positions with positive/negative floating PnL
        let pos_win_count = all_positions.iter().filter(|p| p.pnl > 0.0).count();
        let pos_loss_count = all_positions.iter().filter(|p| p.pnl < 0.0).count();
        let win_amount: f64 = all_positions.iter().filter(|p| p.pnl > 0.0).map(|p| p.pnl).sum();
        let loss_amount: f64 = all_positions.iter().filter(|p| p.pnl < 0.0).map(|p| p.pnl.abs()).sum();
        dlog!(
            "[TodayPnL] 持仓盈: {}笔 共{:.4}, 亏: {}笔 共{:.4}",
            pos_win_count, win_amount, pos_loss_count, loss_amount
        );

        // Use trade count to override if we have actual trades today
        let (win_count, loss_count) = if trade_count > 0 {
            // Split trades by direction as a proxy: sells = realized gains, buys = realized costs
            let sells = today_trades.iter()
                .filter(|t| t.direction == Some(Direction::Short))
                .count();
            let buys = today_trades.iter()
                .filter(|t| t.direction == Some(Direction::Long))
                .count();
            dlog!("[TodayPnL] 今日买入={}, 卖出={}", buys, sells);
            // If we have a net positive PnL, attribute more wins to sells; otherwise to buys
            if today_pnl >= 0.0 {
                (sells.max(1), buys)
            } else {
                (buys.max(1), sells)
            }
        } else {
            (pos_win_count, pos_loss_count)
        };
        dlog!(
            "[TodayPnL] 最终: pnl={:.4}, win_count={}, loss_count={}, win_amount={:.4}, loss_amount={:.4}",
            today_pnl, win_count, loss_count, win_amount, loss_amount
        );

        self.dashboard_panel.update_today_pnl(
            today_pnl,
            win_count,
            loss_count,
            win_amount,
            loss_amount,
        );

        // PnL curve from trade history
        use super::dashboard::PnlPoint;
        use crate::trader::constant::Direction;
        use chrono::{NaiveDateTime, NaiveTime};

        let mut sorted_trades: Vec<_> = all_trades.iter()
            .filter(|t| t.datetime.is_some())
            .collect();
        sorted_trades.sort_by_key(|t| t.datetime);

        let mut cumulative = 0.0_f64;
        let mut curve: Vec<PnlPoint> = Vec::new();

        for trade in &sorted_trades {
            // Approximate PnL contribution from trade direction and volume
            let trade_value = trade.volume * trade.price;
            let contribution = match trade.direction {
                Some(Direction::Short) => trade_value,   // Selling = revenue
                Some(Direction::Long) => -trade_value,   // Buying = cost
                _ => 0.0,
            };
            cumulative += contribution;

            if let Some(dt) = trade.datetime {
                let time_minutes = dt.timestamp() / 60;
                curve.push(PnlPoint {
                    time: time_minutes,
                    cumulative_pnl: cumulative,
                });
            }
        }
        dlog!("[PnLCurve] 累计曲线点数={}, 最后cumulative={:.4}", curve.len(), cumulative);

        // Append current floating PnL as the latest point
        if !curve.is_empty() || today_pnl != 0.0 {
            let now_minutes = Utc::now().timestamp() / 60;
            curve.push(PnlPoint {
                time: now_minutes,
                cumulative_pnl: today_pnl,
            });
        }

        self.dashboard_panel.update_pnl_curve(curve);
        
        // System status (gateways)
        let gateway_names = self.gateway_names.clone();
        let gateway_status: Vec<GatewayStatus> = gateway_names.iter()
            .map(|name| {
                let connected = engine.get_gateway(name).is_some();
                dlog!("[Gateway] {} connected={}", name, connected);
                GatewayStatus {
                    name: name.clone(),
                    connected,
                    reconnecting: false,
                    latency_ms: 0,
                    reconnect_attempts: 0,
                }
            })
            .collect();
        
        // Recent notifications from logs
        let all_logs = engine.get_all_logs();
        dlog!("[System] 总日志数={}, 网关数={}", all_logs.len(), gateway_status.len());
        let notifications: Vec<NotificationItem> = all_logs
            .iter()
            .rev()
            .take(5)
            .map(|log| {
                // Map log level to notification level
                // LogData.level uses: DEBUG=10, INFO=20, WARNING=30, ERROR=40, CRITICAL=50
                let level = match log.level {
                    l if l >= 40 => super::dashboard::NotificationLevel::Error,
                    l if l >= 30 => super::dashboard::NotificationLevel::Warning,
                    _ => super::dashboard::NotificationLevel::Info,
                };
                NotificationItem {
                    time: log.time.format("%H:%M:%S").to_string(),
                    message: log.msg.clone(),
                    level,
                }
            })
            .collect();
        
        self.dashboard_panel.update_system_status(gateway_status, notifications);
        if should_log { tracing::info!("========== Dashboard Data Update END =========="); }
    }
    
    /// Set available gateways
    pub fn set_gateways(&mut self, gateways: Vec<String>) {
        self.gateway_names = gateways.clone();
        self.trading.set_gateways(gateways.clone());
        self.advanced_orders_panel.set_gateways(&gateways);
        self.bracket_panel.set_gateways(gateways.clone());
        
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
    
    /// Apply theme based on dark_mode setting
    pub fn setup_style(&self, ctx: &Context) {
        if self.dark_mode {
            apply_dark_theme(ctx);
        } else {
            apply_light_theme(ctx);
        }
    }
    
    /// Handle keyboard shortcuts
    fn handle_keyboard_shortcuts(&mut self, ctx: &Context) {
        let ctrl = ctx.input(|i| i.modifiers.ctrl);
        
        // Ctrl+1 through Ctrl+7: Switch central tabs
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
            self.central_tab = CentralTab::Tick;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
            self.central_tab = CentralTab::Order;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
            self.central_tab = CentralTab::ActiveOrder;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
            self.central_tab = CentralTab::Trade;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num5)) {
            self.central_tab = CentralTab::Quote;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num6)) {
            self.central_tab = CentralTab::Backtesting;
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num7)) {
            self.central_tab = CentralTab::Strategy;
        }
        // Ctrl+8: Indicator tab
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num8)) {
            self.central_tab = CentralTab::Indicator;
        }
        // Ctrl+0: Dashboard tab
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num0)) {
            self.central_tab = CentralTab::Dashboard;
        }
        // Ctrl+9: Alpha Research tab (only if alpha feature is enabled)
        #[cfg(feature = "alpha")]
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num9)) {
            self.central_tab = CentralTab::AlphaResearch;
        }
        
        // Ctrl+L: Log tab
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::L)) {
            self.bottom_tab = BottomTab::Log;
        }
        // Ctrl+B: Account tab
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::B)) {
            self.bottom_tab = BottomTab::Account;
        }
        // Ctrl+P: Position tab
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::P)) {
            self.bottom_tab = BottomTab::Position;
        }
        // Ctrl+N: Focus symbol input
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::N)) {
            self.focus_symbol_input = true;
        }
        // Escape: Close dialogs
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.connect_dialogs.iter_mut().for_each(|d| d.close());
            self.about_dialog.close();
            self.global_settings.close();
            self.contract_manager.close();
        }
    }
    
    /// Show the main window UI
    pub fn show(&mut self, ctx: &Context) {
        // Apply theme every frame
        self.setup_style(ctx);
        
        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ctx);
        
        // Top menu bar
        self.show_menu_bar(ctx);
        
        // Left panel - trading widget (with toast feedback)
        self.show_trading_panel(ctx);
        
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
        
        // Show toast notifications (after all other UI)
        self.toast_manager.show(ctx);
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
                    
                    ui.separator();
                    
                    ui.checkbox(&mut self.dark_mode, "深色主题");
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
                    
                    ui.menu_button("快捷键", |ui| {
                        ui.label(RichText::new("中心标签页").strong());
                        ui.label("Ctrl+1  行情");
                        ui.label("Ctrl+2  委托");
                        ui.label("Ctrl+3  活动");
                        ui.label("Ctrl+4  成交");
                        ui.label("Ctrl+5  报价");
                        ui.label("Ctrl+6  回测");
                        ui.label("Ctrl+7  策略");
                        ui.label("Ctrl+8  指标");
                        ui.label("Ctrl+9  量化研究");
                        ui.label("Ctrl+0  仪表盘");

                        ui.separator();

                        ui.label(RichText::new("底部标签页").strong());
                        ui.label("Ctrl+L  日志");
                        ui.label("Ctrl+B  资金");
                        ui.label("Ctrl+P  持仓");
                        
                        ui.separator();
                        
                        ui.label(RichText::new("其他").strong());
                        ui.label("Ctrl+N  聚焦代码输入");
                        ui.label("Esc     关闭对话框");
                    });
                    
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
            ui.selectable_value(&mut self.central_tab, CentralTab::Dashboard, "仪表盘");
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
            ui.selectable_value(&mut self.central_tab, CentralTab::Strategy, "策略");
            ui.selectable_value(&mut self.central_tab, CentralTab::Backtesting, "回测");
            ui.selectable_value(&mut self.central_tab, CentralTab::Indicator, "指标");
            ui.selectable_value(&mut self.central_tab, CentralTab::Alert, "告警");
            ui.selectable_value(&mut self.central_tab, CentralTab::AdvancedOrders, "高级委托");
            ui.selectable_value(&mut self.central_tab, CentralTab::BracketOrder, "组合单");
            ui.selectable_value(&mut self.central_tab, CentralTab::RpcMonitor, "远程监控");
            #[cfg(feature = "alpha")]
            ui.selectable_value(&mut self.central_tab, CentralTab::AlphaResearch, "量化研究");
        });
        
        ui.separator();
        
        // Tab content
        match self.central_tab {
            CentralTab::Dashboard => {
                let action = self.dashboard_panel.show(ui);
                self.handle_dashboard_action(action);
            }
            CentralTab::Tick => {
                if let Some(vt_symbol) = self.tick_monitor.show(ui) {
                    // Open chart window for the clicked symbol
                    self.open_chart(&vt_symbol);
                }
            }
            CentralTab::Order => {
                if let Some(vt_orderid) = self.order_monitor.show(ui) {
                    self.pending_cancel_order = Some(vt_orderid);
                    self.toast_manager.add("撤单已提交", ToastType::Info);
                }
            }
            CentralTab::ActiveOrder => {
                if let Some(vt_orderid) = self.active_order_monitor.show(ui) {
                    self.pending_cancel_order = Some(vt_orderid);
                    self.toast_manager.add("撤单已提交", ToastType::Info);
                }
            }
            CentralTab::Trade => {
                self.trade_monitor.show(ui);
            }
            CentralTab::Quote => {
                if let Some(vt_quoteid) = self.quote_monitor.show(ui) {
                    self.pending_cancel_quote = Some(vt_quoteid);
                    self.toast_manager.add("撤销报价已提交", ToastType::Info);
                }
            }
            CentralTab::Strategy => {
                self.strategy_panel.show(ui);
            }
            CentralTab::Backtesting => {
                let ctx = ui.ctx().clone();
                self.backtesting_panel.ui(&ctx, ui);
                // Sync trade overlay to matching chart after backtest
                self.sync_backtest_trade_overlay();
            }
            CentralTab::Indicator => {
                self.indicator_panel.show(ui);
                // Apply indicator changes to charts
                if let Some(configs) = self.indicator_panel.take_apply() {
                    self.apply_indicators_to_charts(&configs);
                }
                // Apply Python indicator changes to charts
                if let Some(python_configs) = self.indicator_panel.take_apply_python() {
                    self.apply_python_indicators_to_charts(&python_configs);
                }
            }
            CentralTab::BracketOrder => {
                let bracket_engine = self.main_engine.as_ref().map(|e| e.bracket_order_engine().clone());
                self.bracket_panel.show(ui, bracket_engine.as_ref(), &mut self.toast_manager);
                // Check for pending cancel request
                if let Some(group_id) = self.bracket_panel.take_cancel() {
                    if let Some(ref engine) = self.main_engine {
                        let boe = engine.bracket_order_engine().clone();
                        tokio::spawn(async move {
                            if let Err(e) = boe.cancel_group(group_id) {
                                tracing::warn!("撤销委托组失败: {}", e);
                            }
                        });
                    }
                }
            }
            CentralTab::Alert => {
                self.show_alert_panel(ui);
            }
            CentralTab::AdvancedOrders => {
                self.advanced_orders_panel.show(ui, &mut self.toast_manager);
            }
            CentralTab::RpcMonitor => {
                // Update RPC panel state from environment
                let rpc_port: u16 = std::env::var("VNRS_RPC_PORT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5555);
                self.rpc_panel.set_rpc_port(rpc_port);
                self.rpc_panel.show(ui);
            }
            #[cfg(feature = "alpha")]
            CentralTab::AlphaResearch => {
                self.alpha_panel.show(ui);
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
    
    /// Show the alert history panel
    fn show_alert_panel(&mut self, ui: &mut Ui) {
        // Toolbar
        ui.horizontal(|ui| {
            ui.label(RichText::new("告警历史").strong());
            ui.add_space(8.0);
            ui.label(RichText::new(format!("共 {} 条", self.alert_history.len())).color(COLOR_TEXT_SECONDARY));
            ui.add_space(8.0);
            if ui.button("清除全部").clicked() {
                if let Some(ref engine) = self.main_engine {
                    engine.toast_manager().clear();
                    self.alert_history.clear();
                    self.last_alert_id = 0;
                }
            }
            if ui.button("全部已读").clicked() {
                if let Some(ref engine) = self.main_engine {
                    engine.toast_manager().dismiss_all();
                }
            }
        });
        
        ui.separator();
        
        // Table header
        ui.horizontal(|ui| {
            ui.label(RichText::new("时间").color(COLOR_TEXT_SECONDARY).size(12.0));
            ui.add_space(18.0);
            ui.label(RichText::new("级别").color(COLOR_TEXT_SECONDARY).size(12.0));
            ui.add_space(18.0);
            ui.label(RichText::new("标题").color(COLOR_TEXT_SECONDARY).size(12.0));
            ui.add_space(18.0);
            ui.label(RichText::new("内容").color(COLOR_TEXT_SECONDARY).size(12.0));
            ui.add_space(18.0);
            ui.label(RichText::new("来源").color(COLOR_TEXT_SECONDARY).size(12.0));
        });
        
        ui.separator();
        
        // Scrollable alert rows (most recent first)
        egui::ScrollArea::vertical().show(ui, |ui| {
            let alerts: Vec<_> = self.alert_history.iter().rev().collect();
            for toast in &alerts {
                let row_height = TABLE_ROW_HEIGHT;
                
                // Determine row color based on alert level
                let level_color = match toast.level {
                    AlertLevel::Info => Color32::from_rgb(80, 150, 255),
                    AlertLevel::Warning => Color32::from_rgb(255, 200, 50),
                    AlertLevel::Critical => Color32::from_rgb(255, 80, 80),
                };
                
                let level_text = match toast.level {
                    AlertLevel::Info => "信息",
                    AlertLevel::Warning => "警告",
                    AlertLevel::Critical => "严重",
                };
                
                // Row background - subtle highlight for unread
                let bg_color = if !toast.dismissed {
                    Color32::from_rgba_unmultiplied(level_color.r(), level_color.g(), level_color.b(), 15)
                } else {
                    Color32::TRANSPARENT
                };
                
                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::click(),
                );
                
                // Draw row background
                if bg_color != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, 0.0, bg_color);
                }
                
                // Left border accent for unread
                if !toast.dismissed {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, egui::vec2(3.0, row_height)),
                        0.0,
                        level_color,
                    );
                }
                
                let mut x = rect.left() + 5.0;
                let y = rect.center().y;
                
                // Time column
                let time_str = toast.timestamp.format("%H:%M:%S").to_string();
                ui.painter().text(
                    egui::Pos2::new(x, y),
                    egui::Align2::LEFT_CENTER,
                    &time_str,
                    egui::FontId::proportional(11.0),
                    COLOR_TEXT_SECONDARY,
                );
                x += 70.0;
                
                // Level column (color-coded)
                ui.painter().text(
                    egui::Pos2::new(x, y),
                    egui::Align2::LEFT_CENTER,
                    level_text,
                    egui::FontId::proportional(11.0),
                    level_color,
                );
                x += 60.0;
                
                // Title column
                ui.painter().text(
                    egui::Pos2::new(x, y),
                    egui::Align2::LEFT_CENTER,
                    &toast.title,
                    egui::FontId::proportional(11.0),
                    COLOR_TEXT_PRIMARY,
                );
                x += 100.0;
                
                // Body column
                let body_display = if toast.body.len() > 80 {
                    format!("{}...", &toast.body[..80])
                } else {
                    toast.body.clone()
                };
                ui.painter().text(
                    egui::Pos2::new(x, y),
                    egui::Align2::LEFT_CENTER,
                    &body_display,
                    egui::FontId::proportional(11.0),
                    COLOR_TEXT_SECONDARY,
                );
                x += rect.width() - 265.0;
                
                // Source column
                ui.painter().text(
                    egui::Pos2::new(x, y),
                    egui::Align2::LEFT_CENTER,
                    &toast.source,
                    egui::FontId::proportional(11.0),
                    COLOR_TEXT_SECONDARY,
                );
            }
        });
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
            if let Some(ref engine) = self.main_engine {
                let active_orders = engine.get_all_active_orders();
                let count = active_orders.len();
                for order in active_orders {
                    if let Some(gw_name) = engine.find_gateway_name_for_exchange(order.exchange) {
                        let req = crate::trader::object::CancelRequest {
                            orderid: order.orderid.clone(),
                            symbol: order.symbol.clone(),
                            exchange: order.exchange,
                            gateway_name: String::new(),
                        };
                        let engine = engine.clone();
                        let gw = gw_name.clone();
                        tokio::spawn(async move {
                            if let Err(e) = engine.cancel_order(req, &gw).await {
                                tracing::warn!("撤单失败: {}", e);
                            }
                        });
                    }
                }
                self.toast_manager.add(&format!("撤销 {} 个委托", count), ToastType::Info);
            }
        }

        // Advanced orders panel: stop order submit
        if let Some(req) = self.advanced_orders_panel.take_stop_request() {
            if let Some(ref engine) = self.main_engine {
                let se = engine.stop_order_engine().clone();
                tokio::spawn(async move {
                    match se.add_stop_order(req) {
                        Ok(id) => tracing::info!("止损单已提交: id={}", id),
                        Err(e) => tracing::warn!("止损单提交失败: {}", e),
                    }
                });
            } else {
                self.toast_manager.add("主引擎未就绪", ToastType::Error);
            }
        }

        // Advanced orders panel: stop order cancel
        if let Some(id) = self.advanced_orders_panel.take_cancel_stop() {
            if let Some(ref engine) = self.main_engine {
                let se = engine.stop_order_engine().clone();
                tokio::spawn(async move {
                    match se.cancel_stop_order(id) {
                        Ok(()) => tracing::info!("止损单已撤销: id={}", id),
                        Err(e) => tracing::warn!("止损单撤销失败: {}", e),
                    }
                });
            }
        }

        // Trading widget: stop order submit (from TradingWidget stop/stop-limit order types)
        if let Some(req) = self.trading.take_stop_order() {
            if let Some(ref engine) = self.main_engine {
                let se = engine.stop_order_engine().clone();
                tokio::spawn(async move {
                    match se.add_stop_order(req) {
                        Ok(id) => tracing::info!("TradingWidget 止损单已提交: id={}", id),
                        Err(e) => tracing::warn!("TradingWidget 止损单提交失败: {}", e),
                    }
                });
            } else {
                self.toast_manager.add("主引擎未就绪", ToastType::Error);
            }
        }

        // Advanced orders panel: emulated order submit
        if let Some(req) = self.advanced_orders_panel.take_emul_request() {
            if let Some(ref engine) = self.main_engine {
                let oe = engine.order_emulator().clone();
                tokio::spawn(async move {
                    match oe.add_order(&req) {
                        Ok(id) => tracing::info!("模拟委托已提交: id={}", id),
                        Err(e) => tracing::warn!("模拟委托提交失败: {}", e),
                    }
                });
            } else {
                self.toast_manager.add("主引擎未就绪", ToastType::Error);
            }
        }

        // Advanced orders panel: emulated order cancel
        if let Some(id) = self.advanced_orders_panel.take_cancel_emul() {
            if let Some(ref engine) = self.main_engine {
                let oe = engine.order_emulator().clone();
                tokio::spawn(async move {
                    match oe.cancel_order(id) {
                        Ok(()) => tracing::info!("模拟委托已撤销: id={}", id),
                        Err(e) => tracing::warn!("模拟委托撤销失败: {}", e),
                    }
                });
            }
        }

        // Strategy panel pending actions
        if let Some(name) = self.strategy_panel.take_init() {
            if let Some(ref se) = self.strategy_engine {
                let se = se.clone();
                let n = name.clone();
                tokio::spawn(async move {
                    match se.init_strategy(&n).await {
                        Ok(_) => tracing::info!("策略 {} 初始化成功", n),
                        Err(e) => tracing::error!("策略 {} 初始化失败: {}", n, e),
                    }
                });
            }
        }
        if let Some(name) = self.strategy_panel.take_start() {
            if let Some(ref se) = self.strategy_engine {
                let se = se.clone();
                let n = name.clone();
                tokio::spawn(async move {
                    match se.start_strategy(&n) {
                        Ok(_) => tracing::info!("策略 {} 启动成功", n),
                        Err(e) => tracing::error!("策略 {} 启动失败: {}", n, e),
                    }
                });
            }
        }
        if let Some(name) = self.strategy_panel.take_stop() {
            if let Some(ref se) = self.strategy_engine {
                let se = se.clone();
                let n = name.clone();
                tokio::spawn(async move {
                    match se.stop_strategy(&n).await {
                        Ok(_) => tracing::info!("策略 {} 停止成功", n),
                        Err(e) => tracing::error!("策略 {} 停止失败: {}", n, e),
                    }
                });
            }
        }
        if let Some(name) = self.strategy_panel.take_remove() {
            if let Some(ref se) = self.strategy_engine {
                let se = se.clone();
                let n = name.clone();
                // Clear selection since we're removing it
                self.strategy_panel.clear_selection();
                tokio::spawn(async move {
                    match se.remove_strategy(&n).await {
                        Ok(_) => tracing::info!("策略 {} 移除成功", n),
                        Err(e) => tracing::error!("策略 {} 移除失败: {}", n, e),
                    }
                });
            }
        }

        // Poll Python strategy indicator registrations
        #[cfg(feature = "python")]
        {
            if let Some(ref se) = self.strategy_engine {
                let names = se.get_all_strategy_names();
                for name in names {
                    let registrations = se.get_pending_indicator_registrations(&name);
                    for reg in registrations {
                        use super::indicator_panel::{IndicatorCategory, PythonIndicatorEntry};
                        use crate::chart::IndicatorLocation;

                        let category = match reg.category.as_str() {
                            "trend" => IndicatorCategory::Trend,
                            "volatility" => IndicatorCategory::Volatility,
                            "volume" => IndicatorCategory::Volume,
                            "oscillator" => IndicatorCategory::Oscillator,
                            "trend_following" => IndicatorCategory::TrendFollowing,
                            "momentum" => IndicatorCategory::Momentum,
                            _ => IndicatorCategory::Oscillator,
                        };

                        let location = match reg.location.as_str() {
                            "main" => IndicatorLocation::Main,
                            _ => IndicatorLocation::Sub,
                        };

                        let entry = PythonIndicatorEntry {
                            id: reg.id,
                            name: reg.name,
                            category,
                            params_desc: reg.params_desc,
                            enabled: true,
                            color: egui::Color32::from_rgb(0, 200, 255),
                            location,
                            last_values: Vec::new(),
                        };

                        self.indicator_panel.add_python_indicator(entry);
                    }
                }
            }
        }

        // Poll Python strategy indicator values and forward to indicator panel + charts
        #[cfg(feature = "python")]
        {
            if let Some(ref se) = self.strategy_engine {
                let names = se.get_all_strategy_names();
                for name in names {
                    let values = se.drain_pending_indicator_values(&name);
                    if values.is_empty() {
                        continue;
                    }
                    for iv in &values {
                        // PythonIndicatorEntry.id = "{strategy_name}_{indicator_name}"
                        let indicator_id = format!("{}_{}", name, iv.name);
                        // Update indicator panel with latest values
                        self.indicator_panel.update_python_indicator_values(
                            &indicator_id,
                            vec![(iv.name.clone(), iv.value)],
                        );
                        // Update chart overlays
                        for chart in self.charts.values_mut() {
                            chart.update_indicator_raw(&iv.name, iv.value);
                        }
                    }
                }
            }
        }
    }
    
    /// Handle dashboard card click actions
    fn handle_dashboard_action(&mut self, action: DashboardAction) {
        match action {
            DashboardAction::None => {}
            DashboardAction::NavigateToAccount => {
                self.bottom_tab = BottomTab::Account;
            }
            DashboardAction::NavigateToPositions => {
                self.bottom_tab = BottomTab::Position;
            }
            DashboardAction::NavigateToTrades => {
                self.central_tab = CentralTab::Trade;
            }
            DashboardAction::NavigateToStrategies => {
                self.central_tab = CentralTab::Strategy;
            }
            DashboardAction::NavigateToNotifications => {
                self.bottom_tab = BottomTab::Log;
            }
            DashboardAction::StartAllStrategies => {
                if let Some(ref se) = self.strategy_engine {
                    if let Ok(cache) = self.strategy_cache.lock() {
                        let names: Vec<String> = cache.iter()
                            .filter(|(_, state, _)| *state == StrategyState::Inited)
                            .map(|(name, _, _)| name.clone())
                            .collect();
                        if names.is_empty() {
                            self.toast_manager.add("没有可启动的策略（需要先初始化）", ToastType::Info);
                        } else {
                            for name in &names {
                                let se = se.clone();
                                let n = name.clone();
                                tokio::spawn(async move {
                                    match se.start_strategy(&n) {
                                        Ok(_) => tracing::info!("策略 {} 启动成功", n),
                                        Err(e) => tracing::error!("策略 {} 启动失败: {}", n, e),
                                    }
                                });
                            }
                            self.toast_manager.add(
                                &format!("启动 {} 个策略", names.len()),
                                ToastType::Info,
                            );
                        }
                    }
                } else {
                    self.toast_manager.add("策略引擎未加载", ToastType::Info);
                }
            }
            DashboardAction::StopAllStrategies => {
                if let Some(ref se) = self.strategy_engine {
                    if let Ok(cache) = self.strategy_cache.lock() {
                        let names: Vec<String> = cache.iter()
                            .filter(|(_, state, _)| *state == StrategyState::Trading)
                            .map(|(name, _, _)| name.clone())
                            .collect();
                        if names.is_empty() {
                            self.toast_manager.add("没有运行中的策略", ToastType::Info);
                        } else {
                            for name in &names {
                                let se = se.clone();
                                let n = name.clone();
                                tokio::spawn(async move {
                                    match se.stop_strategy(&n).await {
                                        Ok(_) => tracing::info!("策略 {} 停止成功", n),
                                        Err(e) => tracing::error!("策略 {} 停止失败: {}", n, e),
                                    }
                                });
                            }
                            self.toast_manager.add(
                                &format!("停止 {} 个策略", names.len()),
                                ToastType::Info,
                            );
                        }
                    }
                } else {
                    self.toast_manager.add("策略引擎未加载", ToastType::Info);
                }
            }
            DashboardAction::OpenChart(vt_symbol) => {
                self.open_chart(&vt_symbol);
            }
        }
    }
    
    /// Show the trading panel and capture order submissions for toast feedback
    fn show_trading_panel(&mut self, ctx: &Context) {
        // Pass focus request to trading widget
        if self.focus_symbol_input {
            self.trading.focus_symbol_input = true;
            self.focus_symbol_input = false;
        }
        
        if self.panels.show_trading {
            SidePanel::left("trading_panel")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    self.trading.show(ui);
                });
        }
        
        // Check if an order was submitted from the trading widget
        if self.trading.pending_order.is_some() {
            self.toast_manager.add("委托已提交", ToastType::Success);
        }
        
        // Check for order submission errors
        if let Some(ref err) = self.trading.last_order_error {
            self.toast_manager.add(err, ToastType::Error);
            self.trading.last_order_error = None;
        }
    }
    
    /// Calculate appropriate history query duration based on interval
    /// More granular intervals need shorter windows, coarser intervals need longer windows
    fn history_duration_for_interval(interval: crate::trader::Interval) -> Duration {
        match interval {
            crate::trader::Interval::Second => Duration::hours(4),
            crate::trader::Interval::Minute => Duration::days(3),
            crate::trader::Interval::Minute5 => Duration::days(7),
            crate::trader::Interval::Minute15 => Duration::days(14),
            crate::trader::Interval::Minute30 => Duration::days(21),
            crate::trader::Interval::Hour => Duration::days(30),
            crate::trader::Interval::Hour4 => Duration::days(90),
            crate::trader::Interval::Daily => Duration::days(365),
            crate::trader::Interval::Weekly => Duration::days(730),
            crate::trader::Interval::Tick => Duration::hours(1),
        }
    }
    
    /// Map an Exchange enum to the appropriate gateway name
    fn gateway_for_exchange(exchange: crate::trader::Exchange, gateway_names: &[String]) -> String {
        match exchange {
            crate::trader::Exchange::Binance => {
                if gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                    "BINANCE_SPOT".to_string()
                } else {
                    String::new()
                }
            }
            crate::trader::Exchange::BinanceUsdm => {
                if gateway_names.contains(&"BINANCE_USDT".to_string()) {
                    "BINANCE_USDT".to_string()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
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
            
            // Find the appropriate gateway based on exchange string
            let gateway_name = match exchange_str {
                "BINANCE" | "Binance" => {
                    if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                        Some("BINANCE_SPOT")
                    } else {
                        None
                    }
                }
                "BINANCE_USDM" | "BinanceUsdm" => {
                    if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                        Some("BINANCE_USDT")
                    } else {
                        None
                    }
                }
                _ => {
                    // Fallback: try to find any Binance gateway
                    if self.gateway_names.contains(&"BINANCE_SPOT".to_string()) {
                        Some("BINANCE_SPOT")
                    } else if self.gateway_names.contains(&"BINANCE_USDT".to_string()) {
                        Some("BINANCE_USDT")
                    } else {
                        None
                    }
                }
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
                let gateway_name = Self::gateway_for_exchange(exchange, &self.gateway_names);
                
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
                            let mut data = pending_data.lock().await;
                            data.insert(vt_sym, Vec::new());
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

                let gateway_name = Self::gateway_for_exchange(exchange, &self.gateway_names);

                let engine_clone = engine.clone();
                let pending_data = self.pending_history_prepend.clone();
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
                            // Insert empty vec to reset loading_history flag
                            let mut data = pending_data.lock().await;
                            data.insert(vt_sym, Vec::new());
                        }
                    }
                });
            }
        }
    }
    
    /// Sync trade overlay from backtesting panel to the matching chart widget.
    ///
    /// When a backtest completes and produces trades, the BacktestingPanel
    /// populates its `trade_overlay`. This method transfers that overlay to
    /// the chart widget for the same vt_symbol so trades are visible on the
    /// K-line chart.
    fn sync_backtest_trade_overlay(&mut self) {
        let overlay = self.backtesting_panel.take_trade_overlay();
        if overlay.markers.is_empty() && overlay.pairs.is_empty() {
            return;
        }
        // Find the chart matching the backtest symbol
        let vt_symbol = self.backtesting_panel.get_vt_symbol().to_string();
        if let Some(chart) = self.charts.get_mut(&vt_symbol) {
            chart.trade_overlay = overlay;
        }
    }
    
    /// Apply indicator configurations to all open chart windows.
    ///
    /// If `configs` is empty, all indicators are cleared from charts.
    /// Otherwise, the current indicators are replaced with the ones specified
    /// in the config entries.
    fn apply_indicators_to_charts(&mut self, configs: &[super::indicator_panel::IndicatorConfigEntry]) {
        use crate::chart::*;
        
        for chart in self.charts.values_mut() {
            chart.clear_indicators();
            
            for config in configs {
                if !config.enabled {
                    continue;
                }
                
                let indicator: Box<dyn Indicator> = match config.indicator_type {
                    IndicatorType::MA => {
                        Box::new(MA::new(config.period, config.color, config.location))
                    }
                    IndicatorType::EMA => {
                        Box::new(EMA::new(config.period, config.color, config.location))
                    }
                    IndicatorType::WMA => {
                        Box::new(WMA::new(config.period, config.color, config.location))
                    }
                    IndicatorType::BOLL => {
                        Box::new(BOLL::new(config.period, config.multiplier, config.location))
                    }
                    IndicatorType::VWAP => {
                        Box::new(VWAP::new(config.color, config.location))
                    }
                    IndicatorType::AVL => {
                        Box::new(AVL::new(config.color, config.location))
                    }
                    IndicatorType::TRIX => {
                        Box::new(TRIX::new(
                            config.period,
                            config.signal_period,
                            config.color,
                            config.signal_color,
                            config.location,
                        ))
                    }
                    IndicatorType::SAR => {
                        Box::new(SAR::new(
                            config.multiplier,
                            0.2,
                            config.color,
                            config.location,
                        ))
                    }
                    IndicatorType::SUPER => {
                        Box::new(SUPER::new(config.period, config.multiplier, Color32::GREEN, Color32::RED, config.location))
                    }
                    IndicatorType::RSI => {
                        Box::new(RSI::new(config.period, config.color, config.location))
                    }
                    IndicatorType::MACD => {
                        Box::new(MACD::new(
                            config.fast_period,
                            config.slow_period,
                            config.signal_period,
                            config.color,
                            config.signal_color,
                            config.hist_color,
                            config.location,
                        ))
                    }
                    IndicatorType::ATR => {
                        Box::new(ATR::new(config.period, config.color, config.location))
                    }
                    IndicatorType::KDJ => {
                        Box::new(KDJ::new(
                            config.period,
                            config.signal_period,
                            config.color,
                            config.signal_color,
                            config.hist_color,
                            config.location,
                        ))
                    }
                    IndicatorType::CCI => {
                        Box::new(CCI::new(config.period, config.color, config.location))
                    }
                    IndicatorType::MFI => {
                        Box::new(MFI::new(config.period, config.color, config.location))
                    }
                };
                
                chart.add_indicator(indicator);
            }
        }
    }

    /// Apply Python indicator configurations to all charts
    fn apply_python_indicators_to_charts(&mut self, configs: &[super::indicator_panel::PythonIndicatorEntry]) {
        use crate::chart::CustomIndicator;

        for chart in self.charts.values_mut() {
            for config in configs {
                if !config.enabled {
                    continue;
                }

                // Create a CustomIndicator from the Python indicator entry.
                // Python indicators compute externally via on_indicator — use a placeholder
                // expression that parses but yields 0.0 (since "nan" isn't a valid variable).
                // Values are then pushed via update_raw(), bypassing expression evaluation.
                let expression = "(close - close)".to_string();

                let mut indicator = CustomIndicator::new(
                    config.name.clone(),
                    expression,
                    config.color,
                    config.location,
                );
                // Mark as externally-computed so update()/calculate() are no-ops.
                // Values come exclusively from update_raw() via on_indicator.
                indicator.set_externally_computed(true);
                chart.add_indicator(Box::new(indicator));
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
            // Find the appropriate gateway for this exchange using MainEngine's method
            let gateway_name = self.main_engine
                .as_ref()
                .and_then(|me| me.find_gateway_name_for_exchange(req.exchange))
                .or_else(|| {
                    // Fallback: try to find any connected gateway
                    self.gateway_names.first().cloned()
                });
            
            if let Some(gw_name) = gateway_name {
                return Some((req, gw_name));
            } else {
                tracing::warn!("No gateway found for exchange {:?}, available gateways: {:?}", 
                    req.exchange, self.gateway_names);
            }
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
                    "strategy" => self.central_tab = CentralTab::Strategy,
                    "alert" => self.central_tab = CentralTab::Alert,
                    "advanced_orders" => self.central_tab = CentralTab::AdvancedOrders,
                    #[cfg(feature = "alpha")]
                    "alpha_research" | "alpha" => self.central_tab = CentralTab::AlphaResearch,
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
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: String::new(),
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
        "RSI" => Box::new(RSI::new(period, Color32::from_rgb(200, 200, 0), sub_loc)),
        "MACD" => Box::new(MACD::new(12, 26, 9, Color32::from_rgb(100, 200, 255), Color32::from_rgb(255, 100, 0), Color32::from_rgb(100, 200, 100), sub_loc)),
        "ATR" => Box::new(ATR::new(period, Color32::from_rgb(200, 100, 100), sub_loc)),
        "KDJ" => Box::new(KDJ::new(period, 3, Color32::from_rgb(255, 255, 0), Color32::from_rgb(0, 200, 255), Color32::from_rgb(255, 100, 200), sub_loc)),
        "CCI" => Box::new(CCI::new(period, Color32::from_rgb(200, 150, 255), sub_loc)),
        "MFI" => Box::new(MFI::new(period, Color32::from_rgb(100, 255, 200), sub_loc)),
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
            crate::trader::Interval::Minute5 => {
                let minute = dt.minute();
                let rounded_minute = (minute / 5) * 5;
                dt.with_minute(rounded_minute).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Minute15 => {
                let minute = dt.minute();
                let rounded_minute = (minute / 15) * 15;
                dt.with_minute(rounded_minute).unwrap_or(*dt)
                    .with_second(0).unwrap_or(*dt)
                    .with_nanosecond(0).unwrap_or(*dt)
            }
            crate::trader::Interval::Minute30 => {
                let minute = dt.minute();
                let rounded_minute = (minute / 30) * 30;
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
