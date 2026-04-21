//! Dashboard panel for the trading platform.
//!
//! Provides a comprehensive overview of account status, positions,
//! strategies, and system health.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

use super::style::*;

// ============================================================================
// Dashboard Panel
// ============================================================================

/// Dashboard panel showing overview cards
pub struct DashboardPanel {
    // Cached data for display
    account_summary: AccountSummary,
    today_pnl: TodayPnl,
    risk_status: RiskStatus,
    positions: Vec<PositionSummary>,
    strategies: Vec<StrategySummary>,
    pnl_curve: Vec<PnlPoint>,
    system_status: SystemStatus,

    // UI state
    selected_position: Option<String>,
    pnl_time_range: PnlTimeRange,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum PnlTimeRange {
    #[default]
    Today,
    Week,
    Month,
    Year,
    Year3,
    All,
}

impl PnlTimeRange {
    /// Returns the cutoff timestamp in minutes (unix minutes) for this time range.
    /// Returns None if All (no cutoff).
    fn cutoff_minutes(&self) -> Option<i64> {
        let now_secs = chrono::Utc::now().timestamp();
        let cutoff_secs = match self {
            PnlTimeRange::Today => {
                // Start of today in UTC
                let now = chrono::Utc::now();
                let start_of_day = now.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
                start_of_day.and_utc().timestamp()
            }
            PnlTimeRange::Week => now_secs - 7 * 86400,
            PnlTimeRange::Month => now_secs - 30 * 86400,
            PnlTimeRange::Year => now_secs - 365 * 86400,
            PnlTimeRange::Year3 => now_secs - 3 * 365 * 86400,
            PnlTimeRange::All => return None,
        };
        Some(cutoff_secs / 60)
    }

    fn label(&self) -> &'static str {
        match self {
            PnlTimeRange::Today => "今日",
            PnlTimeRange::Week => "本周",
            PnlTimeRange::Month => "本月",
            PnlTimeRange::Year => "本年",
            PnlTimeRange::Year3 => "三年",
            PnlTimeRange::All => "全部",
        }
    }
}

/// Single asset balance entry
#[derive(Default, Clone)]
pub struct AssetBalance {
    pub asset: String, // e.g. "USDT", "BTC", "ETH"
    pub balance: f64,
    pub available: f64,
    pub frozen: f64,
}

pub struct AccountSummary {
    pub total_balance: f64,
    pub available: f64,
    pub margin_used: f64,
    pub currency: String,
    /// All asset balances from the exchange
    pub assets: Vec<AssetBalance>,
}

#[derive(Default, Clone)]
pub struct TodayPnl {
    pub total_pnl: f64,
    pub pnl_percent: f64,
    pub win_count: usize,
    pub loss_count: usize,
    pub win_amount: f64,
    pub loss_amount: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
}

#[derive(Default, Clone)]
pub struct RiskStatus {
    pub margin_ratio: f64, // 0.0 - 1.0+
    pub warning_level: RiskLevel,
    pub liquidation_prices: Vec<(String, f64)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskLevel {
    #[default]
    Safe,    // < 50%
    Normal,  // 50-70%
    Warning, // 70-85%
    Danger,  // > 85%
}


#[derive(Default, Clone)]
pub struct PositionSummary {
    pub vt_symbol: String,
    pub direction: String,
    pub volume: f64,
    pub avg_price: f64,
    pub pnl: f64,
    pub pnl_percent: f64,
}

#[derive(Default, Clone)]
pub struct StrategySummary {
    pub name: String,
    pub state: StrategyStateDisplay,
    pub today_pnl: f64,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StrategyStateDisplay {
    Running,
    Inited,
    #[default]
    Stopped,
}

#[derive(Default, Clone)]
pub struct PnlPoint {
    pub time: i64, // Unix timestamp (minutes)
    pub cumulative_pnl: f64,
}

#[derive(Default, Clone)]
pub struct SystemStatus {
    pub gateways: Vec<GatewayStatus>,
    pub notifications: Vec<NotificationItem>,
}

#[derive(Clone)]
pub struct GatewayStatus {
    pub name: String,
    pub connected: bool,
    pub reconnecting: bool,
    pub latency_ms: u64,
    pub reconnect_attempts: u32,
}

#[derive(Clone)]
pub struct NotificationItem {
    pub time: String,
    pub message: String,
    pub level: NotificationLevel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

impl Default for DashboardPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardPanel {
    pub fn new() -> Self {
        Self {
            account_summary: AccountSummary {
                total_balance: 0.0,
                available: 0.0,
                margin_used: 0.0,
                currency: "USDT".to_string(),
                assets: Vec::new(),
            },
            today_pnl: TodayPnl::default(),
            risk_status: RiskStatus::default(),
            positions: Vec::new(),
            strategies: Vec::new(),
            pnl_curve: Vec::new(),
            system_status: SystemStatus::default(),
            selected_position: None,
            pnl_time_range: PnlTimeRange::Today,
        }
    }

    // ========================================================================
    // Data Update Methods
    // ========================================================================

    /// Update account summary data with multi-asset support
    pub fn update_account(
        &mut self,
        total: f64,
        available: f64,
        margin: f64,
        currency: &str,
        assets: Vec<AssetBalance>,
    ) {
        self.account_summary = AccountSummary {
            total_balance: total,
            available,
            margin_used: margin,
            currency: currency.to_string(),
            assets,
        };

        // Update risk status based on margin
        let margin_ratio = if total > 0.0 { margin / total } else { 0.0 };
        self.risk_status.margin_ratio = margin_ratio;
        self.risk_status.warning_level = if margin_ratio < 0.5 {
            RiskLevel::Safe
        } else if margin_ratio < 0.7 {
            RiskLevel::Normal
        } else if margin_ratio < 0.85 {
            RiskLevel::Warning
        } else {
            RiskLevel::Danger
        };
    }

    /// Update today's PnL data
    pub fn update_today_pnl(
        &mut self,
        total_pnl: f64,
        win_count: usize,
        loss_count: usize,
        win_amount: f64,
        loss_amount: f64,
    ) {
        let total_trades = win_count + loss_count;
        let win_rate = if total_trades > 0 {
            win_count as f64 / total_trades as f64 * 100.0
        } else {
            0.0
        };

        let pnl_percent = if self.account_summary.total_balance > 0.0 {
            total_pnl / self.account_summary.total_balance * 100.0
        } else {
            0.0
        };

        let profit_factor = if loss_amount.abs() > 0.0 {
            win_amount / loss_amount.abs()
        } else if win_amount > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        self.today_pnl = TodayPnl {
            total_pnl,
            pnl_percent,
            win_count,
            loss_count,
            win_amount,
            loss_amount,
            win_rate,
            profit_factor,
        };
    }

    /// Update positions summary
    pub fn update_positions(&mut self, positions: Vec<PositionSummary>) {
        self.positions = positions;
    }

    /// Update strategies summary
    pub fn update_strategies(&mut self, strategies: Vec<StrategySummary>) {
        self.strategies = strategies;
    }

    /// Update PnL curve data
    pub fn update_pnl_curve(&mut self, curve: Vec<PnlPoint>) {
        self.pnl_curve = curve;
    }

    /// Update system status
    pub fn update_system_status(
        &mut self,
        gateways: Vec<GatewayStatus>,
        notifications: Vec<NotificationItem>,
    ) {
        self.system_status = SystemStatus {
            gateways,
            notifications,
        };
    }

    // ========================================================================
    // UI Rendering
    // ========================================================================

    /// Show the dashboard panel
    pub fn show(&mut self, ui: &mut egui::Ui) -> DashboardAction {
        let mut action = DashboardAction::None;

        // Calculate responsive layout
        let available_width = ui.available_width();
        let card_spacing = 10.0;

        let is_wide = available_width > 1600.0;
        let is_medium = available_width > 1200.0;

        // Row 1: Account / Today PnL / Risk
        {
            let num_cards = if is_wide || is_medium { 3usize } else { 1 };
            let card_width =
                (available_width - card_spacing * (num_cards - 1) as f32) / num_cards as f32;

            ui.horizontal_top(|ui| {
                // Account Summary Card
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(card_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let a = self.show_account_card(ui, card_width);
                        if a != DashboardAction::None {
                            action = a;
                        }
                    },
                );

                if num_cards >= 2 {
                    ui.add_space(card_spacing);

                    // Today PnL Card
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(card_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let a = self.show_pnl_card(ui, card_width);
                            if a != DashboardAction::None {
                                action = a;
                            }
                        },
                    );
                }

                if num_cards >= 3 {
                    ui.add_space(card_spacing);

                    // Risk Status Card
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(card_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let a = self.show_risk_card(ui, card_width);
                            if a != DashboardAction::None {
                                action = a;
                            }
                        },
                    );
                }
            });
        }

        ui.add_space(card_spacing);

        // Row 2: Positions / Strategies
        if is_wide {
            let pos_width = (available_width - card_spacing) * 0.6;
            let strat_width = available_width - card_spacing - pos_width;

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(pos_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let a = self.show_positions_card(ui, pos_width);
                        if a != DashboardAction::None {
                            action = a;
                        }
                    },
                );

                ui.add_space(card_spacing);

                ui.allocate_ui_with_layout(
                    egui::Vec2::new(strat_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let a = self.show_strategies_card(ui, strat_width);
                        if a != DashboardAction::None {
                            action = a;
                        }
                    },
                );
            });
        } else {
            let a = self.show_positions_card(ui, available_width);
            if a != DashboardAction::None {
                action = a;
            }

            ui.add_space(card_spacing);

            let a = self.show_strategies_card(ui, available_width);
            if a != DashboardAction::None {
                action = a;
            }
        }

        ui.add_space(card_spacing);

        // Row 3: PnL Curve / System Status
        if is_wide || is_medium {
            let curve_width = (available_width - card_spacing) * 0.6;
            let sys_width = available_width - card_spacing - curve_width;

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(curve_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let a = self.show_pnl_curve_card(ui, curve_width);
                        if a != DashboardAction::None {
                            action = a;
                        }
                    },
                );

                ui.add_space(card_spacing);

                ui.allocate_ui_with_layout(
                    egui::Vec2::new(sys_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        let a = self.show_system_card(ui, sys_width);
                        if a != DashboardAction::None {
                            action = a;
                        }
                    },
                );
            });
        } else {
            let a = self.show_pnl_curve_card(ui, available_width);
            if a != DashboardAction::None {
                action = a;
            }

            ui.add_space(card_spacing);

            let a = self.show_system_card(ui, available_width);
            if a != DashboardAction::None {
                action = a;
            }
        }

        action
    }

    // ========================================================================
    // Individual Cards
    // ========================================================================

    /// Render a card inside a fixed-width column allocated by the layout
    fn show_account_card(&mut self, ui: &mut egui::Ui, _width: f32) -> DashboardAction {
        let mut action = DashboardAction::None;

        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("💰 账户总览").size(14.0).strong());
                });

                ui.add_space(8.0);

                // Primary balance (USDT or the main currency)
                let balance_text = format!(
                    "{:.4} {}",
                    self.account_summary.total_balance, self.account_summary.currency
                );
                ui.label(
                    RichText::new(&balance_text)
                        .size(20.0)
                        .strong()
                        .color(COLOR_TEXT_PRIMARY),
                );

                ui.add_space(6.0);

                // Available / Margin bar
                let total = self.account_summary.total_balance.max(1.0);
                let available_ratio = (self.account_summary.available / total).clamp(0.0, 1.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("可用").size(11.0).color(COLOR_TEXT_SECONDARY));

                    // Progress bar
                    let bar_rect = ui.available_rect_before_wrap();
                    let bar_width = bar_rect.width().min(100.0);
                    let bar_height = 8.0;

                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(bar_width, bar_height),
                        egui::Sense::hover(),
                    );

                    let painter = ui.painter();

                    // Background
                    painter.rect_filled(rect, 2.0, COLOR_BG_DARK);

                    // Available portion (green)
                    let available_width = bar_width * available_ratio as f32;
                    if available_width > 0.0 {
                        let available_rect =
                            Rect::from_min_size(rect.min, Vec2::new(available_width, bar_height));
                        painter.rect_filled(available_rect, 2.0, COLOR_SHORT);
                    }

                    ui.add_space(8.0);

                    // Labels
                    let avail_text = format!("{:.0}%", available_ratio * 100.0);
                    let margin_text = format!("{:.0}%", (1.0 - available_ratio) * 100.0);
                    ui.label(RichText::new(&avail_text).size(10.0).color(COLOR_SHORT));
                    ui.label(RichText::new("/").size(10.0).color(COLOR_TEXT_SECONDARY));
                    ui.label(RichText::new(&margin_text).size(10.0).color(COLOR_LONG));
                });

                ui.add_space(4.0);

                // Other asset balances (non-zero, sorted: USDT first, then by balance descending)
                if !self.account_summary.assets.is_empty() {
                    // Sort: USDT/USDC first, then by balance descending
                    let mut display_assets: Vec<&AssetBalance> = self
                        .account_summary
                        .assets
                        .iter()
                        .filter(|a| a.balance > 0.0)
                        .collect();
                    display_assets.sort_by(|a, b| {
                        let a_priority = match a.asset.as_str() {
                            "USDT" | "USDC" => 0,
                            "BTC" => 1,
                            "ETH" => 2,
                            "BNB" => 3,
                            _ => 99,
                        };
                        let b_priority = match b.asset.as_str() {
                            "USDT" | "USDC" => 0,
                            "BTC" => 1,
                            "ETH" => 2,
                            "BNB" => 3,
                            _ => 99,
                        };
                        a_priority.cmp(&b_priority).then_with(|| {
                            b.balance
                                .partial_cmp(&a.balance)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    });

                    // Show up to 5 assets, skip the primary one if it's already shown as total_balance
                    let mut shown = 0;
                    for asset in display_assets.iter() {
                        if shown >= 5 {
                            break;
                        }
                        // Skip the primary currency if it's already shown above as total_balance
                        if asset.asset == self.account_summary.currency {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}:", asset.asset))
                                    .size(10.0)
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                            ui.label(
                                RichText::new(format!("{:.6}", asset.balance))
                                    .size(10.0)
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                            if asset.frozen > 0.0 {
                                ui.label(
                                    RichText::new(format!("冻结 {:.6}", asset.frozen))
                                        .size(9.0)
                                        .color(COLOR_LONG),
                                );
                            }
                        });
                        shown += 1;
                    }

                    // Show remaining count
                    let remaining = display_assets.len().saturating_sub(5);
                    if remaining > 0 {
                        ui.label(
                            RichText::new(format!("+{} 种资产", remaining))
                                .size(9.0)
                                .color(COLOR_TEXT_SECONDARY),
                        );
                    }
                }

                ui.add_space(4.0);

                // View detail link
                if ui
                    .button(
                        RichText::new("查看详情 →")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 150, 255)),
                    )
                    .clicked()
                {
                    action = DashboardAction::NavigateToAccount;
                }
            });

        action
    }

    fn show_pnl_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        let mut action = DashboardAction::None;

        let is_profit = self.today_pnl.total_pnl >= 0.0;
        let pnl_color = if is_profit {
            COLOR_POSITIVE
        } else {
            COLOR_NEGATIVE
        };

        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(116.0);

                // Header
                ui.label(RichText::new("📊 今日盈亏").size(14.0).strong());

                ui.add_space(8.0);

                // PnL amount
                let pnl_sign = if is_profit { "+" } else { "" };
                let pnl_text = format!("{}{:.2}", pnl_sign, self.today_pnl.total_pnl);
                ui.label(
                    RichText::new(&pnl_text)
                        .size(20.0)
                        .strong()
                        .color(pnl_color),
                );

                // PnL percent
                let percent_text = format!("({}{:.2}%)", pnl_sign, self.today_pnl.pnl_percent);
                ui.label(RichText::new(&percent_text).size(12.0).color(pnl_color));

                ui.add_space(6.0);

                // Win/Loss stats
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("盈利 {} 笔", self.today_pnl.win_count))
                            .size(10.0)
                            .color(COLOR_POSITIVE),
                    );
                    ui.label(RichText::new("|").size(10.0).color(COLOR_TEXT_SECONDARY));
                    ui.label(
                        RichText::new(format!("亏损 {} 笔", self.today_pnl.loss_count))
                            .size(10.0)
                            .color(COLOR_NEGATIVE),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("胜率: {:.0}%", self.today_pnl.win_rate))
                            .size(10.0)
                            .color(COLOR_TEXT_SECONDARY),
                    );
                    ui.label(RichText::new("|").size(10.0).color(COLOR_TEXT_SECONDARY));
                    let pf_text = if self.today_pnl.profit_factor.is_finite() {
                        format!("盈亏比: {:.1}", self.today_pnl.profit_factor)
                    } else {
                        "盈亏比: ∞".to_string()
                    };
                    ui.label(
                        RichText::new(pf_text)
                            .size(10.0)
                            .color(COLOR_TEXT_SECONDARY),
                    );
                });

                ui.add_space(4.0);

                if ui
                    .button(
                        RichText::new("查看成交 →")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 150, 255)),
                    )
                    .clicked()
                {
                    action = DashboardAction::NavigateToTrades;
                }
            });

        action
    }

    fn show_risk_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(116.0);

                // Header
                ui.label(RichText::new("⚠️ 风险监控").size(14.0).strong());

                ui.add_space(8.0);

                // Margin ratio gauge
                let ratio = self.risk_status.margin_ratio;
                let (level_color, level_text) = match self.risk_status.warning_level {
                    RiskLevel::Safe => (Color32::from_rgb(80, 200, 80), "安全"),
                    RiskLevel::Normal => (Color32::from_rgb(200, 200, 80), "正常"),
                    RiskLevel::Warning => (Color32::from_rgb(255, 150, 50), "警告"),
                    RiskLevel::Danger => (Color32::from_rgb(255, 80, 80), "危险"),
                };

                // Progress bar for margin ratio
                let bar_rect = ui.available_rect_before_wrap();
                let bar_width = bar_rect.width().min(150.0);
                let bar_height = 12.0;

                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(bar_width, bar_height), egui::Sense::hover());

                let painter = ui.painter();

                // Background
                painter.rect_filled(rect, 3.0, COLOR_BG_DARK);

                // Warning zone markers (70%, 85%)
                let marker_70 = rect.left() + bar_width * 0.7;
                let marker_85 = rect.left() + bar_width * 0.85;
                painter.line_segment(
                    [
                        Pos2::new(marker_70, rect.top()),
                        Pos2::new(marker_70, rect.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 150, 50, 100)),
                );
                painter.line_segment(
                    [
                        Pos2::new(marker_85, rect.top()),
                        Pos2::new(marker_85, rect.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 80, 80, 100)),
                );

                // Current ratio bar
                let ratio_clamped = ratio.clamp(0.0, 1.0);
                let ratio_width = bar_width * ratio_clamped as f32;
                if ratio_width > 0.0 {
                    let ratio_rect =
                        Rect::from_min_size(rect.min, Vec2::new(ratio_width, bar_height));
                    painter.rect_filled(ratio_rect, 3.0, level_color);
                }

                // Labels
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("保证金占用: {:.0}%", ratio * 100.0))
                            .size(11.0)
                            .color(COLOR_TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new(format!("[{}]", level_text))
                            .size(11.0)
                            .color(level_color)
                            .strong(),
                    );
                });

                ui.add_space(6.0);

                // Liquidation prices
                if !self.risk_status.liquidation_prices.is_empty() {
                    ui.label(
                        RichText::new("预估强平价:")
                            .size(10.0)
                            .color(COLOR_TEXT_SECONDARY),
                    );
                    for (symbol, price) in self.risk_status.liquidation_prices.iter().take(2) {
                        ui.label(
                            RichText::new(format!("  {}: {:.2}", symbol, price))
                                .size(10.0)
                                .color(COLOR_TEXT_SECONDARY),
                        );
                    }
                }
            });

        DashboardAction::None
    }

    fn show_positions_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        let mut action = DashboardAction::None;

        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(156.0);

                // Header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📦 持仓概览").size(14.0).strong());
                    ui.label(
                        RichText::new(format!("({})", self.positions.len()))
                            .size(12.0)
                            .color(COLOR_TEXT_SECONDARY),
                    );
                });

                ui.add_space(8.0);

                // Positions table (mini)
                egui::Grid::new("dashboard_positions")
                    .num_columns(5)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        // Header
                        ui.label(RichText::new("品种").size(10.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new("方向").size(10.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new("持仓").size(10.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new("均价").size(10.0).color(COLOR_TEXT_SECONDARY));
                        ui.label(RichText::new("盈亏").size(10.0).color(COLOR_TEXT_SECONDARY));
                        ui.end_row();

                        // Rows (max 5)
                        for pos in self.positions.iter().take(5) {
                            let is_profit = pos.pnl >= 0.0;
                            let pnl_color = if is_profit {
                                COLOR_POSITIVE
                            } else {
                                COLOR_NEGATIVE
                            };

                            // Clickable symbol to open chart
                            let symbol_response = ui.add(
                                egui::Label::new(
                                    RichText::new(&pos.vt_symbol)
                                        .size(11.0)
                                        .color(Color32::from_rgb(100, 150, 255)),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if symbol_response.clicked() {
                                action = DashboardAction::OpenChart(pos.vt_symbol.clone());
                            }
                            symbol_response.on_hover_cursor(egui::CursorIcon::PointingHand);

                            let dir_text = if pos.direction == "Long" {
                                "多"
                            } else {
                                "空"
                            };
                            let dir_color = if pos.direction == "Long" {
                                COLOR_LONG
                            } else {
                                COLOR_SHORT
                            };
                            ui.label(RichText::new(dir_text).size(11.0).color(dir_color));

                            ui.label(
                                RichText::new(format!("{:.4}", pos.volume))
                                    .size(11.0)
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                            ui.label(
                                RichText::new(format!("{:.2}", pos.avg_price))
                                    .size(11.0)
                                    .color(COLOR_TEXT_PRIMARY),
                            );

                            let pnl_sign = if is_profit { "+" } else { "" };
                            ui.label(
                                RichText::new(format!("{}{:.2}", pnl_sign, pos.pnl))
                                    .size(11.0)
                                    .color(pnl_color),
                            );

                            ui.end_row();
                        }
                    });

                // Total floating PnL
                if !self.positions.is_empty() {
                    let total_pnl: f64 = self.positions.iter().map(|p| p.pnl).sum();
                    let is_profit = total_pnl >= 0.0;
                    let pnl_color = if is_profit {
                        COLOR_POSITIVE
                    } else {
                        COLOR_NEGATIVE
                    };
                    let pnl_sign = if is_profit { "+" } else { "" };

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("合计浮动盈亏: {}{:.2}", pnl_sign, total_pnl))
                            .size(11.0)
                            .color(pnl_color),
                    );
                }

                ui.add_space(4.0);

                if ui
                    .button(
                        RichText::new("查看全部持仓 →")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 150, 255)),
                    )
                    .clicked()
                {
                    action = DashboardAction::NavigateToPositions;
                }
            });

        action
    }

    fn show_strategies_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        let mut action = DashboardAction::None;

        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(156.0);

                // Header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🤖 策略状态").size(14.0).strong());
                    ui.label(
                        RichText::new(format!("({})", self.strategies.len()))
                            .size(12.0)
                            .color(COLOR_TEXT_SECONDARY),
                    );
                });

                ui.add_space(8.0);

                // Strategy list
                for strategy in self.strategies.iter().take(5) {
                    let (state_icon, state_color, state_text) = match strategy.state {
                        StrategyStateDisplay::Running => {
                            ("🟢", Color32::from_rgb(80, 200, 80), "运行中")
                        }
                        StrategyStateDisplay::Inited => {
                            ("🟡", Color32::from_rgb(200, 200, 80), "已初始化")
                        }
                        StrategyStateDisplay::Stopped => {
                            ("🔴", Color32::from_rgb(200, 80, 80), "已停止")
                        }
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new(state_icon).size(12.0));
                        ui.label(
                            RichText::new(&strategy.name)
                                .size(11.0)
                                .color(COLOR_TEXT_PRIMARY),
                        );
                        ui.label(RichText::new(state_text).size(10.0).color(state_color));

                        if strategy.state != StrategyStateDisplay::Stopped {
                            let is_profit = strategy.today_pnl >= 0.0;
                            let pnl_color = if is_profit {
                                COLOR_POSITIVE
                            } else {
                                COLOR_NEGATIVE
                            };
                            let pnl_sign = if is_profit { "+" } else { "" };
                            ui.label(
                                RichText::new(format!("{}{:.2}", pnl_sign, strategy.today_pnl))
                                    .size(10.0)
                                    .color(pnl_color),
                            );
                        }
                    });
                }

                ui.add_space(6.0);

                // Batch buttons
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("▶ 全部启动").size(10.0)).clicked() {
                        action = DashboardAction::StartAllStrategies;
                    }
                    if ui.button(RichText::new("⏹ 全部停止").size(10.0)).clicked() {
                        action = DashboardAction::StopAllStrategies;
                    }
                });

                ui.add_space(4.0);

                if ui
                    .button(
                        RichText::new("管理策略 →")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 150, 255)),
                    )
                    .clicked()
                {
                    action = DashboardAction::NavigateToStrategies;
                }
            });

        action
    }

    fn show_pnl_curve_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(136.0);

                // Header with time range selector
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📈 盈亏曲线").size(14.0).strong());

                    // Time range selector
                    egui::ComboBox::from_id_salt("pnl_time_range")
                        .selected_text(self.pnl_time_range.label())
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::Today,
                                "今日",
                            );
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::Week,
                                "本周",
                            );
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::Month,
                                "本月",
                            );
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::Year,
                                "本年",
                            );
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::Year3,
                                "三年",
                            );
                            ui.selectable_value(
                                &mut self.pnl_time_range,
                                PnlTimeRange::All,
                                "全部",
                            );
                        });
                });

                ui.add_space(8.0);

                // Draw curve
                let rect = ui.available_rect_before_wrap();
                let curve_height = rect.height().min(80.0);

                let (curve_rect, _) = ui.allocate_exact_size(
                    Vec2::new(rect.width(), curve_height),
                    egui::Sense::hover(),
                );

                let painter = ui.painter();

                // Background
                painter.rect_filled(curve_rect, 2.0, COLOR_BG_DARK);

                // Zero line
                let zero_y = curve_rect.center().y;
                painter.line_segment(
                    [
                        Pos2::new(curve_rect.left(), zero_y),
                        Pos2::new(curve_rect.right(), zero_y),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(128, 128, 128, 80)),
                );

                // Draw curve if data exists
                // Filter by time range
                let cutoff = self.pnl_time_range.cutoff_minutes();
                let filtered_curve: Vec<&PnlPoint> = self
                    .pnl_curve
                    .iter()
                    .filter(|p| cutoff.is_none_or(|c| p.time >= c))
                    .collect();

                if filtered_curve.len() >= 2 {
                    // Find min/max for scaling
                    let min_pnl = filtered_curve
                        .iter()
                        .map(|p| p.cumulative_pnl)
                        .fold(f64::INFINITY, f64::min);
                    let max_pnl = filtered_curve
                        .iter()
                        .map(|p| p.cumulative_pnl)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let range = (max_pnl - min_pnl).max(1.0);

                    // Build points
                    let points: Vec<Pos2> = filtered_curve
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let x = curve_rect.left()
                                + (i as f32 / (filtered_curve.len() - 1).max(1) as f32)
                                    * curve_rect.width();
                            let normalized = (p.cumulative_pnl - min_pnl) / range;
                            let y = curve_rect.bottom() - (normalized as f32 * curve_rect.height());
                            Pos2::new(x, y)
                        })
                        .collect();

                    // Determine color based on final value
                    let final_pnl = filtered_curve
                        .last()
                        .map(|p| p.cumulative_pnl)
                        .unwrap_or(0.0);
                    let curve_color = if final_pnl >= 0.0 {
                        COLOR_POSITIVE
                    } else {
                        COLOR_NEGATIVE
                    };

                    // Draw line
                    if points.len() >= 2 {
                        painter.add(egui::Shape::line(points, Stroke::new(1.5, curve_color)));
                    }
                } else {
                    // No data message
                    painter.text(
                        curve_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "暂无数据",
                        egui::FontId::default(),
                        COLOR_TEXT_SECONDARY,
                    );
                }

                // Stats below curve
                if !filtered_curve.is_empty() {
                    let max_pnl = filtered_curve
                        .iter()
                        .map(|p| p.cumulative_pnl)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let min_pnl = filtered_curve
                        .iter()
                        .map(|p| p.cumulative_pnl)
                        .fold(f64::INFINITY, f64::min);
                    let current_pnl = filtered_curve
                        .last()
                        .map(|p| p.cumulative_pnl)
                        .unwrap_or(0.0);

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("最高: {:.2}", max_pnl))
                                .size(10.0)
                                .color(COLOR_POSITIVE),
                        );
                        ui.label(
                            RichText::new(format!("最低: {:.2}", min_pnl))
                                .size(10.0)
                                .color(COLOR_NEGATIVE),
                        );
                        let current_color = if current_pnl >= 0.0 {
                            COLOR_POSITIVE
                        } else {
                            COLOR_NEGATIVE
                        };
                        ui.label(
                            RichText::new(format!("当前: {:.2}", current_pnl))
                                .size(10.0)
                                .color(current_color),
                        );
                    });
                }
            });

        DashboardAction::None
    }

    fn show_system_card(&mut self, ui: &mut egui::Ui, width: f32) -> DashboardAction {
        let mut action = DashboardAction::None;

        egui::Frame::NONE
            .fill(COLOR_BG_MEDIUM)
            .inner_margin(12.0)
            .outer_margin(0.0)
            .show(ui, |ui| {
                ui.set_min_width(width - 24.0);
                ui.set_min_height(136.0);

                // Header
                ui.label(RichText::new("⚙️ 系统状态").size(14.0).strong());

                ui.add_space(8.0);

                // Gateway status
                ui.label(
                    RichText::new("网关连接:")
                        .size(11.0)
                        .color(COLOR_TEXT_SECONDARY),
                );

                for gw in self.system_status.gateways.iter().take(3) {
                    let (icon, color) = if gw.connected {
                        ("🟢", Color32::from_rgb(80, 200, 80))
                    } else if gw.reconnecting {
                        ("🟡", Color32::from_rgb(200, 200, 80))
                    } else {
                        ("🔴", Color32::from_rgb(200, 80, 80))
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).size(10.0));
                        ui.label(RichText::new(&gw.name).size(10.0).color(COLOR_TEXT_PRIMARY));

                        if gw.connected {
                            ui.label(
                                RichText::new(format!("{}ms", gw.latency_ms))
                                    .size(9.0)
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                        } else if gw.reconnecting {
                            ui.label(
                                RichText::new(format!("重连中({})", gw.reconnect_attempts))
                                    .size(9.0)
                                    .color(color),
                            );
                        } else {
                            ui.label(RichText::new("已断开").size(9.0).color(color));
                        }
                    });
                }

                ui.add_space(6.0);

                // Recent notifications
                if !self.system_status.notifications.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "📢 通知 ({}条)",
                            self.system_status.notifications.len()
                        ))
                        .size(11.0)
                        .color(COLOR_TEXT_SECONDARY),
                    );

                    for notif in self.system_status.notifications.iter().take(3) {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&notif.time)
                                    .size(9.0)
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                            ui.label(
                                RichText::new(&notif.message)
                                    .size(9.0)
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                        });
                    }
                }

                ui.add_space(4.0);

                if ui
                    .button(
                        RichText::new("查看全部通知 →")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 150, 255)),
                    )
                    .clicked()
                {
                    action = DashboardAction::NavigateToNotifications;
                }
            });

        action
    }

    // ========================================================================
    // Data Getters
    // ========================================================================

    /// Get position selected for chart opening
    pub fn take_selected_position(&mut self) -> Option<String> {
        self.selected_position.take()
    }
}

// ============================================================================
// Action Enum
// ============================================================================

#[derive(Default, Clone, PartialEq, Eq)]
pub enum DashboardAction {
    #[default]
    None,
    NavigateToAccount,
    NavigateToPositions,
    NavigateToTrades,
    NavigateToStrategies,
    NavigateToNotifications,
    StartAllStrategies,
    StopAllStrategies,
    OpenChart(String),
}
