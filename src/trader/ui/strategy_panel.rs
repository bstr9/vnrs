//! Strategy Management UI Panel
//!
//! Provides a table of loaded strategies with color-coded status
//! and buttons for Init/Start/Stop lifecycle control.

use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use super::widget::SortState;
use super::workflow_state::StrategyDeployConfig;

/// A single strategy row for display
#[derive(Clone)]
pub struct StrategyRow {
    pub name: String,
    pub state: String,
    pub strategy_type: String,
    pub symbols: String,
}

/// Get the display color for a strategy state string
fn state_color(state: &str) -> Color32 {
    match state {
        "NotInited" => Color32::GRAY,
        "Inited" => Color32::from_rgb(100, 150, 255),
        "Trading" => Color32::GREEN,
        "Stopped" => Color32::RED,
        "Error" => Color32::YELLOW,
        _ => Color32::GRAY,
    }
}

/// Get the Chinese display label for a strategy state
fn state_label(state: &str) -> &str {
    match state {
        "NotInited" => "未初始化",
        "Inited" => "已初始化",
        "Trading" => "交易中",
        "Stopped" => "已停止",
        "Error" => "错误",
        _ => state,
    }
}

/// Strategy management panel
pub struct StrategyPanel {
    strategies: Vec<StrategyRow>,
    sort: SortState,
    selected: Option<String>,
    pending_init: Option<String>,
    pending_start: Option<String>,
    pending_stop: Option<String>,
    pending_remove: Option<String>,
    /// Deploy config from workflow (backtest → deploy)
    deploy_config: Option<StrategyDeployConfig>,
}

impl Default for StrategyPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyPanel {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            sort: SortState::new(),
            selected: None,
            pending_init: None,
            pending_start: None,
            pending_stop: None,
            pending_remove: None,
            deploy_config: None,
        }
    }

    /// Receive updated strategy rows from external source
    pub fn update_strategies(&mut self, strategies: Vec<StrategyRow>) {
        self.strategies = strategies;
    }

    /// Take and clear pending init action
    pub fn take_init(&mut self) -> Option<String> {
        self.pending_init.take()
    }

    /// Take and clear pending start action
    pub fn take_start(&mut self) -> Option<String> {
        self.pending_start.take()
    }

    /// Take and clear pending stop action
    pub fn take_stop(&mut self) -> Option<String> {
        self.pending_stop.take()
    }

    /// Take and clear pending remove action
    pub fn take_remove(&mut self) -> Option<String> {
        self.pending_remove.take()
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Set deploy config from workflow (backtest → deploy)
    pub fn set_deploy_config(&mut self, config: StrategyDeployConfig) {
        self.deploy_config = Some(config);
    }

    /// Take and clear the deploy config
    pub fn take_deploy_config(&mut self) -> Option<StrategyDeployConfig> {
        self.deploy_config.take()
    }

    /// Sort strategy rows based on current sort state
    fn sort_rows(&self, rows: &mut Vec<&StrategyRow>) {
        let col = match self.sort.column {
            Some(c) => c,
            None => {
                rows.sort_by(|a, b| a.name.cmp(&b.name));
                return;
            }
        };
        let sort = &self.sort;
        match col {
            0 => rows.sort_by(|a, b| sort.apply_order(a.name.cmp(&b.name))),
            1 => rows.sort_by(|a, b| sort.apply_order(a.state.cmp(&b.state))),
            2 => rows.sort_by(|a, b| sort.apply_order(a.strategy_type.cmp(&b.strategy_type))),
            3 => rows.sort_by(|a, b| sort.apply_order(a.symbols.cmp(&b.symbols))),
            _ => rows.sort_by(|a, b| a.name.cmp(&b.name)),
        }
    }

    /// Render the strategy panel
    pub fn show(&mut self, ui: &mut Ui) {
        // Show deploy config banner if present
        if self.deploy_config.is_some() {
            // Extract config info for display before the closure
            let config_info = self.deploy_config.as_ref().map(|c| (
                c.strategy_name.clone(),
                c.strategy_class.clone(),
                c.vt_symbol.clone(),
                c.mode.to_string(),
                c.capital,
                c.rate,
                c.slippage,
            ));
            
            if let Some((name, class, symbol, mode, capital, rate, slippage)) = config_info {
                egui::Frame::group(ui.style())
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 150, 255)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🚀 策略部署配置").color(egui::Color32::from_rgb(80, 150, 255)).strong());
                        });
                        ui.label(format!("策略: {} ({})", name, class));
                        ui.label(format!("品种: {} | 模式: {}", symbol, mode));
                        ui.label(format!("资金: {:.0} | 手续费: {:.4} | 滑点: {:.4}", capital, rate, slippage));
                        ui.horizontal(|ui| {
                            if ui.button("初始化策略").clicked() {
                                // Select the strategy if it matches
                                self.selected = Some(name.clone());
                                self.pending_init = Some(name.clone());
                            }
                            if ui.button("清除配置").clicked() {
                                self.deploy_config = None;
                            }
                        });
                    });
                ui.separator();
            }
        }
        
        let available_height = ui.available_height();
        let selection_bg = ui.visuals().selection.bg_fill;

        // Table headers
        let headers = ["策略名", "状态", "类型", "合约"];

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::auto().at_least(80.0), headers.len())
            .min_scrolled_height(available_height - 40.0)
            .header(20.0, |mut header| {
                for (i, h) in headers.iter().enumerate() {
                    header.col(|ui| {
                        let label = format!("{}{}", h, self.sort.indicator(i));
                        let response = ui.strong(&label).interact(egui::Sense::click());
                        if response.clicked() {
                            self.sort.toggle(i);
                        }
                        if response.hovered() {
                            ui.painter().rect_filled(
                                response.rect,
                                0.0,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                            );
                        }
                    });
                }
            })
            .body(|mut body| {
                let mut rows: Vec<&StrategyRow> = self.strategies.iter().collect();
                self.sort_rows(&mut rows);

                for row in &rows {
                    let is_selected = self.selected.as_deref() == Some(&row.name);
                    body.row(18.0, |mut table_row| {
                        if is_selected {
                            table_row.col(|ui| {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, selection_bg);
                                ui.label(RichText::new(&row.name).strong());
                            });
                        } else {
                            table_row.col(|ui| {
                                if ui.selectable_label(false, &row.name).clicked() {
                                    self.selected = Some(row.name.clone());
                                }
                            });
                        }

                        table_row.col(|ui| {
                            let color = state_color(&row.state);
                            let label = state_label(&row.state);
                            ui.label(RichText::new(label).color(color));
                        });

                        table_row.col(|ui| {
                            ui.label(&row.strategy_type);
                        });

                        table_row.col(|ui| {
                            ui.label(&row.symbols);
                        });
                    });
                }
            });

        // Action buttons
        ui.separator();
        ui.horizontal(|ui| {
            let has_selection = self.selected.is_some();
            ui.add_enabled_ui(has_selection, |ui| {
                if ui.button("初始化").clicked() {
                    if let Some(ref name) = self.selected {
                        self.pending_init = Some(name.clone());
                    }
                }
                if ui.button("启动").clicked() {
                    if let Some(ref name) = self.selected {
                        self.pending_start = Some(name.clone());
                    }
                }
                if ui.button("停止").clicked() {
                    if let Some(ref name) = self.selected {
                        self.pending_stop = Some(name.clone());
                    }
                }
                // Remove button with destructive styling
                let remove_button = ui.add(
                    egui::Button::new(egui::RichText::new("移除").color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(180, 60, 60))
                );
                if remove_button.clicked() {
                    if let Some(ref name) = self.selected {
                        self.pending_remove = Some(name.clone());
                    }
                }
            });
        });
    }
}
