//! Dialog windows for the trading platform.

use std::collections::HashMap;
use egui::{Window, Context};

/// Connection dialog for gateway configuration
pub struct ConnectDialog {
    pub gateway_name: String,
    pub settings: HashMap<String, SettingField>,
    pub is_open: bool,
    pub should_connect: bool,
}

/// Setting field with type info
#[derive(Clone)]
pub struct SettingField {
    pub value: String,
    pub field_type: FieldType,
    pub options: Vec<String>,
}

#[derive(Clone, PartialEq)]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Select,
    Password,
}

impl ConnectDialog {
    pub fn new(gateway_name: &str) -> Self {
        Self {
            gateway_name: gateway_name.to_string(),
            settings: HashMap::new(),
            is_open: false,
            should_connect: false,
        }
    }
    
    /// Set default settings from gateway
    pub fn set_default_settings(&mut self, defaults: HashMap<String, serde_json::Value>) {
        self.settings.clear();
        
        for (key, value) in defaults {
            let field = match value {
                serde_json::Value::String(s) => {
                    let field_type = if key.contains("密码") || key.contains("password") {
                        FieldType::Password
                    } else {
                        FieldType::String
                    };
                    SettingField {
                        value: s,
                        field_type,
                        options: Vec::new(),
                    }
                }
                serde_json::Value::Number(n) => {
                    let field_type = if n.is_i64() {
                        FieldType::Int
                    } else {
                        FieldType::Float
                    };
                    SettingField {
                        value: n.to_string(),
                        field_type,
                        options: Vec::new(),
                    }
                }
                serde_json::Value::Bool(b) => SettingField {
                    value: b.to_string(),
                    field_type: FieldType::Bool,
                    options: Vec::new(),
                },
                serde_json::Value::Array(arr) => {
                    let options: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    SettingField {
                        value: options.first().cloned().unwrap_or_default(),
                        field_type: FieldType::Select,
                        options,
                    }
                }
                _ => continue,
            };
            self.settings.insert(key, field);
        }
    }
    
    /// Load saved settings
    pub fn load_settings(&mut self, saved: HashMap<String, serde_json::Value>) {
        for (key, value) in saved {
            if let Some(field) = self.settings.get_mut(&key) {
                field.value = match value {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
            }
        }
    }
    
    /// Show the dialog
    pub fn show(&mut self, ctx: &Context) {
        if !self.is_open {
            return;
        }
        
        let title = format!("连接 {}", self.gateway_name);
        
        Window::new(title)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("connect_settings")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        let mut keys: Vec<_> = self.settings.keys().cloned().collect();
                        keys.sort();
                        
                        for key in keys {
                            if let Some(field) = self.settings.get_mut(&key) {
                                ui.label(&key);
                                
                                match field.field_type {
                                    FieldType::String | FieldType::Int | FieldType::Float => {
                                        ui.text_edit_singleline(&mut field.value);
                                    }
                                    FieldType::Password => {
                                        ui.add(egui::TextEdit::singleline(&mut field.value).password(true));
                                    }
                                    FieldType::Bool => {
                                        let mut checked = field.value == "true";
                                        if ui.checkbox(&mut checked, "").changed() {
                                            field.value = checked.to_string();
                                        }
                                    }
                                    FieldType::Select => {
                                        egui::ComboBox::from_id_salt(&key)
                                            .selected_text(&field.value)
                                            .show_ui(ui, |ui| {
                                                for option in &field.options {
                                                    ui.selectable_value(&mut field.value, option.clone(), option);
                                                }
                                            });
                                    }
                                }
                                ui.end_row();
                            }
                        }
                    });
                
                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("连接").clicked() {
                        self.should_connect = true;
                        self.is_open = false;
                    }
                    if ui.button("取消").clicked() {
                        self.is_open = false;
                    }
                });
            });
    }
    
    /// Get settings as JSON-compatible HashMap
    pub fn get_settings(&self) -> HashMap<String, serde_json::Value> {
        let mut result = HashMap::new();
        
        for (key, field) in &self.settings {
            let value = match field.field_type {
                FieldType::Int => {
                    if let Ok(n) = field.value.parse::<i64>() {
                        serde_json::Value::Number(n.into())
                    } else {
                        serde_json::Value::String(field.value.clone())
                    }
                }
                FieldType::Float => {
                    if let Ok(n) = field.value.parse::<f64>() {
                        serde_json::json!(n)
                    } else {
                        serde_json::Value::String(field.value.clone())
                    }
                }
                FieldType::Bool => {
                    serde_json::Value::Bool(field.value == "true")
                }
                _ => serde_json::Value::String(field.value.clone()),
            };
            result.insert(key.clone(), value);
        }
        
        result
    }
    
    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
        self.should_connect = false;
    }
    
    /// Check if should connect and reset flag
    pub fn take_connect(&mut self) -> bool {
        let result = self.should_connect;
        self.should_connect = false;
        result
    }
}

/// About dialog showing version information
pub struct AboutDialog {
    pub is_open: bool,
}

impl Default for AboutDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AboutDialog {
    pub fn new() -> Self {
        Self { is_open: false }
    }
    
    pub fn open(&mut self) {
        self.is_open = true;
    }
    
    pub fn show(&mut self, ctx: &Context) {
        if !self.is_open {
            return;
        }
        
        Window::new("关于 Trade Engine")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Trade Engine");
                    ui.add_space(10.0);
                    
                    ui.label("By Traders, For Traders.");
                    ui.add_space(10.0);
                    
                    ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
                    ui.label(format!("Rust: {}", rustc_version_runtime::version()));
                    ui.add_space(10.0);
                    
                    ui.label("License: MIT");
                    ui.add_space(20.0);
                    
                    if ui.button("关闭").clicked() {
                        self.is_open = false;
                    }
                });
            });
    }
}

/// Global settings dialog
pub struct GlobalSettingsDialog {
    pub is_open: bool,
    pub settings: HashMap<String, String>,
    pub should_save: bool,
}

impl Default for GlobalSettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalSettingsDialog {
    pub fn new() -> Self {
        Self {
            is_open: false,
            settings: HashMap::new(),
            should_save: false,
        }
    }
    
    pub fn open(&mut self, settings: HashMap<String, serde_json::Value>) {
        self.is_open = true;
        self.should_save = false;
        self.settings = settings
            .into_iter()
            .map(|(k, v)| {
                let s = match v {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => v.to_string(),
                };
                (k, s)
            })
            .collect();
    }
    
    pub fn show(&mut self, ctx: &Context) {
        if !self.is_open {
            return;
        }
        
        Window::new("全局配置")
            .collapsible(false)
            .resizable(true)
            .min_width(600.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        egui::Grid::new("global_settings")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                let mut keys: Vec<_> = self.settings.keys().cloned().collect();
                                keys.sort();
                                
                                for key in keys {
                                    if let Some(value) = self.settings.get_mut(&key) {
                                        ui.label(&key);
                                        ui.text_edit_singleline(value);
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                
                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("确定").clicked() {
                        self.should_save = true;
                        self.is_open = false;
                    }
                    if ui.button("取消").clicked() {
                        self.is_open = false;
                    }
                });
                
                ui.label("注意：配置修改需要重启后生效");
            });
    }
    
    /// Get settings as JSON-compatible HashMap
    pub fn get_settings(&self) -> HashMap<String, serde_json::Value> {
        self.settings
            .iter()
            .map(|(k, v)| {
                // Try to parse as number or bool
                let value = if let Ok(n) = v.parse::<i64>() {
                    serde_json::Value::Number(n.into())
                } else if let Ok(n) = v.parse::<f64>() {
                    serde_json::json!(n)
                } else if v == "true" {
                    serde_json::Value::Bool(true)
                } else if v == "false" {
                    serde_json::Value::Bool(false)
                } else {
                    serde_json::Value::String(v.clone())
                };
                (k.clone(), value)
            })
            .collect()
    }
    
    /// Check if should save and reset flag
    pub fn take_save(&mut self) -> bool {
        let result = self.should_save;
        self.should_save = false;
        result
    }
}

/// Contract manager dialog for querying contracts
pub struct ContractManagerDialog {
    pub is_open: bool,
    pub filter: String,
    pub contracts: Vec<ContractRow>,
    pub filtered_contracts: Vec<usize>,
}

/// Contract row for display
#[derive(Clone)]
pub struct ContractRow {
    pub vt_symbol: String,
    pub symbol: String,
    pub exchange: String,
    pub name: String,
    pub product: String,
    pub size: f64,
    pub pricetick: f64,
    pub min_volume: f64,
    pub gateway_name: String,
}

impl Default for ContractManagerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ContractManagerDialog {
    pub fn new() -> Self {
        Self {
            is_open: false,
            filter: String::new(),
            contracts: Vec::new(),
            filtered_contracts: Vec::new(),
        }
    }
    
    pub fn open(&mut self) {
        self.is_open = true;
        self.filter.clear();
        self.update_filter();
    }
    
    pub fn set_contracts(&mut self, contracts: Vec<ContractRow>) {
        self.contracts = contracts;
        self.update_filter();
    }
    
    fn update_filter(&mut self) {
        let filter_lower = self.filter.to_lowercase();
        self.filtered_contracts = self.contracts
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                self.filter.is_empty() 
                    || c.vt_symbol.to_lowercase().contains(&filter_lower)
                    || c.name.to_lowercase().contains(&filter_lower)
            })
            .map(|(i, _)| i)
            .collect();
    }
    
    pub fn show(&mut self, ctx: &Context) {
        if !self.is_open {
            return;
        }
        
        Window::new("合约查询")
            .collapsible(true)
            .resizable(true)
            .min_width(900.0)
            .min_height(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("过滤:");
                    if ui.text_edit_singleline(&mut self.filter).changed() {
                        self.update_filter();
                    }
                    ui.label(format!("共 {} 条", self.filtered_contracts.len()));
                });
                
                ui.separator();
                
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui_extras::TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .column(egui_extras::Column::auto().at_least(120.0)) // vt_symbol
                            .column(egui_extras::Column::auto().at_least(80.0))  // symbol
                            .column(egui_extras::Column::auto().at_least(80.0))  // exchange
                            .column(egui_extras::Column::auto().at_least(100.0)) // name
                            .column(egui_extras::Column::auto().at_least(60.0))  // product
                            .column(egui_extras::Column::auto().at_least(60.0))  // size
                            .column(egui_extras::Column::auto().at_least(80.0))  // pricetick
                            .column(egui_extras::Column::auto().at_least(60.0))  // min_volume
                            .column(egui_extras::Column::auto().at_least(80.0))  // gateway
                            .header(20.0, |mut header| {
                                header.col(|ui| { ui.strong("本地代码"); });
                                header.col(|ui| { ui.strong("代码"); });
                                header.col(|ui| { ui.strong("交易所"); });
                                header.col(|ui| { ui.strong("名称"); });
                                header.col(|ui| { ui.strong("类型"); });
                                header.col(|ui| { ui.strong("乘数"); });
                                header.col(|ui| { ui.strong("价格跳动"); });
                                header.col(|ui| { ui.strong("最小量"); });
                                header.col(|ui| { ui.strong("接口"); });
                            })
                            .body(|mut body| {
                                for &idx in &self.filtered_contracts {
                                    if let Some(row) = self.contracts.get(idx) {
                                        body.row(18.0, |mut table_row| {
                                            table_row.col(|ui| { ui.label(&row.vt_symbol); });
                                            table_row.col(|ui| { ui.label(&row.symbol); });
                                            table_row.col(|ui| { ui.label(&row.exchange); });
                                            table_row.col(|ui| { ui.label(&row.name); });
                                            table_row.col(|ui| { ui.label(&row.product); });
                                            table_row.col(|ui| { ui.label(format!("{:.0}", row.size)); });
                                            table_row.col(|ui| { ui.label(format!("{}", row.pricetick)); });
                                            table_row.col(|ui| { ui.label(format!("{:.0}", row.min_volume)); });
                                            table_row.col(|ui| { ui.label(&row.gateway_name); });
                                        });
                                    }
                                }
                            });
                    });
                
                ui.separator();
                
                if ui.button("关闭").clicked() {
                    self.is_open = false;
                }
            });
    }
}
