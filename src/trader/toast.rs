//! Toast notification manager for UI alert display.
//!
//! Receives EVENT_ALERT events via the BaseEngine process_event dispatch,
//! maintains a history of recent alerts, and provides a queue of active
//! (undismissed) toasts for the UI to display.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::alert::{AlertLevel, AlertMessage};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;

/// Maximum number of toasts to keep in history
const MAX_TOAST_HISTORY: usize = 100;

/// A toast notification with display metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toast {
    /// Unique toast ID
    pub id: u64,
    /// Alert severity level
    pub level: AlertLevel,
    /// Short title/summary
    pub title: String,
    /// Detailed message body
    pub body: String,
    /// Source engine/gateway that generated the alert
    pub source: String,
    /// When the alert was created
    pub timestamp: DateTime<Utc>,
    /// Related trading symbol (if applicable)
    pub vt_symbol: Option<String>,
    /// Whether this toast is currently displayed (for UI tracking)
    pub displayed: bool,
    /// Whether this toast has been dismissed by user
    pub dismissed: bool,
}

/// ToastManager — maintains a queue of toast notifications from AlertEngine.
///
/// Receives EVENT_ALERT events via the BaseEngine process_event dispatch,
/// maintains a history of recent alerts, and provides a queue of active
/// (undismissed) toasts for the UI to display.
pub struct ToastManager {
    name: String,
    toasts: RwLock<VecDeque<Toast>>,
    next_id: AtomicU64,
    running: AtomicBool,
}

impl ToastManager {
    /// Create a new ToastManager
    pub fn new() -> Self {
        Self {
            name: "ToastManager".to_string(),
            toasts: RwLock::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
        }
    }

    /// Get all active (undismissed) toasts
    pub fn get_active_toasts(&self) -> Vec<Toast> {
        let toasts = self.toasts.read().unwrap_or_else(|e| e.into_inner());
        toasts.iter().filter(|t| !t.dismissed).cloned().collect()
    }

    /// Get recent toast history (last N toasts, most recent first)
    pub fn get_recent_toasts(&self, limit: usize) -> Vec<Toast> {
        let toasts = self.toasts.read().unwrap_or_else(|e| e.into_inner());
        toasts.iter().rev().take(limit).cloned().collect()
    }

    /// Get all toasts in history
    pub fn get_all_toasts(&self) -> Vec<Toast> {
        let toasts = self.toasts.read().unwrap_or_else(|e| e.into_inner());
        toasts.iter().cloned().collect()
    }

    /// Dismiss a toast by ID
    pub fn dismiss_toast(&self, id: u64) -> Result<(), String> {
        let mut toasts = self.toasts.write().unwrap_or_else(|e| e.into_inner());
        for toast in toasts.iter_mut() {
            if toast.id == id {
                toast.dismissed = true;
                return Ok(());
            }
        }
        Err(format!("Toast #{} not found", id))
    }

    /// Dismiss all toasts
    pub fn dismiss_all(&self) {
        let mut toasts = self.toasts.write().unwrap_or_else(|e| e.into_inner());
        for toast in toasts.iter_mut() {
            toast.dismissed = true;
        }
    }

    /// Clear all toast history
    pub fn clear(&self) {
        let mut toasts = self.toasts.write().unwrap_or_else(|e| e.into_inner());
        toasts.clear();
    }

    /// Get count of active toasts
    pub fn active_count(&self) -> usize {
        let toasts = self.toasts.read().unwrap_or_else(|e| e.into_inner());
        toasts.iter().filter(|t| !t.dismissed).count()
    }

    /// Get total count of toasts in history
    pub fn total_count(&self) -> usize {
        let toasts = self.toasts.read().unwrap_or_else(|e| e.into_inner());
        toasts.len()
    }

    fn add_toast(&self, alert: &AlertMessage) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let toast = Toast {
            id,
            level: alert.level,
            title: alert.title.clone(),
            body: alert.body.clone(),
            source: alert.source.clone(),
            timestamp: alert.timestamp,
            vt_symbol: alert.vt_symbol.clone(),
            displayed: false,
            dismissed: false,
        };

        let mut toasts = self.toasts.write().unwrap_or_else(|e| e.into_inner());
        toasts.push_back(toast);

        // Trim old entries
        while toasts.len() > MAX_TOAST_HISTORY {
            toasts.pop_front();
        }
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for ToastManager {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        if let GatewayEvent::Alert(alert) = event {
            self.add_toast(alert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_alert(title: &str, level: AlertLevel) -> AlertMessage {
        AlertMessage::new(level, title, "Test body", "TestEngine")
    }

    #[test]
    fn test_toast_manager_add() {
        let manager = ToastManager::new();
        let alert = make_alert("Test Alert", AlertLevel::Info);
        
        manager.add_toast(&alert);
        
        let toasts = manager.get_all_toasts();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].title, "Test Alert");
        assert!(!toasts[0].dismissed);
    }

    #[test]
    fn test_toast_manager_dismiss() {
        let manager = ToastManager::new();
        let alert = make_alert("Test Alert", AlertLevel::Warning);
        
        manager.add_toast(&alert);
        
        let toasts = manager.get_all_toasts();
        let id = toasts[0].id;
        
        manager.dismiss_toast(id).unwrap();
        
        let active = manager.get_active_toasts();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_toast_manager_dismiss_all() {
        let manager = ToastManager::new();
        
        manager.add_toast(&make_alert("Alert 1", AlertLevel::Info));
        manager.add_toast(&make_alert("Alert 2", AlertLevel::Warning));
        manager.add_toast(&make_alert("Alert 3", AlertLevel::Critical));
        
        assert_eq!(manager.active_count(), 3);
        
        manager.dismiss_all();
        
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_toast_manager_max_history() {
        let manager = ToastManager::new();
        
        for i in 0..150 {
            manager.add_toast(&make_alert(&format!("Alert {}", i), AlertLevel::Info));
        }
        
        assert_eq!(manager.total_count(), MAX_TOAST_HISTORY);
    }

    #[test]
    fn test_toast_manager_recent() {
        let manager = ToastManager::new();
        
        manager.add_toast(&make_alert("First", AlertLevel::Info));
        manager.add_toast(&make_alert("Second", AlertLevel::Info));
        manager.add_toast(&make_alert("Third", AlertLevel::Info));
        
        let recent = manager.get_recent_toasts(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].title, "Third");
        assert_eq!(recent[1].title, "Second");
    }
}
