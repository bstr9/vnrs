//! Reconciliation Engine for syncing local vs venue state on gateway reconnect.
//!
//! When a gateway reconnects after a disconnection, local OMS state may have drifted
//! from the actual venue state (filled orders, changed positions, etc.).
//! The ReconciliationEngine detects this drift and alerts the operator.

use std::collections::HashMap;
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use super::alert::{AlertLevel, AlertMessage};
use super::constant::Status;
use super::engine::{BaseEngine, MainEngine};
use super::event::EVENT_CONTRACT;
use super::gateway::GatewayEvent;

// ============================================================================
// Drift detection structs
// ============================================================================

/// Default drift threshold percentage for critical alert
const DEFAULT_DRIFT_THRESHOLD_PERCENT: f64 = 5.0;

/// Position drift detected during reconciliation
#[derive(Debug, Clone)]
pub struct PositionDrift {
    /// vt_symbol (e.g., BTCUSDT.BINANCE)
    pub vt_symbol: String,
    /// Volume from OmsEngine
    pub local_volume: f64,
    /// Volume from exchange query
    pub venue_volume: f64,
    /// venue_volume - local_volume
    pub drift: f64,
    /// drift / local_volume * 100 (0.0 if local_volume == 0)
    pub drift_percent: f64,
}

/// Order drift detected during reconciliation
#[derive(Debug, Clone)]
pub struct OrderDrift {
    /// vt_orderid (gateway_name.orderid)
    pub vt_orderid: String,
    /// Status from OmsEngine
    pub local_status: Status,
    /// Status from exchange query
    pub venue_status: Status,
    /// Remaining volume from OmsEngine (volume - traded)
    pub local_volume: f64,
    /// Remaining volume on exchange
    pub venue_volume: f64,
}

/// Result of a reconciliation pass
#[derive(Debug, Clone)]
pub struct ReconciliationResult {
    /// Position drifts detected
    pub position_drifts: Vec<PositionDrift>,
    /// Order drifts detected
    pub order_drifts: Vec<OrderDrift>,
    /// When this reconciliation was performed
    pub timestamp: DateTime<Utc>,
    /// Which gateway was reconciled
    pub gateway_name: String,
    /// Whether any drift was detected
    pub has_drift: bool,
}

impl ReconciliationResult {
    /// Create a new reconciliation result
    pub fn new(
        position_drifts: Vec<PositionDrift>,
        order_drifts: Vec<OrderDrift>,
        gateway_name: String,
    ) -> Self {
        let has_drift = !position_drifts.is_empty() || !order_drifts.is_empty();
        Self {
            position_drifts,
            order_drifts,
            timestamp: Utc::now(),
            gateway_name,
            has_drift,
        }
    }
}

// ============================================================================
// ReconciliationEngine
// ============================================================================

/// Reconciliation engine for syncing local vs venue state.
///
/// On gateway reconnect, triggers query_position() and query_account()
/// on the gateway, then compares the updated OmsEngine state against
/// pre-snapshot state to detect drift.
///
/// For order reconciliation, compares local active orders against
/// venue state after query_order triggers.
pub struct ReconciliationEngine {
    /// Reference to MainEngine for data access
    main_engine: Arc<MainEngine>,
    /// Last reconciliation result
    last_result: RwLock<Option<ReconciliationResult>>,
    /// Whether auto-reconciliation is enabled on gateway reconnect
    auto_reconcile: AtomicBool,
    /// Drift threshold percentage for critical alerts
    drift_threshold: RwLock<f64>,
    /// Snapshot of positions before venue query (vt_positionid -> volume)
    position_snapshot: RwLock<HashMap<String, f64>>,
    /// Snapshot of active orders before venue query (vt_orderid -> (Status, remaining_volume))
    order_snapshot: RwLock<HashMap<String, (Status, f64)>>,
}

impl ReconciliationEngine {
    /// Create a new ReconciliationEngine
    pub fn new(main_engine: Arc<MainEngine>) -> Self {
        Self {
            main_engine,
            last_result: RwLock::new(None),
            auto_reconcile: AtomicBool::new(true),
            drift_threshold: RwLock::new(DEFAULT_DRIFT_THRESHOLD_PERCENT),
            position_snapshot: RwLock::new(HashMap::new()),
            order_snapshot: RwLock::new(HashMap::new()),
        }
    }

    /// Run full reconciliation for a specific gateway.
    ///
    /// This will:
    /// 1. Take a snapshot of current local positions and orders
    /// 2. Trigger gateway query_position() and query_account() to refresh venue state
    /// 3. Wait briefly for events to propagate through OmsEngine
    /// 4. Compare local state (post-query) against the pre-snapshot
    /// 5. Return a ReconciliationResult with any detected drift
    pub async fn reconcile(&self, gateway_name: &str) -> Result<ReconciliationResult, String> {
        info!("开始对账: gateway={}", gateway_name);

        // Step 1: Take snapshots of current local state
        self.take_position_snapshot();
        self.take_order_snapshot(gateway_name);

        // Step 2: Trigger venue queries
        let gateway = self.main_engine.get_gateway(gateway_name)
            .ok_or_else(|| format!("找不到底层接口：{}", gateway_name))?;

        // Query positions and accounts to trigger venue state updates
        if let Err(e) = gateway.query_position().await {
            warn!("对账查询持仓失败: {}", e);
        }
        if let Err(e) = gateway.query_account().await {
            warn!("对账查询账户失败: {}", e);
        }

        // Step 3: Wait briefly for events to propagate through OmsEngine
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Step 4: Compare and detect drift
        let position_drifts = self.detect_position_drift(gateway_name);
        let order_drifts = self.detect_order_drift(gateway_name);

        let result = ReconciliationResult::new(position_drifts, order_drifts, gateway_name.to_string());

        // Step 5: Alert on drift
        if result.has_drift {
            self.alert_drift(&result);
        }

        // Store result
        *self.last_result.write().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        }) = Some(result.clone());

        info!(
            "对账完成: gateway={}, 持仓偏差={}, 委托偏差={}",
            gateway_name,
            result.position_drifts.len(),
            result.order_drifts.len()
        );

        Ok(result)
    }

    /// Reconcile positions only.
    ///
    /// Takes a snapshot, queries venue positions, then detects drift.
    pub async fn reconcile_positions(&self, gateway_name: &str) -> Result<Vec<PositionDrift>, String> {
        info!("开始持仓对账: gateway={}", gateway_name);

        self.take_position_snapshot();

        let gateway = self.main_engine.get_gateway(gateway_name)
            .ok_or_else(|| format!("找不到底层接口：{}", gateway_name))?;

        if let Err(e) = gateway.query_position().await {
            warn!("对账查询持仓失败: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let drifts = self.detect_position_drift(gateway_name);

        if !drifts.is_empty() {
            self.alert_position_drift(&drifts, gateway_name);
        }

        info!("持仓对账完成: 发现{}个偏差", drifts.len());
        Ok(drifts)
    }

    /// Reconcile orders only.
    ///
    /// Takes a snapshot of active orders, queries venue, then detects drift.
    pub async fn reconcile_orders(&self, gateway_name: &str) -> Result<Vec<OrderDrift>, String> {
        info!("开始委托对账: gateway={}", gateway_name);

        self.take_order_snapshot(gateway_name);

        let gateway = self.main_engine.get_gateway(gateway_name)
            .ok_or_else(|| format!("找不到底层接口：{}", gateway_name))?;

        // Query account to trigger order updates via gateway connect flow
        if let Err(e) = gateway.query_account().await {
            warn!("对账查询账户失败: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let drifts = self.detect_order_drift(gateway_name);

        if !drifts.is_empty() {
            self.alert_order_drift(&drifts, gateway_name);
        }

        info!("委托对账完成: 发现{}个偏差", drifts.len());
        Ok(drifts)
    }

    /// Get last reconciliation result
    pub fn last_result(&self) -> Option<ReconciliationResult> {
        self.last_result.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        }).clone()
    }

    /// Enable/disable auto-reconcile on gateway reconnect
    pub fn set_auto_reconcile(&self, enabled: bool) {
        self.auto_reconcile.store(enabled, Ordering::SeqCst);
        if enabled {
            info!("自动对账已启用");
        } else {
            info!("自动对账已禁用");
        }
    }

    /// Check if auto-reconcile is enabled
    pub fn is_auto_reconcile(&self) -> bool {
        self.auto_reconcile.load(Ordering::SeqCst)
    }

    /// Set drift threshold percentage for critical alerts
    pub fn set_drift_threshold(&self, threshold: f64) {
        let mut current = self.drift_threshold.write().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });
        *current = threshold;
        info!("持仓偏差阈值已设置为: {}%", threshold);
    }

    /// Get current drift threshold
    pub fn drift_threshold(&self) -> f64 {
        *self.drift_threshold.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        })
    }

    // ========================================================================
    // Internal methods
    // ========================================================================

    /// Take a snapshot of current positions from OmsEngine
    fn take_position_snapshot(&self) {
        let positions = self.main_engine.get_all_positions();
        let mut snapshot = self.position_snapshot.write().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });
        snapshot.clear();
        for pos in &positions {
            snapshot.insert(pos.vt_positionid(), pos.volume);
        }
        info!("持仓快照: {} 个持仓", snapshot.len());
    }

    /// Take a snapshot of active orders from OmsEngine filtered by gateway
    fn take_order_snapshot(&self, gateway_name: &str) {
        let orders = self.main_engine.get_all_active_orders();
        let mut snapshot = self.order_snapshot.write().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });
        snapshot.clear();
        for order in &orders {
            if order.gateway_name == gateway_name {
                let remaining = order.volume - order.traded;
                snapshot.insert(order.vt_orderid(), (order.status, remaining));
            }
        }
        info!("委托快照: {} 个活跃委托 (gateway={})", snapshot.len(), gateway_name);
    }

    /// Detect position drift by comparing snapshot against current OmsEngine state
    fn detect_position_drift(&self, gateway_name: &str) -> Vec<PositionDrift> {
        let snapshot = self.position_snapshot.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });

        let current_positions = self.main_engine.get_all_positions();
        let threshold = *self.drift_threshold.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });

        // Build current position map (filtered by gateway)
        let mut current_map: HashMap<String, f64> = HashMap::new();
        for pos in &current_positions {
            if pos.gateway_name == gateway_name {
                current_map.insert(pos.vt_positionid(), pos.volume);
            }
        }

        let mut drifts = Vec::new();

        // Check all positions that exist in either snapshot or current
        let mut all_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        for key in snapshot.keys() {
            all_keys.insert(key.clone());
        }
        for key in current_map.keys() {
            all_keys.insert(key.clone());
        }

        for vt_positionid in all_keys {
            let local_volume = snapshot.get(&vt_positionid).copied().unwrap_or(0.0);
            let venue_volume = current_map.get(&vt_positionid).copied().unwrap_or(0.0);
            let drift = venue_volume - local_volume;

            // Only report if there's meaningful drift
            if drift.abs() > 1e-10 {
                let drift_percent = if local_volume.abs() > 1e-10 {
                    (drift / local_volume) * 100.0
                } else {
                    // New position appeared on venue (was 0 locally)
                    if venue_volume.abs() > 1e-10 {
                        100.0
                    } else {
                        0.0
                    }
                };

                // Extract vt_symbol from vt_positionid (format: gateway_name.vt_symbol.direction)
                // We'll construct it from the position ID
                let parts: Vec<&str> = vt_positionid.split('.').collect();
                let vt_symbol = if parts.len() >= 3 {
                    // Reconstruct: symbol.exchange (middle parts)
                    parts[1..parts.len() - 1].join(".")
                } else {
                    vt_positionid.clone()
                };

                let position_drift = PositionDrift {
                    vt_symbol,
                    local_volume,
                    venue_volume,
                    drift,
                    drift_percent,
                };

                // Only report if drift exceeds a minimal threshold or percentage
                if drift_percent.abs() >= threshold || local_volume.abs() < 1e-10 {
                    drifts.push(position_drift);
                }
            }
        }

        drifts
    }

    /// Detect order drift by comparing snapshot against current OmsEngine state
    fn detect_order_drift(&self, gateway_name: &str) -> Vec<OrderDrift> {
        let snapshot = self.order_snapshot.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });

        let current_orders = self.main_engine.get_all_active_orders();
        let mut current_map: HashMap<String, (Status, f64)> = HashMap::new();
        for order in &current_orders {
            if order.gateway_name == gateway_name {
                let remaining = order.volume - order.traded;
                current_map.insert(order.vt_orderid(), (order.status, remaining));
            }
        }

        let mut drifts = Vec::new();

        // Check all orders in snapshot — if they're no longer active or status changed
        for (vt_orderid, (local_status, local_remaining)) in snapshot.iter() {
            match current_map.get(vt_orderid) {
                Some((venue_status, venue_remaining)) => {
                    // Order still exists but status or remaining volume changed
                    if local_status != venue_status || (local_remaining - venue_remaining).abs() > 1e-10 {
                        drifts.push(OrderDrift {
                            vt_orderid: vt_orderid.clone(),
                            local_status: *local_status,
                            venue_status: *venue_status,
                            local_volume: *local_remaining,
                            venue_volume: *venue_remaining,
                        });
                    }
                }
                None => {
                    // Order was in local snapshot but no longer active on venue
                    // This could mean it was filled or cancelled on the venue
                    drifts.push(OrderDrift {
                        vt_orderid: vt_orderid.clone(),
                        local_status: *local_status,
                        venue_status: Status::AllTraded, // Assumed filled/cancelled since no longer active
                        local_volume: *local_remaining,
                        venue_volume: 0.0,
                    });
                }
            }
        }

        // Check for orders that appeared on venue but weren't in local snapshot
        for (vt_orderid, (venue_status, venue_remaining)) in current_map.iter() {
            if !snapshot.contains_key(vt_orderid) {
                drifts.push(OrderDrift {
                    vt_orderid: vt_orderid.clone(),
                    local_status: Status::Submitting,
                    venue_status: *venue_status,
                    local_volume: 0.0,
                    venue_volume: *venue_remaining,
                });
            }
        }

        drifts
    }

    /// Alert on detected drift
    fn alert_drift(&self, result: &ReconciliationResult) {
        let alert_engine = self.main_engine.alert_engine();
        let threshold = *self.drift_threshold.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });

        // Position drift alerts
        for drift in &result.position_drifts {
            let level = if drift.drift_percent.abs() >= threshold {
                AlertLevel::Critical
            } else {
                AlertLevel::Warning
            };

            let alert = AlertMessage::new(
                level,
                "发现持仓偏差",
                format!(
                    "{}: 本地={}, 交易所={}, 偏差={:.4} ({:.2}%)",
                    drift.vt_symbol, drift.local_volume, drift.venue_volume,
                    drift.drift, drift.drift_percent
                ),
                "ReconciliationEngine",
            ).with_symbol(&drift.vt_symbol);

            alert_engine.send_alert(alert);
        }

        // Order drift alerts
        for drift in &result.order_drifts {
            let alert = AlertMessage::new(
                AlertLevel::Warning,
                "发现委托偏差",
                format!(
                    "{}: 本地状态={:?}, 交易所状态={:?}, 本地余量={}, 交易所余量={}",
                    drift.vt_orderid, drift.local_status, drift.venue_status,
                    drift.local_volume, drift.venue_volume
                ),
                "ReconciliationEngine",
            ).with_orderid(&drift.vt_orderid);

            alert_engine.send_alert(alert);
        }
    }

    /// Alert on position drift only
    fn alert_position_drift(&self, drifts: &[PositionDrift], gateway_name: &str) {
        let alert_engine = self.main_engine.alert_engine();
        let threshold = *self.drift_threshold.read().unwrap_or_else(|e| {
            warn!("ReconciliationEngine lock poisoned, recovering");
            e.into_inner()
        });

        for drift in drifts {
            let level = if drift.drift_percent.abs() >= threshold {
                AlertLevel::Critical
            } else {
                AlertLevel::Warning
            };

            let alert = AlertMessage::new(
                level,
                "发现持仓偏差",
                format!(
                    "{}: 本地={}, 交易所={}, 偏差={:.4} ({:.2}%)",
                    drift.vt_symbol, drift.local_volume, drift.venue_volume,
                    drift.drift, drift.drift_percent
                ),
                gateway_name,
            ).with_symbol(&drift.vt_symbol);

            alert_engine.send_alert(alert);
        }
    }

    /// Alert on order drift only
    fn alert_order_drift(&self, drifts: &[OrderDrift], gateway_name: &str) {
        let alert_engine = self.main_engine.alert_engine();

        for drift in drifts {
            let alert = AlertMessage::new(
                AlertLevel::Warning,
                "发现委托偏差",
                format!(
                    "{}: 本地状态={:?}, 交易所状态={:?}",
                    drift.vt_orderid, drift.local_status, drift.venue_status
                ),
                gateway_name,
            ).with_orderid(&drift.vt_orderid);

            alert_engine.send_alert(alert);
        }
    }
}

impl BaseEngine for ReconciliationEngine {
    fn engine_name(&self) -> &str {
        "ReconciliationEngine"
    }

    fn process_event(&self, event_type: &str, _event: &GatewayEvent) {
        // On gateway connect event (EVENT_CONTRACT), trigger auto-reconciliation
        if event_type.starts_with(EVENT_CONTRACT) && self.auto_reconcile.load(Ordering::SeqCst) {
            // Extract gateway name from the event type or use the event data
            // EVENT_CONTRACT is fired when a gateway connects and sends contract data
            if let GatewayEvent::Contract(contract) = _event {
                let gateway_name = contract.gateway_name.clone();
                info!("检测到网关连接，触发自动对账: gateway={}", gateway_name);

                let main_engine = self.main_engine.clone();
                tokio::spawn(async move {
                    // Small delay to let initial contract/account queries complete
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                    // Get the reconciliation engine from MainEngine and run reconcile
                    if let Some(recon) = main_engine.reconciliation_engine() {
                        if let Err(e) = recon.reconcile(&gateway_name).await {
                            warn!("自动对账失败: gateway={}, error={}", gateway_name, e);
                        }
                    }
                });
            }
        }
    }

    fn close(&self) {
        self.auto_reconcile.store(false, Ordering::SeqCst);
        info!("ReconciliationEngine已关闭");
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Direction, Exchange};
    use crate::trader::object::{OrderData, PositionData};

    /// Helper to create a PositionData for testing
    fn make_position(
        gateway_name: &str,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        volume: f64,
    ) -> PositionData {
        let mut pos = PositionData::new(
            gateway_name.to_string(),
            symbol.to_string(),
            exchange,
            direction,
        );
        pos.volume = volume;
        pos
    }

    #[test]
    fn test_reconciliation_result_new_no_drift() {
        let result = ReconciliationResult::new(
            Vec::new(),
            Vec::new(),
            "BINANCE_SPOT".to_string(),
        );
        assert!(!result.has_drift);
        assert!(result.position_drifts.is_empty());
        assert!(result.order_drifts.is_empty());
        assert_eq!(result.gateway_name, "BINANCE_SPOT");
    }

    #[test]
    fn test_reconciliation_result_new_with_drift() {
        let position_drift = PositionDrift {
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            local_volume: 1.0,
            venue_volume: 1.5,
            drift: 0.5,
            drift_percent: 50.0,
        };
        let result = ReconciliationResult::new(
            vec![position_drift],
            Vec::new(),
            "BINANCE_SPOT".to_string(),
        );
        assert!(result.has_drift);
        assert_eq!(result.position_drifts.len(), 1);
    }

    #[test]
    fn test_position_drift_calculation() {
        let drift = PositionDrift {
            vt_symbol: "ETHUSDT.BINANCE".to_string(),
            local_volume: 10.0,
            venue_volume: 11.0,
            drift: 1.0,
            drift_percent: 10.0,
        };
        assert_eq!(drift.drift, 1.0);
        assert!((drift.drift_percent - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_position_drift_zero_local_volume() {
        // When local volume is 0 but venue has position
        let drift = PositionDrift {
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            local_volume: 0.0,
            venue_volume: 0.5,
            drift: 0.5,
            drift_percent: 100.0, // New position appeared
        };
        assert_eq!(drift.drift, 0.5);
        assert!((drift.drift_percent - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_order_drift_status_change() {
        let drift = OrderDrift {
            vt_orderid: "BINANCE_SPOT.12345".to_string(),
            local_status: Status::NotTraded,
            venue_status: Status::AllTraded,
            local_volume: 1.0,
            venue_volume: 0.0,
        };
        assert_ne!(drift.local_status, drift.venue_status);
        assert_eq!(drift.venue_volume, 0.0);
    }

    #[test]
    fn test_reconciliation_engine_auto_reconcile() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));

        assert!(recon.is_auto_reconcile());
        recon.set_auto_reconcile(false);
        assert!(!recon.is_auto_reconcile());
        recon.set_auto_reconcile(true);
        assert!(recon.is_auto_reconcile());
    }

    #[test]
    fn test_reconciliation_engine_drift_threshold() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));

        assert!((recon.drift_threshold() - 5.0).abs() < 1e-10);
        recon.set_drift_threshold(10.0);
        assert!((recon.drift_threshold() - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_reconciliation_engine_last_result_initially_none() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));
        assert!(recon.last_result().is_none());
    }

    #[test]
    fn test_reconciliation_engine_base_engine() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));
        assert_eq!(recon.engine_name(), "ReconciliationEngine");
    }

    #[test]
    fn test_detect_position_drift_with_snapshot() {
        let engine = Arc::new(MainEngine::new());
        let recon = ReconciliationEngine::new(engine.clone());

        // Manually populate snapshot
        {
            let mut snapshot = recon.position_snapshot.write().unwrap_or_else(|e| e.into_inner());
            snapshot.insert("BINANCE_SPOT.BTCUSDT.BINANCE.多".to_string(), 1.0);
            snapshot.insert("BINANCE_SPOT.ETHUSDT.BINANCE.多".to_string(), 10.0);
        }

        // Add a position to OmsEngine that differs from snapshot
        let pos = make_position("BINANCE_SPOT", "BTCUSDT", Exchange::Binance, Direction::Long, 1.5);
        engine.oms().process_position(pos);

        // Add matching position (no drift)
        let pos2 = make_position("BINANCE_SPOT", "ETHUSDT", Exchange::Binance, Direction::Long, 10.0);
        engine.oms().process_position(pos2);

        let drifts = recon.detect_position_drift("BINANCE_SPOT");

        // Should find drift for BTCUSDT but not ETHUSDT
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].vt_symbol.contains("BTCUSDT"));
        assert!((drifts[0].drift - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_detect_order_drift_missing_on_venue() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));

        // Add an order to the snapshot
        {
            let mut snapshot = recon.order_snapshot.write().unwrap_or_else(|e| e.into_inner());
            snapshot.insert("BINANCE_SPOT.order1".to_string(), (Status::NotTraded, 1.0));
        }

        // No active orders in OmsEngine — order was filled on venue
        let drifts = recon.detect_order_drift("BINANCE_SPOT");

        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].local_status, Status::NotTraded);
        assert_eq!(drifts[0].venue_status, Status::AllTraded);
        assert_eq!(drifts[0].venue_volume, 0.0);
    }

    #[test]
    fn test_detect_order_drift_new_on_venue() {
        let engine = Arc::new(MainEngine::new());
        let recon = ReconciliationEngine::new(engine.clone());

        // Empty snapshot, but OmsEngine has an active order
        {
            let mut snapshot = recon.order_snapshot.write().unwrap_or_else(|e| e.into_inner());
            snapshot.clear();
        }

        // Add an active order to OmsEngine
        use crate::trader::constant::OrderType;
        let order = OrderData {
                    gateway_name: "BINANCE_SPOT".to_string(),
                    symbol: "btcusdt".to_string(),
                    exchange: Exchange::Binance,
                    orderid: "order_new".to_string(),
                    order_type: OrderType::Limit,
                    direction: Some(Direction::Long),
                    offset: crate::trader::constant::Offset::None,
                    price: 50000.0,
                    volume: 1.0,
                    traded: 0.0,
                    status: Status::NotTraded,
                    datetime: Some(Utc::now()),
                    reference: String::new(),
                    post_only: false,
                    reduce_only: false,
                    extra: None,
                };
        engine.oms().process_order(order);

        let drifts = recon.detect_order_drift("BINANCE_SPOT");

        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].local_status, Status::Submitting);
        assert_eq!(drifts[0].venue_status, Status::NotTraded);
    }

    #[test]
    fn test_close_disables_auto_reconcile() {
        let engine = MainEngine::new();
        let recon = ReconciliationEngine::new(Arc::new(engine));
        assert!(recon.is_auto_reconcile());
        recon.close();
        assert!(!recon.is_auto_reconcile());
    }
}
