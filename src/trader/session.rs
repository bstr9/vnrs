//! Trading Session Manager — tracks exchange trading hours and session state
//!
//! Provides centralized trading session management with pre-defined sessions
//! for major exchanges. Handles overnight sessions (e.g., night session crossing midnight).

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, NaiveTime, Timelike, Utc, Duration, FixedOffset, TimeZone};
use tracing::warn;

use super::engine::BaseEngine;
use super::constant::Exchange;
use super::gateway::GatewayEvent;

// ============================================================================
// TradingSession
// ============================================================================

/// A trading session with open/close times
#[derive(Debug, Clone)]
pub struct TradingSession {
    /// Session name (e.g., "Day Session", "Night Session")
    pub name: String,
    /// Exchange this session belongs to
    pub exchange: Exchange,
    /// Session open time (local time)
    pub open_time: NaiveTime,
    /// Session close time (local time)
    pub close_time: NaiveTime,
    /// Whether this session crosses midnight (close_time < open_time)
    pub is_overnight: bool,
}

impl TradingSession {
    /// Create a new trading session
    pub fn new(name: String, exchange: Exchange, open_time: NaiveTime, close_time: NaiveTime) -> Self {
        let is_overnight = close_time < open_time;
        Self {
            name,
            exchange,
            open_time,
            close_time,
            is_overnight,
        }
    }
    
    /// Check if a given time is within this session
    pub fn contains_time(&self, time: NaiveTime) -> bool {
        if self.is_overnight {
            // Overnight session: active from open_time to midnight OR midnight to close_time
            time >= self.open_time || time < self.close_time
        } else {
            // Normal session: active between open and close
            time >= self.open_time && time < self.close_time
        }
    }
    
    /// Get remaining time until session close
    pub fn remaining_time(&self, now: NaiveTime) -> Option<Duration> {
        if !self.contains_time(now) {
            return None;
        }
        
        if self.is_overnight {
            if now >= self.open_time {
                // After midnight crossing point
                let end_of_day = NaiveTime::from_hms_opt(23, 59, 59).unwrap_or_default();
                let to_end = end_of_day - now;
                let from_start = Duration::seconds(self.close_time.num_seconds_from_midnight() as i64);
                Some(to_end + from_start)
            } else {
                // Before close_time
                let secs = self.close_time.num_seconds_from_midnight() - now.num_seconds_from_midnight();
                Some(Duration::seconds(secs as i64))
            }
        } else {
            let secs = self.close_time.num_seconds_from_midnight() - now.num_seconds_from_midnight();
            if secs > 0 {
                Some(Duration::seconds(secs as i64))
            } else {
                None
            }
        }
    }
}

// ============================================================================
// TradingSessionManager
// ============================================================================

/// Manages trading sessions for all exchanges
pub struct TradingSessionManager {
    /// Sessions by exchange
    sessions: RwLock<HashMap<Exchange, Vec<TradingSession>>>,
}

impl TradingSessionManager {
    /// Create a new TradingSessionManager with pre-defined sessions
    pub fn new() -> Self {
        let mut sessions: HashMap<Exchange, Vec<TradingSession>> = HashMap::new();
        
        // Binance: 24/7 crypto trading
        sessions.insert(Exchange::Binance, vec![
            TradingSession::new(
                "24/7".to_string(),
                Exchange::Binance,
                NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default(),
                NaiveTime::from_hms_opt(23, 59, 59).unwrap_or_default(),
            ),
        ]);
        
        // Chinese futures day session: 09:00 - 15:15
        let chinese_futures_day = TradingSession::new(
            "Day Session".to_string(),
            Exchange::Shfe,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap_or_default(),
            NaiveTime::from_hms_opt(15, 15, 0).unwrap_or_default(),
        );
        
        // Chinese futures night session: 21:00 - 02:30 (overnight)
        let chinese_futures_night = TradingSession::new(
            "Night Session".to_string(),
            Exchange::Shfe,
            NaiveTime::from_hms_opt(21, 0, 0).unwrap_or_default(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap_or_default(),
        );
        
        for exchange in [Exchange::Shfe, Exchange::Ine, Exchange::Dce, Exchange::Czce, Exchange::Gfex] {
            let mut exchange_sessions = vec![chinese_futures_day.clone()];
            exchange_sessions[0].exchange = exchange;
            let mut night = chinese_futures_night.clone();
            night.exchange = exchange;
            exchange_sessions.push(night);
            sessions.insert(exchange, exchange_sessions);
        }
        
        // CFFEX: 09:30 - 15:15 (no night session)
        sessions.insert(Exchange::Cffex, vec![
            TradingSession::new(
                "Day Session".to_string(),
                Exchange::Cffex,
                NaiveTime::from_hms_opt(9, 30, 0).unwrap_or_default(),
                NaiveTime::from_hms_opt(15, 15, 0).unwrap_or_default(),
            ),
        ]);
        
        // SSE/SZSE: 09:30 - 11:30, 13:00 - 15:00
        for exchange in [Exchange::Sse, Exchange::Szse] {
            sessions.insert(exchange, vec![
                TradingSession::new(
                    "Morning Session".to_string(),
                    exchange,
                    NaiveTime::from_hms_opt(9, 30, 0).unwrap_or_default(),
                    NaiveTime::from_hms_opt(11, 30, 0).unwrap_or_default(),
                ),
                TradingSession::new(
                    "Afternoon Session".to_string(),
                    exchange,
                    NaiveTime::from_hms_opt(13, 0, 0).unwrap_or_default(),
                    NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default(),
                ),
            ]);
        }
        
        Self {
            sessions: RwLock::new(sessions),
        }
    }
    
    /// Convert UTC time to Asia/Shanghai time (UTC+8)
    fn to_shanghai_time(dt: DateTime<Utc>) -> DateTime<FixedOffset> {
        let shanghai_offset = FixedOffset::east_opt(8 * 3600).unwrap_or_else(|| {
            FixedOffset::east_opt(0).unwrap_or_else(|| {
                // UTC+0 is always valid; this should never fail
                panic!("FixedOffset::east_opt(0) failed — this should never happen")
            })
        });
        dt.with_timezone(&shanghai_offset)
    }
    
    /// Check if trading is allowed at the given time for an exchange
    pub fn is_trading_time(&self, exchange: Exchange, dt: DateTime<Utc>) -> bool {
        let sessions = self.sessions.read().unwrap_or_else(|e| {
            warn!("TradingSessionManager lock poisoned, recovering");
            e.into_inner()
        });
        
        let exchange_sessions = match sessions.get(&exchange) {
            Some(s) => s,
            None => return true, // Unknown exchanges default to always open
        };
        
        // Convert to local time (assume Asia/Shanghai for Chinese exchanges)
        let local_dt = Self::to_shanghai_time(dt);
        let local_time = local_dt.time();
        
        exchange_sessions.iter().any(|s| s.contains_time(local_time))
    }
    
    /// Get the current active session for an exchange
    pub fn get_current_session(&self, exchange: Exchange, dt: DateTime<Utc>) -> Option<TradingSession> {
        let sessions = self.sessions.read().unwrap_or_else(|e| {
            warn!("TradingSessionManager lock poisoned, recovering");
            e.into_inner()
        });
        
        let exchange_sessions = sessions.get(&exchange)?;
        let local_dt = Self::to_shanghai_time(dt);
        let local_time = local_dt.time();
        
        exchange_sessions
            .iter()
            .find(|s| s.contains_time(local_time))
            .cloned()
    }
    
    /// Get all sessions for an exchange
    pub fn get_sessions(&self, exchange: Exchange) -> Vec<TradingSession> {
        let sessions = self.sessions.read().unwrap_or_else(|e| {
            warn!("TradingSessionManager lock poisoned, recovering");
            e.into_inner()
        });
        
        sessions.get(&exchange).cloned().unwrap_or_default()
    }
    
    /// Add a custom session for an exchange
    pub fn add_session(&self, session: TradingSession) {
        let mut sessions = self.sessions.write().unwrap_or_else(|e| {
            warn!("TradingSessionManager lock poisoned, recovering");
            e.into_inner()
        });
        
        sessions
            .entry(session.exchange)
            .or_default()
            .push(session);
    }
    
    /// Get remaining time until session close
    pub fn get_remaining_time(&self, exchange: Exchange, dt: DateTime<Utc>) -> Option<Duration> {
        let session = self.get_current_session(exchange, dt)?;
        let local_dt = Self::to_shanghai_time(dt);
        let local_time = local_dt.time();
        session.remaining_time(local_time)
    }
    
    /// Get the next session open time after the given time
    pub fn get_next_open_time(&self, exchange: Exchange, dt: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let sessions = self.sessions.read().unwrap_or_else(|e| {
            warn!("TradingSessionManager lock poisoned, recovering");
            e.into_inner()
        });
        
        let exchange_sessions = sessions.get(&exchange)?;
        let local_dt = Self::to_shanghai_time(dt);
        let local_time = local_dt.time();
        
        // Find the next session that opens after current time
        for session in exchange_sessions {
            if local_time < session.open_time {
                // Today's session hasn't opened yet
                let open_dt = local_dt.date_naive().and_time(session.open_time);
                return Some(Utc.from_utc_datetime(&open_dt));
            }
        }
        
        // All sessions for today have passed, find tomorrow's first session
        let first_session = exchange_sessions.first()?;
        let tomorrow = local_dt.date_naive() + chrono::Duration::days(1);
        let open_dt = tomorrow.and_time(first_session.open_time);
        Some(Utc.from_utc_datetime(&open_dt))
    }
}

impl Default for TradingSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for TradingSessionManager {
    fn engine_name(&self) -> &str {
        "TradingSessionManager"
    }
    
    fn process_event(&self, _event_type: &str, _event: &GatewayEvent) {
        // No action needed — sessions are static
    }
    
    fn close(&self) {
        // No cleanup needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    
    #[test]
    fn test_session_new() {
        let session = TradingSession::new(
            "Test".to_string(),
            Exchange::Binance,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap_or_default(),
            NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default(),
        );
        assert_eq!(session.name, "Test");
        assert!(!session.is_overnight);
    }
    
    #[test]
    fn test_overnight_session() {
        let session = TradingSession::new(
            "Night".to_string(),
            Exchange::Shfe,
            NaiveTime::from_hms_opt(21, 0, 0).unwrap_or_default(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap_or_default(),
        );
        assert!(session.is_overnight);
        assert!(session.contains_time(NaiveTime::from_hms_opt(22, 0, 0).unwrap_or_default()));
        assert!(session.contains_time(NaiveTime::from_hms_opt(1, 0, 0).unwrap_or_default()));
        assert!(!session.contains_time(NaiveTime::from_hms_opt(15, 0, 0).unwrap_or_default()));
    }
    
    #[test]
    fn test_binance_always_open() {
        let manager = TradingSessionManager::new();
        let dt = Utc.with_ymd_and_hms(2025, 1, 1, 3, 30, 0).unwrap(); // 3:30 AM UTC
        assert!(manager.is_trading_time(Exchange::Binance, dt));
    }
    
    #[test]
    fn test_chinese_futures_day_session() {
        let manager = TradingSessionManager::new();
        // 10:00 AM Shanghai time = 02:00 UTC
        let dt = Utc.with_ymd_and_hms(2025, 1, 1, 2, 0, 0).unwrap();
        assert!(manager.is_trading_time(Exchange::Shfe, dt));
        
        // 4:00 PM Shanghai time = 08:00 UTC (after session close)
        let dt2 = Utc.with_ymd_and_hms(2025, 1, 1, 8, 0, 0).unwrap();
        assert!(!manager.is_trading_time(Exchange::Shfe, dt2));
    }
    
    #[test]
    fn test_get_sessions() {
        let manager = TradingSessionManager::new();
        let sessions = manager.get_sessions(Exchange::Sse);
        assert_eq!(sessions.len(), 2); // Morning + Afternoon
    }
    
    #[test]
    fn test_remaining_time() {
        let manager = TradingSessionManager::new();
        // 14:00 Shanghai time = 06:00 UTC (1h15m before close at 15:15)
        let dt = Utc.with_ymd_and_hms(2025, 1, 1, 6, 0, 0).unwrap();
        let remaining = manager.get_remaining_time(Exchange::Shfe, dt);
        assert!(remaining.is_some());
        let secs = remaining.unwrap().num_seconds();
        assert!(secs > 4000 && secs < 5000); // ~1h15m = 4500s
    }
}
