//! Binance gateway configuration persistence module.
//!
//! Handles saving and loading gateway configurations to/from disk.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json;
use tracing::{error, info};

use crate::trader::{GatewaySettings, GatewaySettingValue};
use crate::trader::utility::get_folder_path;

/// Configuration for a single Binance gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceGatewayConfig {
    /// API key
    #[serde(default)]
    pub key: String,
    /// API secret
    #[serde(default)]
    pub secret: String,
    /// Server mode (REAL or TESTNET)
    #[serde(default = "default_server")]
    pub server: String,
    /// Proxy host (can be empty)
    #[serde(default)]
    pub proxy_host: String,
    /// Proxy port (0 means no proxy)
    #[serde(default)]
    pub proxy_port: u16,
}

fn default_server() -> String {
    "REAL".to_string()
}

impl Default for BinanceGatewayConfig {
    fn default() -> Self {
        Self {
            key: String::new(),
            secret: String::new(),
            server: "REAL".to_string(),
            proxy_host: String::new(),
            proxy_port: 0,
        }
    }
}

impl BinanceGatewayConfig {
    /// Create from GatewaySettings
    pub fn from_settings(settings: &GatewaySettings) -> Self {
        let key = match settings.get("key") {
            Some(GatewaySettingValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let secret = match settings.get("secret") {
            Some(GatewaySettingValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let server = match settings.get("server") {
            Some(GatewaySettingValue::String(s)) => s.clone(),
            _ => "REAL".to_string(),
        };
        let proxy_host = match settings.get("proxy_host") {
            Some(GatewaySettingValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let proxy_port = match settings.get("proxy_port") {
            Some(GatewaySettingValue::Int(p)) => *p as u16,
            _ => 0,
        };

        Self {
            key,
            secret,
            server,
            proxy_host,
            proxy_port,
        }
    }

    /// Convert to GatewaySettings
    pub fn to_settings(&self) -> GatewaySettings {
        let mut settings = GatewaySettings::new();
        settings.insert("key".to_string(), GatewaySettingValue::String(self.key.clone()));
        settings.insert("secret".to_string(), GatewaySettingValue::String(self.secret.clone()));
        settings.insert("server".to_string(), GatewaySettingValue::String(self.server.clone()));
        settings.insert("proxy_host".to_string(), GatewaySettingValue::String(self.proxy_host.clone()));
        settings.insert("proxy_port".to_string(), GatewaySettingValue::Int(self.proxy_port as i64));
        settings
    }
}

/// All Binance gateway configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BinanceConfigs {
    /// Map of gateway_name -> config
    #[serde(default)]
    pub gateways: HashMap<String, BinanceGatewayConfig>,
}

impl BinanceConfigs {
    /// Load configurations from disk
    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(configs) => {
                        info!("Loaded Binance gateway configurations from {:?}", config_path);
                        return configs;
                    }
                    Err(e) => {
                        error!("Failed to parse Binance config file: {}", e);
                    }
                },
                Err(e) => {
                    error!("Failed to read Binance config file: {}", e);
                }
            }
        }
        
        // Return default empty config
        Self::default()
    }

    /// Save configurations to disk
    pub fn save(&self) -> Result<(), String> {
        let config_path = Self::get_config_path();
        
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                // Ensure parent directory exists
                if let Some(parent) = config_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        return Err(format!("Failed to create config directory: {}", e));
                    }
                }
                
                match fs::write(&config_path, json) {
                    Ok(_) => {
                        info!("Saved Binance gateway configurations to {:?}", config_path);
                        Ok(())
                    }
                    Err(e) => Err(format!("Failed to write config file: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to serialize config: {}", e)),
        }
    }

    /// Get config for a specific gateway
    pub fn get(&self, gateway_name: &str) -> Option<&BinanceGatewayConfig> {
        self.gateways.get(gateway_name)
    }

    /// Set config for a specific gateway
    pub fn set(&mut self, gateway_name: String, config: BinanceGatewayConfig) {
        self.gateways.insert(gateway_name, config);
    }

    /// Remove config for a specific gateway
    pub fn remove(&mut self, gateway_name: &str) -> Option<BinanceGatewayConfig> {
        self.gateways.remove(gateway_name)
    }

    /// Get the config file path
    fn get_config_path() -> PathBuf {
        get_folder_path("binance").join("gateway_configs.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = BinanceGatewayConfig {
            key: "test_key".to_string(),
            secret: "test_secret".to_string(),
            server: "REAL".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 1080,
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: BinanceGatewayConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.key, deserialized.key);
        assert_eq!(config.secret, deserialized.secret);
        assert_eq!(config.server, deserialized.server);
        assert_eq!(config.proxy_host, deserialized.proxy_host);
        assert_eq!(config.proxy_port, deserialized.proxy_port);
    }

    #[test]
    fn test_config_to_from_settings() {
        let config = BinanceGatewayConfig {
            key: "test_key".to_string(),
            secret: "test_secret".to_string(),
            server: "TESTNET".to_string(),
            proxy_host: "socks5://127.0.0.1".to_string(),
            proxy_port: 1080,
        };

        let settings = config.to_settings();
        let config2 = BinanceGatewayConfig::from_settings(&settings);

        assert_eq!(config.key, config2.key);
        assert_eq!(config.secret, config2.secret);
        assert_eq!(config.server, config2.server);
        assert_eq!(config.proxy_host, config2.proxy_host);
        assert_eq!(config.proxy_port, config2.proxy_port);
    }

    #[test]
    fn test_configs_operations() {
        let mut configs = BinanceConfigs::default();
        
        let config1 = BinanceGatewayConfig {
            key: "key1".to_string(),
            secret: "secret1".to_string(),
            server: "REAL".to_string(),
            proxy_host: String::new(),
            proxy_port: 0,
        };

        configs.set("BINANCE_SPOT".to_string(), config1.clone());
        
        let retrieved = configs.get("BINANCE_SPOT").unwrap();
        assert_eq!(retrieved.key, "key1");
        
        configs.remove("BINANCE_SPOT");
        assert!(configs.get("BINANCE_SPOT").is_none());
    }
}
