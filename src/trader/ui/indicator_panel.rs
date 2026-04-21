//! Technical indicator configuration panel.
//!
//! This module provides a UI panel for selecting and configuring technical indicators
//! to overlay on candlestick charts.

use egui::{Color32, Ui, RichText};
use std::collections::HashMap;

use crate::chart::{IndicatorType, IndicatorLocation};

/// Indicator category for grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorCategory {
    Trend,          // MA, EMA, WMA
    Volatility,     // BOLL, ATR
    Volume,         // VWAP, AVL, MFI
    Oscillator,     // TRIX, RSI, MACD, CCI
    TrendFollowing, // SAR, SUPER
    Momentum,       // KDJ
}

impl IndicatorCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            IndicatorCategory::Trend => "趋势指标",
            IndicatorCategory::Volatility => "波动率指标",
            IndicatorCategory::Volume => "成交量指标",
            IndicatorCategory::Oscillator => "震荡指标",
            IndicatorCategory::TrendFollowing => "趋势跟踪指标",
            IndicatorCategory::Momentum => "动量指标",
        }
    }
    
    pub fn all() -> Vec<IndicatorCategory> {
        vec![
            IndicatorCategory::Trend,
            IndicatorCategory::Volatility,
            IndicatorCategory::Volume,
            IndicatorCategory::Oscillator,
            IndicatorCategory::TrendFollowing,
            IndicatorCategory::Momentum,
        ]
    }
}

/// Configuration for a single indicator
#[derive(Debug, Clone)]
pub struct IndicatorConfigEntry {
    /// Indicator type
    pub indicator_type: IndicatorType,
    /// Primary period (MA, EMA, WMA, BOLL, TRIX, SUPER, RSI, ATR, CCI, MFI)
    pub period: usize,
    /// Multiplier for BOLL and SUPER
    pub multiplier: f64,
    /// Signal period for TRIX, KDJ
    pub signal_period: usize,
    /// Fast period for MACD
    pub fast_period: usize,
    /// Slow period for MACD
    pub slow_period: usize,
    /// Line color
    pub color: Color32,
    /// Signal line color (for TRIX, MACD)
    pub signal_color: Color32,
    /// Histogram / third line color (for MACD, KDJ)
    pub hist_color: Color32,
    /// Location on chart (Main or Sub)
    pub location: IndicatorLocation,
    /// Line width
    pub line_width: f32,
    /// Whether this indicator is enabled
    pub enabled: bool,
}

impl Default for IndicatorConfigEntry {
    fn default() -> Self {
        Self {
            indicator_type: IndicatorType::MA,
            period: 20,
            multiplier: 2.0,
            signal_period: 9,
            fast_period: 12,
            slow_period: 26,
            color: Color32::YELLOW,
            signal_color: Color32::from_rgb(255, 100, 0),
            hist_color: Color32::from_rgb(100, 200, 100),
            location: IndicatorLocation::Main,
            line_width: 1.5,
            enabled: false,
        }
    }
}

impl IndicatorConfigEntry {
    /// Create a new config entry for a specific indicator type with defaults
    pub fn new(indicator_type: IndicatorType) -> Self {
        let mut config = Self::default();
        config.indicator_type = indicator_type;
        config.apply_type_defaults();
        config
    }
    
    /// Apply default values based on indicator type
    fn apply_type_defaults(&mut self) {
        match self.indicator_type {
            IndicatorType::MA => {
                self.period = 20;
                self.color = Color32::YELLOW;
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::EMA => {
                self.period = 20;
                self.color = Color32::from_rgb(0, 200, 255);
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::WMA => {
                self.period = 20;
                self.color = Color32::from_rgb(255, 150, 0);
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::BOLL => {
                self.period = 20;
                self.multiplier = 2.0;
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::VWAP => {
                self.color = Color32::from_rgb(0, 255, 200);
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::AVL => {
                self.color = Color32::WHITE;
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::TRIX => {
                self.period = 12;
                self.signal_period = 9;
                self.color = Color32::from_rgb(200, 100, 255);
                self.signal_color = Color32::from_rgb(255, 100, 0);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::SAR => {
                self.multiplier = 0.02; // acceleration
                self.color = Color32::from_rgb(0, 255, 0);
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::SUPER => {
                self.period = 10;
                self.multiplier = 3.0;
                self.location = IndicatorLocation::Main;
            }
            IndicatorType::RSI => {
                self.period = 14;
                self.color = Color32::from_rgb(200, 200, 0);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::MACD => {
                self.fast_period = 12;
                self.slow_period = 26;
                self.signal_period = 9;
                self.color = Color32::from_rgb(100, 200, 255);
                self.signal_color = Color32::from_rgb(255, 100, 0);
                self.hist_color = Color32::from_rgb(100, 200, 100);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::ATR => {
                self.period = 14;
                self.color = Color32::from_rgb(200, 100, 100);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::KDJ => {
                self.period = 9;
                self.signal_period = 3;
                self.color = Color32::from_rgb(255, 255, 0);
                self.signal_color = Color32::from_rgb(0, 200, 255);
                self.hist_color = Color32::from_rgb(255, 100, 200);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::CCI => {
                self.period = 14;
                self.color = Color32::from_rgb(200, 150, 255);
                self.location = IndicatorLocation::Sub;
            }
            IndicatorType::MFI => {
                self.period = 14;
                self.color = Color32::from_rgb(100, 255, 200);
                self.location = IndicatorLocation::Sub;
            }
        }
    }
    
    /// Get a unique key for this config
    pub fn key(&self) -> String {
        format!("{:?}_{}", self.indicator_type, self.period)
    }
    
    /// Get display name
    pub fn display_name(&self) -> String {
        match self.indicator_type {
            IndicatorType::MA => format!("MA({})", self.period),
            IndicatorType::EMA => format!("EMA({})", self.period),
            IndicatorType::WMA => format!("WMA({})", self.period),
            IndicatorType::BOLL => format!("BOLL({}, {})", self.period, self.multiplier),
            IndicatorType::VWAP => "VWAP".to_string(),
            IndicatorType::AVL => "AVL".to_string(),
            IndicatorType::TRIX => format!("TRIX({}, {})", self.period, self.signal_period),
            IndicatorType::SAR => format!("SAR({}, 0.2)", self.multiplier),
            IndicatorType::SUPER => format!("SUPER({}, {})", self.period, self.multiplier),
            IndicatorType::RSI => format!("RSI({})", self.period),
            IndicatorType::MACD => format!("MACD({}, {}, {})", self.fast_period, self.slow_period, self.signal_period),
            IndicatorType::ATR => format!("ATR({})", self.period),
            IndicatorType::KDJ => format!("KDJ({}, {})", self.period, self.signal_period),
            IndicatorType::CCI => format!("CCI({})", self.period),
            IndicatorType::MFI => format!("MFI({})", self.period),
        }
    }
    
    /// Get category for this indicator
    pub fn category(&self) -> IndicatorCategory {
        match self.indicator_type {
            IndicatorType::MA | IndicatorType::EMA | IndicatorType::WMA => IndicatorCategory::Trend,
            IndicatorType::BOLL | IndicatorType::ATR => IndicatorCategory::Volatility,
            IndicatorType::VWAP | IndicatorType::AVL | IndicatorType::MFI => IndicatorCategory::Volume,
            IndicatorType::TRIX | IndicatorType::RSI | IndicatorType::MACD | IndicatorType::CCI => IndicatorCategory::Oscillator,
            IndicatorType::SAR | IndicatorType::SUPER => IndicatorCategory::TrendFollowing,
            IndicatorType::KDJ => IndicatorCategory::Momentum,
        }
    }
}

/// Saved indicator preset
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndicatorPreset {
    pub name: String,
    pub indicators: Vec<IndicatorConfigEntry>,
}

/// Indicator panel state
pub struct IndicatorPanel {
    /// Available indicator configs (one per type)
    available_indicators: HashMap<IndicatorType, IndicatorConfigEntry>,
    /// Currently selected indicator type for configuration
    selected_type: Option<IndicatorType>,
    /// Currently editing config (clone of selected)
    editing_config: Option<IndicatorConfigEntry>,
    /// Saved presets
    presets: Vec<IndicatorPreset>,
    /// Selected preset index
    selected_preset: Option<usize>,
    /// New preset name input
    new_preset_name: String,
    /// Show preset save dialog
    show_save_preset: bool,
    /// Pending action: apply indicators to chart
    pending_apply: Option<Vec<IndicatorConfigEntry>>,
}

impl Default for IndicatorPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorPanel {
    /// Create a new indicator panel
    pub fn new() -> Self {
        let mut available_indicators = HashMap::new();
        for it in IndicatorType::all() {
            available_indicators.insert(it, IndicatorConfigEntry::new(it));
        }
        
        Self {
            available_indicators,
            selected_type: None,
            editing_config: None,
            presets: Vec::new(),
            selected_preset: None,
            new_preset_name: String::new(),
            show_save_preset: false,
            pending_apply: None,
        }
    }
    
    /// Get all enabled indicators
    pub fn get_enabled_indicators(&self) -> Vec<&IndicatorConfigEntry> {
        self.available_indicators
            .values()
            .filter(|c| c.enabled)
            .collect()
    }
    
    /// Take pending apply action
    pub fn take_apply(&mut self) -> Option<Vec<IndicatorConfigEntry>> {
        self.pending_apply.take()
    }
    
    /// Show the indicator panel UI
    pub fn show(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("技术指标配置");
            ui.separator();
            
            // Show indicator list by category
            self.show_indicator_list(ui);
            
            ui.separator();
            
            // Show selected indicator configuration
            if let Some(config) = &self.editing_config {
                self.show_indicator_config(ui, config.clone());
            } else {
                ui.label(RichText::new("请从上方选择一个指标进行配置").color(Color32::GRAY));
            }
            
            ui.separator();
            
            // Action buttons
            self.show_action_buttons(ui);
            
            ui.separator();
            
            // Presets section
            self.show_presets(ui);
        });
    }
    
    /// Show indicator list grouped by category
    fn show_indicator_list(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("可用指标").strong());
        
        for category in IndicatorCategory::all() {
            ui.collapsing(category.display_name(), |ui| {
                for it in IndicatorType::all() {
                    let config = self.available_indicators.get(&it).cloned().unwrap_or_default();
                    if config.category() == category {
                        let mut enabled = config.enabled;
                        let type_name = it.display_name();
                        
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut enabled, "").changed() {
                                if let Some(c) = self.available_indicators.get_mut(&it) {
                                    c.enabled = enabled;
                                }
                            }
                            
                            // Make the label clickable to select for editing
                            let label = if self.selected_type == Some(it) {
                                RichText::new(type_name).color(Color32::from_rgb(100, 200, 255))
                            } else {
                                RichText::new(type_name)
                            };
                            
                            if ui.selectable_label(self.selected_type == Some(it), label).clicked() {
                                self.selected_type = Some(it);
                                self.editing_config = self.available_indicators.get(&it).cloned();
                            }
                        });
                    }
                }
            });
        }
    }
    
    /// Show configuration for a specific indicator
    fn show_indicator_config(&mut self, ui: &mut Ui, mut config: IndicatorConfigEntry) {
        ui.label(RichText::new(format!("配置: {}", config.display_name())).strong());
        
        // Period parameter (for MA, EMA, WMA, BOLL, TRIX, SUPER, RSI, ATR, CCI, MFI, KDJ)
        if matches!(config.indicator_type, 
            IndicatorType::MA | IndicatorType::EMA | IndicatorType::WMA | 
            IndicatorType::BOLL | IndicatorType::TRIX | IndicatorType::SUPER |
            IndicatorType::RSI | IndicatorType::ATR | IndicatorType::CCI |
            IndicatorType::MFI | IndicatorType::KDJ) {
            ui.horizontal(|ui| {
                ui.label("周期:");
                let mut period = config.period as i32;
                ui.add(egui::DragValue::new(&mut period).range(1..=500));
                config.period = period as usize;
            });
        }
        
        // MACD: fast/slow/signal periods
        if config.indicator_type == IndicatorType::MACD {
            ui.horizontal(|ui| {
                ui.label("快线周期:");
                let mut fast = config.fast_period as i32;
                ui.add(egui::DragValue::new(&mut fast).range(1..=200));
                config.fast_period = fast as usize;
            });
            ui.horizontal(|ui| {
                ui.label("慢线周期:");
                let mut slow = config.slow_period as i32;
                ui.add(egui::DragValue::new(&mut slow).range(1..=200));
                config.slow_period = slow as usize;
            });
            ui.horizontal(|ui| {
                ui.label("信号周期:");
                let mut signal = config.signal_period as i32;
                ui.add(egui::DragValue::new(&mut signal).range(1..=100));
                config.signal_period = signal as usize;
            });
        }
        
        // Multiplier parameter (for BOLL, SAR, SUPER)
        if matches!(config.indicator_type, IndicatorType::BOLL | IndicatorType::SAR | IndicatorType::SUPER) {
            ui.horizontal(|ui| {
                let label = match config.indicator_type {
                    IndicatorType::SAR => "加速因子:",
                    _ => "倍数:",
                };
                ui.label(label);
                ui.add(egui::DragValue::new(&mut config.multiplier).range(0.01..=10.0).speed(0.01));
            });
        }
        
        // Signal period (for TRIX, KDJ)
        if matches!(config.indicator_type, IndicatorType::TRIX | IndicatorType::KDJ) {
            ui.horizontal(|ui| {
                let label = match config.indicator_type {
                    IndicatorType::KDJ => "D周期:",
                    _ => "信号周期:",
                };
                ui.label(label);
                let mut signal_period = config.signal_period as i32;
                ui.add(egui::DragValue::new(&mut signal_period).range(1..=100));
                config.signal_period = signal_period as usize;
            });
        }
        
        // Color picker
        ui.horizontal(|ui| {
            ui.label("颜色:");
            ui.color_edit_button_srgba(&mut config.color);
        });
        
        // Signal color (for TRIX, MACD, KDJ)
        if matches!(config.indicator_type, IndicatorType::TRIX | IndicatorType::MACD | IndicatorType::KDJ) {
            ui.horizontal(|ui| {
                let label = match config.indicator_type {
                    IndicatorType::MACD => "信号线颜色:",
                    IndicatorType::KDJ => "D线颜色:",
                    _ => "信号线颜色:",
                };
                ui.label(label);
                ui.color_edit_button_srgba(&mut config.signal_color);
            });
        }
        
        // Histogram / J-line color (for MACD, KDJ)
        if matches!(config.indicator_type, IndicatorType::MACD | IndicatorType::KDJ) {
            ui.horizontal(|ui| {
                let label = match config.indicator_type {
                    IndicatorType::KDJ => "J线颜色:",
                    _ => "柱状图颜色:",
                };
                ui.label(label);
                ui.color_edit_button_srgba(&mut config.hist_color);
            });
        }
        
        // Line width
        ui.horizontal(|ui| {
            ui.label("线宽:");
            ui.add(egui::DragValue::new(&mut config.line_width).range(0.5..=5.0).speed(0.1));
        });
        
        // Location (for most indicators)
        if config.indicator_type != IndicatorType::VWAP && config.indicator_type != IndicatorType::AVL {
            ui.horizontal(|ui| {
                ui.label("显示位置:");
                let mut main = config.location == IndicatorLocation::Main;
                if ui.checkbox(&mut main, "主图").changed() {
                    config.location = if main {
                        IndicatorLocation::Main
                    } else {
                        IndicatorLocation::Sub
                    };
                }
            });
        }
        
        // Save changes button
        ui.horizontal(|ui| {
            if ui.button("保存配置").clicked() {
                if let Some(it) = self.selected_type {
                    self.available_indicators.insert(it, config.clone());
                    self.editing_config = Some(config);
                }
            }
            
            if ui.button("重置为默认").clicked() {
                if let Some(it) = self.selected_type {
                    let default_config = IndicatorConfigEntry::new(it);
                    self.available_indicators.insert(it, default_config.clone());
                    self.editing_config = Some(default_config);
                }
            }
        });
    }
    
    /// Show action buttons
    fn show_action_buttons(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            // Apply button
            let enabled_count = self.available_indicators.values().filter(|c| c.enabled).count();
            if ui.button(format!("应用到图表 ({})", enabled_count)).clicked() && enabled_count > 0 {
                let enabled: Vec<IndicatorConfigEntry> = self.available_indicators
                    .values()
                    .filter(|c| c.enabled)
                    .cloned()
                    .collect();
                self.pending_apply = Some(enabled);
            }
            
            // Clear all button
            if ui.button("清除所有").clicked() {
                for config in self.available_indicators.values_mut() {
                    config.enabled = false;
                }
                // Also trigger clear action
                self.pending_apply = Some(Vec::new());
            }
        });
    }
    
    /// Show presets section
    fn show_presets(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("指标预设").strong());
        
        ui.horizontal(|ui| {
            // Preset dropdown
            let preset_names: Vec<&str> = self.presets.iter().map(|p| p.name.as_str()).collect();
            egui::ComboBox::from_label("")
                .selected_text(self.selected_preset.and_then(|i| self.presets.get(i)).map(|p| p.name.as_str()).unwrap_or("选择预设"))
                .show_ui(ui, |ui| {
                    for (i, name) in preset_names.iter().enumerate() {
                        if ui.selectable_label(self.selected_preset == Some(i), *name).clicked() {
                            self.selected_preset = Some(i);
                        }
                    }
                });
            
            // Load preset
            if ui.button("加载").clicked() {
                if let Some(idx) = self.selected_preset {
                    if let Some(preset) = self.presets.get(idx) {
                        for entry in &preset.indicators {
                            self.available_indicators.insert(entry.indicator_type, entry.clone());
                        }
                    }
                }
            }
            
            // Save as preset
            if ui.button("保存为预设").clicked() {
                self.show_save_preset = true;
                self.new_preset_name = String::new();
            }
        });
        
        // Save preset dialog
        if self.show_save_preset {
            ui.horizontal(|ui| {
                ui.label("预设名称:");
                ui.text_edit_singleline(&mut self.new_preset_name);
                
                if ui.button("确定").clicked() && !self.new_preset_name.is_empty() {
                    let enabled: Vec<IndicatorConfigEntry> = self.available_indicators
                        .values()
                        .filter(|c| c.enabled)
                        .cloned()
                        .collect();
                    
                    let preset = IndicatorPreset {
                        name: self.new_preset_name.clone(),
                        indicators: enabled,
                    };
                    self.presets.push(preset);
                    self.show_save_preset = false;
                }
                
                if ui.button("取消").clicked() {
                    self.show_save_preset = false;
                }
            });
        }
        
        // Delete preset button
        if let Some(idx) = self.selected_preset {
            if idx < self.presets.len() {
                ui.horizontal(|ui| {
                    if ui.button("删除预设").clicked() {
                        self.presets.remove(idx);
                        self.selected_preset = None;
                    }
                });
            }
        }
    }
}

impl serde::Serialize for IndicatorConfigEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("IndicatorConfigEntry", 12)?;
        s.serialize_field("indicator_type", &format!("{:?}", self.indicator_type))?;
        s.serialize_field("period", &self.period)?;
        s.serialize_field("multiplier", &self.multiplier)?;
        s.serialize_field("signal_period", &self.signal_period)?;
        s.serialize_field("fast_period", &self.fast_period)?;
        s.serialize_field("slow_period", &self.slow_period)?;
        s.serialize_field("color", &format!("{:08x}", self.color.to_srgba_unmultiplied().iter().copied().fold(0u32, |acc, b| (acc << 8) | b as u32)))?;
        s.serialize_field("signal_color", &format!("{:08x}", self.signal_color.to_srgba_unmultiplied().iter().copied().fold(0u32, |acc, b| (acc << 8) | b as u32)))?;
        s.serialize_field("hist_color", &format!("{:08x}", self.hist_color.to_srgba_unmultiplied().iter().copied().fold(0u32, |acc, b| (acc << 8) | b as u32)))?;
        s.serialize_field("location", &format!("{:?}", self.location))?;
        s.serialize_field("line_width", &self.line_width)?;
        s.serialize_field("enabled", &self.enabled)?;
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for IndicatorConfigEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut result = IndicatorConfigEntry::default();
        let map = serde_json::Map::deserialize(deserializer)?;
        
        if let Some(v) = map.get("indicator_type").and_then(|v| v.as_str()) {
            result.indicator_type = match v {
                "MA" => IndicatorType::MA,
                "EMA" => IndicatorType::EMA,
                "WMA" => IndicatorType::WMA,
                "BOLL" => IndicatorType::BOLL,
                "VWAP" => IndicatorType::VWAP,
                "AVL" => IndicatorType::AVL,
                "TRIX" => IndicatorType::TRIX,
                "SAR" => IndicatorType::SAR,
                "SUPER" => IndicatorType::SUPER,
                "RSI" => IndicatorType::RSI,
                "MACD" => IndicatorType::MACD,
                "ATR" => IndicatorType::ATR,
                "KDJ" => IndicatorType::KDJ,
                "CCI" => IndicatorType::CCI,
                "MFI" => IndicatorType::MFI,
                _ => IndicatorType::MA,
            };
        }
        if let Some(v) = map.get("period").and_then(|v| v.as_u64()) {
            result.period = v as usize;
        }
        if let Some(v) = map.get("multiplier").and_then(|v| v.as_f64()) {
            result.multiplier = v;
        }
        if let Some(v) = map.get("signal_period").and_then(|v| v.as_u64()) {
            result.signal_period = v as usize;
        }
        if let Some(v) = map.get("fast_period").and_then(|v| v.as_u64()) {
            result.fast_period = v as usize;
        }
        if let Some(v) = map.get("slow_period").and_then(|v| v.as_u64()) {
            result.slow_period = v as usize;
        }
        if let Some(v) = map.get("line_width").and_then(|v| v.as_f64()) {
            result.line_width = v as f32;
        }
        if let Some(v) = map.get("enabled").and_then(|v| v.as_bool()) {
            result.enabled = v;
        }
        
        Ok(result)
    }
}
