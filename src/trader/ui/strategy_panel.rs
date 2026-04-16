//! Strategy Management UI Panel
//!
//! Provides a table of loaded strategies with color-coded status
//! and buttons for Init/Start/Stop lifecycle control.

use egui::{Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use super::widget::SortState;

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

                let selection_bg = selection_bg;

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
            });
        });
    }
}
