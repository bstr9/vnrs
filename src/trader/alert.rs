//! Alert Engine for sending notifications on critical trading events.
//!
//! Provides configurable alert channels (log, webhook) for:
//! - Trade executions
//! - Order status changes (filled, cancelled, rejected)
//! - Risk rejections
//! - Connection state changes

use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use super::constant::Status;
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{LogData, OrderData, TradeData};

/// Alert severity level
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    /// Informational - trade filled, order submitted
    #[default]
    Info,
    /// Warning - risk rejection, connection issues
    Warning,
    /// Critical - risk breach, order rejected
    Critical,
}

impl std::fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertLevel::Info => write!(f, "INFO"),
            AlertLevel::Warning => write!(f, "WARNING"),
            AlertLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Alert message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertMessage {
    /// Alert level
    pub level: AlertLevel,
    /// Alert title (short summary)
    pub title: String,
    /// Alert body (detailed message)
    pub body: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Source engine/gateway
    pub source: String,
    /// Related vt_symbol (if applicable)
    pub vt_symbol: Option<String>,
    /// Related vt_orderid (if applicable)
    pub vt_orderid: Option<String>,
}

impl AlertMessage {
    /// Create a new alert message
    pub fn new(level: AlertLevel, title: impl Into<String>, body: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            body: body.into(),
            timestamp: Utc::now(),
            source: source.into(),
            vt_symbol: None,
            vt_orderid: None,
        }
    }

    /// Add vt_symbol
    pub fn with_symbol(mut self, vt_symbol: impl Into<String>) -> Self {
        self.vt_symbol = Some(vt_symbol.into());
        self
    }

    /// Add vt_orderid
    pub fn with_orderid(mut self, vt_orderid: impl Into<String>) -> Self {
        self.vt_orderid = Some(vt_orderid.into());
        self
    }

    /// Format for logging
    pub fn format_log(&self) -> String {
        let symbol = self.vt_symbol.as_deref().unwrap_or("-");
        let orderid = self.vt_orderid.as_deref().unwrap_or("-");
        format!(
            "[{}] {} | {} | {} | symbol={} | order={}",
            self.level, self.source, self.title, self.body, symbol, orderid
        )
    }
}

/// Alert channel trait - implement to add custom notification backends
pub trait AlertChannel: Send + Sync {
    /// Send an alert
    fn send(&self, alert: &AlertMessage);

    /// Get channel name
    fn channel_name(&self) -> &str;
}

/// Log-based alert channel (writes to tracing log)
pub struct LogAlertChannel {
    name: String,
}

impl LogAlertChannel {
    /// Create a new log alert channel
    pub fn new() -> Self {
        Self {
            name: "LogAlertChannel".to_string(),
        }
    }
}

impl Default for LogAlertChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertChannel for LogAlertChannel {
    fn send(&self, alert: &AlertMessage) {
        match alert.level {
            AlertLevel::Info => info!("{}", alert.format_log()),
            AlertLevel::Warning => warn!("{}", alert.format_log()),
            AlertLevel::Critical => error!("{}", alert.format_log()),
        }
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

/// Webhook alert channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL
    pub url: String,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Optional secret for signing (webhook-specific)
    pub secret: Option<String>,
    /// Minimum alert level to send (Info, Warning, Critical)
    #[serde(default)]
    pub min_level: AlertLevel,
}

fn default_timeout() -> u64 {
    10
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_secs: 10,
            secret: None,
            min_level: AlertLevel::Warning,
        }
    }
}

/// Webhook-based alert channel (HTTP POST)
pub struct WebhookAlertChannel {
    name: String,
    config: WebhookConfig,
    client: Client,
    enabled: AtomicBool,
}

impl WebhookAlertChannel {
    /// Create a new webhook alert channel
    pub fn new(config: WebhookConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            name: "WebhookAlertChannel".to_string(),
            config,
            client,
            enabled: AtomicBool::new(true),
        }
    }

    /// Check if alert level meets minimum threshold
    fn should_send(&self, level: AlertLevel) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        // Compare levels: Critical > Warning > Info
        match (self.config.min_level, level) {
            (AlertLevel::Critical, AlertLevel::Critical) => true,
            (AlertLevel::Critical, _) => false,
            (AlertLevel::Warning, AlertLevel::Critical | AlertLevel::Warning) => true,
            (AlertLevel::Warning, AlertLevel::Info) => false,
            (AlertLevel::Info, _) => true,
        }
    }

    /// Enable the channel
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    /// Disable the channel
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }
}

impl AlertChannel for WebhookAlertChannel {
    fn send(&self, alert: &AlertMessage) {
        if !self.should_send(alert.level) {
            return;
        }

        let client = self.client.clone();
        let url = self.config.url.clone();
        let alert_json = serde_json::to_string(alert).unwrap_or_default();

        // Fire and forget - don't block on webhook
        tokio::spawn(async move {
            let result = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(alert_json)
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {}
                Ok(resp) => {
                    warn!("[WebhookAlertChannel] Webhook returned status: {}", resp.status());
                }
                Err(e) => {
                    warn!("[WebhookAlertChannel] Webhook request failed: {}", e);
                }
            }
        });
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable trade execution alerts
    #[serde(default = "default_true")]
    pub alert_on_trade: bool,
    /// Enable order filled alerts
    #[serde(default = "default_true")]
    pub alert_on_order_filled: bool,
    /// Enable order cancelled alerts
    #[serde(default = "default_true")]
    pub alert_on_order_cancelled: bool,
    /// Enable order rejected alerts
    #[serde(default = "default_true")]
    pub alert_on_order_rejected: bool,
    /// Enable risk rejection alerts
    #[serde(default = "default_true")]
    pub alert_on_risk_reject: bool,
    /// Enable connection state alerts
    #[serde(default = "default_true")]
    pub alert_on_connection: bool,
    /// Minimum level for alerts
    #[serde(default)]
    pub min_level: AlertLevel,
}

fn default_true() -> bool {
    true
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            alert_on_trade: true,
            alert_on_order_filled: true,
            alert_on_order_cancelled: true,
            alert_on_order_rejected: true,
            alert_on_risk_reject: true,
            alert_on_connection: true,
            min_level: AlertLevel::Info,
        }
    }
}

/// Alert Engine - sends notifications on critical events
pub struct AlertEngine {
    name: String,
    config: RwLock<AlertConfig>,
    channels: RwLock<Vec<Arc<dyn AlertChannel>>>,
    running: AtomicBool,
    enabled: AtomicBool,
}

impl AlertEngine {
    /// Create a new AlertEngine with default configuration
    pub fn new() -> Self {
        Self::with_config(AlertConfig::default())
    }

    /// Create a new AlertEngine with custom configuration
    pub fn with_config(config: AlertConfig) -> Self {
        let engine = Self {
            name: "AlertEngine".to_string(),
            config: RwLock::new(config),
            channels: RwLock::new(vec![Arc::new(LogAlertChannel::new())]),
            running: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        };
        engine
    }

    /// Add an alert channel
    pub fn add_channel(&self, channel: Arc<dyn AlertChannel>) {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());
        channels.push(channel);
    }

    /// Remove all channels
    pub fn clear_channels(&self) {
        let mut channels = self.channels.write().unwrap_or_else(|e| e.into_inner());
        channels.clear();
    }

    /// Get number of registered channels
    pub fn channel_count(&self) -> usize {
        self.channels.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Enable alerts
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
        info!("[AlertEngine] Alerts enabled");
    }

    /// Disable alerts
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        warn!("[AlertEngine] Alerts DISABLED");
    }

    /// Check if alerts are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Update configuration
    pub fn update_config(&self, config: AlertConfig) {
        let mut current = self.config.write().unwrap_or_else(|e| e.into_inner());
        *current = config;
        info!("[AlertEngine] Configuration updated");
    }

    /// Get current configuration
    pub fn get_config(&self) -> AlertConfig {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Send an alert through all registered channels
    pub fn send_alert(&self, alert: AlertMessage) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let channels = self.channels.read().unwrap_or_else(|e| e.into_inner());
        for channel in channels.iter() {
            channel.send(&alert);
        }
    }

    /// Create alert for trade execution
    pub fn alert_trade(&self, trade: &TradeData) {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        if !config.alert_on_trade {
            return;
        }

        let direction = trade.direction.map(|d| d.to_string()).unwrap_or_else(|| "-".to_string());
        let alert = AlertMessage::new(
            AlertLevel::Info,
            "Trade Executed",
            format!(
                "{} {} @ {} (vol={})",
                direction, trade.symbol, trade.price, trade.volume
            ),
            trade.gateway_name.clone(),
        )
        .with_symbol(format!("{}.{}", trade.symbol, trade.exchange.value()))
        .with_orderid(trade.vt_tradeid());

        self.send_alert(alert);
    }

    /// Create alert for order status change
    pub fn alert_order(&self, order: &OrderData) {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());

        let (level, title, body) = match order.status {
            Status::AllTraded if config.alert_on_order_filled => {
                (
                    AlertLevel::Info,
                    "Order Filled",
                    format!(
                        "Order {} fully filled: {} {} @ {}",
                        order.vt_orderid(),
                        order.direction.map(|d| d.to_string()).unwrap_or("-".to_string()),
                        order.volume,
                        order.price
                    ),
                )
            }
            Status::PartTraded if config.alert_on_order_filled => {
                (
                    AlertLevel::Info,
                    "Order Partial Fill",
                    format!(
                        "Order {} partially filled: {}/{} @ {}",
                        order.vt_orderid(),
                        order.traded,
                        order.volume,
                        order.price
                    ),
                )
            }
            Status::Cancelled if config.alert_on_order_cancelled => {
                (
                    AlertLevel::Warning,
                    "Order Cancelled",
                    format!("Order {} cancelled", order.vt_orderid()),
                )
            }
            Status::Rejected if config.alert_on_order_rejected => {
                (
                    AlertLevel::Critical,
                    "Order Rejected",
                    format!("Order {} rejected", order.vt_orderid()),
                )
            }
            _ => return, // Don't alert for other statuses
        };

        let alert = AlertMessage::new(level, title, body, order.gateway_name.clone())
            .with_symbol(order.vt_symbol())
            .with_orderid(order.vt_orderid());

        self.send_alert(alert);
    }

    /// Create alert for risk rejection
    pub fn alert_risk_reject(&self, reason: &str, symbol: Option<&str>, gateway_name: &str) {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        if !config.alert_on_risk_reject {
            return;
        }

        let mut alert = AlertMessage::new(
            AlertLevel::Critical,
            "Risk Rejection",
            reason.to_string(),
            gateway_name,
        );

        if let Some(s) = symbol {
            alert = alert.with_symbol(s);
        }

        self.send_alert(alert);
    }

    /// Create alert for connection state change
    pub fn alert_connection(&self, gateway_name: &str, connected: bool, reason: Option<&str>) {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        if !config.alert_on_connection {
            return;
        }

        let (level, title, body) = if connected {
            (
                AlertLevel::Info,
                "Connection Restored",
                format!("Gateway {} connected", gateway_name),
            )
        } else {
            (
                AlertLevel::Critical,
                "Connection Lost",
                format!(
                    "Gateway {} disconnected{}",
                    gateway_name,
                    reason.map(|r| format!(": {}", r)).unwrap_or_default()
                ),
            )
        };

        let alert = AlertMessage::new(level, title, body, gateway_name);
        self.send_alert(alert);
    }

    /// Process a log event (for connection alerts from gateway logs)
    pub fn process_log(&self, log: &LogData) {
        // Detect connection-related log messages
        let msg = log.msg.to_lowercase();
        let is_disconnect = msg.contains("disconnect") || msg.contains("连接断开");
        let is_connect = msg.contains("connect") && !is_disconnect;

        if is_disconnect || is_connect {
            self.alert_connection(
                &log.gateway_name,
                is_connect,
                if is_disconnect { Some(&log.msg) } else { None },
            );
        }
    }
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for AlertEngine {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Trade(trade) => self.alert_trade(trade),
            GatewayEvent::Order(order) => self.alert_order(order),
            GatewayEvent::Log(log) => self.process_log(log),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Direction, Exchange, OrderType};

    fn make_trade() -> TradeData {
        TradeData {
            symbol: "btcusdt".to_string(),
            exchange: Exchange::Binance,
            orderid: "order1".to_string(),
            tradeid: "trade1".to_string(),
            direction: Some(Direction::Long),
            offset: crate::trader::constant::Offset::None,
            price: 50000.0,
            volume: 0.1,
            datetime: Some(Utc::now()),
            gateway_name: "BINANCE_SPOT".to_string(),
            extra: None,
        }
    }

    fn make_order(status: Status) -> OrderData {
        OrderData {
            symbol: "btcusdt".to_string(),
            exchange: Exchange::Binance,
            orderid: "order1".to_string(),
            direction: Some(Direction::Long),
            offset: crate::trader::constant::Offset::None,
            order_type: OrderType::Limit,
            price: 50000.0,
            volume: 0.1,
            traded: if status == Status::AllTraded { 0.1 } else { 0.0 },
            status,
            datetime: Some(Utc::now()),
            gateway_name: "BINANCE_SPOT".to_string(),
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            extra: None,
        }
    }

    #[test]
    fn test_alert_message_format() {
        let alert = AlertMessage::new(
            AlertLevel::Warning,
            "Test Alert",
            "This is a test",
            "TestEngine",
        )
        .with_symbol("BTCUSDT.BINANCE")
        .with_orderid("order123");

        let formatted = alert.format_log();
        assert!(formatted.contains("WARNING"));
        assert!(formatted.contains("Test Alert"));
        assert!(formatted.contains("BTCUSDT.BINANCE"));
        assert!(formatted.contains("order123"));
    }

    #[test]
    fn test_alert_engine_trade() {
        let engine = AlertEngine::new();
        let trade = make_trade();
        // Should not panic
        engine.alert_trade(&trade);
    }

    #[test]
    fn test_alert_engine_order_filled() {
        let engine = AlertEngine::new();
        let order = make_order(Status::AllTraded);
        engine.alert_order(&order);
    }

    #[test]
    fn test_alert_engine_order_cancelled() {
        let engine = AlertEngine::new();
        let order = make_order(Status::Cancelled);
        engine.alert_order(&order);
    }

    #[test]
    fn test_alert_engine_order_rejected() {
        let engine = AlertEngine::new();
        let order = make_order(Status::Rejected);
        engine.alert_order(&order);
    }

    #[test]
    fn test_alert_engine_disabled() {
        let engine = AlertEngine::new();
        engine.disable();
        assert!(!engine.is_enabled());

        let trade = make_trade();
        engine.alert_trade(&trade); // Should be silently ignored

        engine.enable();
        assert!(engine.is_enabled());
    }

    #[test]
    fn test_alert_config_disabled() {
        let mut config = AlertConfig::default();
        config.alert_on_trade = false;
        let engine = AlertEngine::with_config(config);

        let trade = make_trade();
        engine.alert_trade(&trade); // Should be silently ignored due to config
    }

    #[test]
    fn test_log_alert_channel() {
        let channel = LogAlertChannel::new();
        let alert = AlertMessage::new(AlertLevel::Info, "Test", "Body", "Test");
        channel.send(&alert); // Should not panic
    }

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.min_level, AlertLevel::Warning);
    }

    #[test]
    fn test_alert_engine_add_channel() {
        let engine = AlertEngine::new();
        assert_eq!(engine.channel_count(), 1); // LogAlertChannel by default

        engine.add_channel(Arc::new(LogAlertChannel::new()));
        assert_eq!(engine.channel_count(), 2);

        engine.clear_channels();
        assert_eq!(engine.channel_count(), 0);
    }

    #[test]
    fn test_alert_risk_reject() {
        let engine = AlertEngine::new();
        engine.alert_risk_reject("Order exceeds daily limit", Some("BTCUSDT.BINANCE"), "BINANCE_SPOT");
    }

    #[test]
    fn test_alert_connection() {
        let engine = AlertEngine::new();
        engine.alert_connection("BINANCE_SPOT", false, Some("WebSocket closed"));
        engine.alert_connection("BINANCE_SPOT", true, None);
    }
}
