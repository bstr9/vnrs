//! Global setting of the trading platform.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use std::sync::{LazyLock, RwLock};

use super::utility::get_file_path;

/// Default settings
fn default_settings() -> HashMap<String, SettingValue> {
    let mut settings = HashMap::new();
    
    // Font settings
    settings.insert("font.family".to_string(), SettingValue::String("微软雅黑".to_string()));
    settings.insert("font.size".to_string(), SettingValue::Int(12));
    
    // Log settings
    settings.insert("log.active".to_string(), SettingValue::Bool(true));
    settings.insert("log.level".to_string(), SettingValue::Int(20)); // INFO level
    settings.insert("log.console".to_string(), SettingValue::Bool(true));
    settings.insert("log.file".to_string(), SettingValue::Bool(true));
    
    // Email settings
    settings.insert("email.server".to_string(), SettingValue::String("smtp.qq.com".to_string()));
    settings.insert("email.port".to_string(), SettingValue::Int(465));
    settings.insert("email.username".to_string(), SettingValue::String(String::new()));
    settings.insert("email.password".to_string(), SettingValue::String(String::new()));
    settings.insert("email.sender".to_string(), SettingValue::String(String::new()));
    settings.insert("email.receiver".to_string(), SettingValue::String(String::new()));
    
    // Datafeed settings
    settings.insert("datafeed.name".to_string(), SettingValue::String(String::new()));
    settings.insert("datafeed.username".to_string(), SettingValue::String(String::new()));
    settings.insert("datafeed.password".to_string(), SettingValue::String(String::new()));
    
    // Database settings
    settings.insert("database.timezone".to_string(), SettingValue::String("UTC".to_string()));
    settings.insert("database.name".to_string(), SettingValue::String("sqlite".to_string()));
    settings.insert("database.database".to_string(), SettingValue::String("database.db".to_string()));
    settings.insert("database.host".to_string(), SettingValue::String(String::new()));
    settings.insert("database.port".to_string(), SettingValue::Int(0));
    settings.insert("database.user".to_string(), SettingValue::String(String::new()));
    settings.insert("database.password".to_string(), SettingValue::String(String::new()));

    // General settings
    settings.insert("language".to_string(), SettingValue::String("zh_CN".to_string()));
    
    settings
}

/// Setting value types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl SettingValue {
    /// Get as string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            SettingValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as i64
    pub fn as_int(&self) -> Option<i64> {
        match self {
            SettingValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as f64
    pub fn as_float(&self) -> Option<f64> {
        match self {
            SettingValue::Float(f) => Some(*f),
            SettingValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SettingValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Global settings container
pub struct Settings {
    settings: RwLock<HashMap<String, SettingValue>>,
}

impl Settings {
    /// Create new Settings with defaults
    pub fn new() -> Self {
        let mut settings = default_settings();
        
        // Try to load from file
        if let Some(file_settings) = load_settings_from_file() {
            for (key, value) in file_settings {
                settings.insert(key, value);
            }
        }
        
        Self {
            settings: RwLock::new(settings),
        }
    }

    /// Get a setting value
    pub fn get(&self, key: &str) -> Option<SettingValue> {
        self.settings.read().ok()?.get(key).cloned()
    }

    /// Get a string setting
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get an integer setting
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_int())
    }

    /// Get a float setting
    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_float())
    }

    /// Get a bool setting
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    /// Set a setting value
    pub fn set(&self, key: impl Into<String>, value: SettingValue) {
        if let Ok(mut settings) = self.settings.write() {
            settings.insert(key.into(), value);
        }
    }

    /// Update settings from a map
    pub fn update(&self, new_settings: HashMap<String, SettingValue>) {
        if let Ok(mut settings) = self.settings.write() {
            for (key, value) in new_settings {
                settings.insert(key, value);
            }
        }
    }

    /// Get all settings as HashMap
    pub fn get_all(&self) -> HashMap<String, SettingValue> {
        self.settings.read()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    /// Save settings to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let filepath = get_file_path(SETTING_FILENAME);
        let settings = self.settings.read().map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(&*settings)?;
        fs::write(filepath, json)?;
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

/// Setting filename
const SETTING_FILENAME: &str = "engine_setting.json";

/// Load settings from JSON file
fn load_settings_from_file() -> Option<HashMap<String, SettingValue>> {
    let filepath = get_file_path(SETTING_FILENAME);
    if filepath.exists() {
        let content = fs::read_to_string(filepath).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

/// Global settings instance
pub static SETTINGS: LazyLock<Settings> = LazyLock::new(Settings::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setting_value_types() {
        let s = SettingValue::String("test".to_string());
        assert_eq!(s.as_str(), Some("test"));
        
        let i = SettingValue::Int(42);
        assert_eq!(i.as_int(), Some(42));
        
        let f = SettingValue::Float(3.14);
        assert_eq!(f.as_float(), Some(3.14));
        
        let b = SettingValue::Bool(true);
        assert_eq!(b.as_bool(), Some(true));
    }

    #[test]
    fn test_default_settings() {
        let settings = Settings::new();
        assert!(settings.get_bool("log.active").unwrap_or(false));
        assert_eq!(settings.get_int("font.size"), Some(12));
    }
}
