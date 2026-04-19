//! Contract Manager — caches instrument/contract details for order validation
//!
//! Provides centralized contract data lookup with price tick rounding,
//! volume rounding, and order validation. Inspired by vnpy's
//! `ContractManager` / `BaseGateway` contract caching.

use std::collections::HashMap;
use std::sync::RwLock;

use tracing::warn;

use super::gateway::GatewayEvent;
use super::object::{ContractData, OrderRequest};

// ============================================================================
// ContractManager
// ============================================================================

/// Manages contract data for all traded instruments.
///
/// Automatically updates when contract events arrive from gateways.
/// Provides price/volume rounding and order validation utilities.
pub struct ContractManager {
    /// Contract data cache: vt_symbol -> ContractData
    contracts: RwLock<HashMap<String, ContractData>>,
}

impl ContractManager {
    /// Create a new empty ContractManager
    pub fn new() -> Self {
        Self {
            contracts: RwLock::new(HashMap::new()),
        }
    }

    // ========================================================================
    // Data ingestion
    // ========================================================================

    /// Process a gateway event — updates contract cache on Contract events
    pub fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        if let GatewayEvent::Contract(contract) = event {
            self.on_contract(contract.clone());
        }
    }

    /// Add or update a contract in the cache
    pub fn on_contract(&self, contract: ContractData) {
        let vt_symbol = contract.vt_symbol();
        let mut contracts = self.contracts.write().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.insert(vt_symbol, contract);
    }

    // ========================================================================
    // Lookup
    // ========================================================================

    /// Get contract data by vt_symbol
    pub fn get_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        let contracts = self.contracts.read().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.get(vt_symbol).cloned()
    }

    /// Get all cached contracts
    pub fn get_all_contracts(&self) -> Vec<ContractData> {
        let contracts = self.contracts.read().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.values().cloned().collect()
    }

    /// Get the number of cached contracts
    pub fn len(&self) -> usize {
        let contracts = self.contracts.read().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.len()
    }

    /// Check if no contracts are cached
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the price tick for a symbol
    pub fn get_pricetick(&self, vt_symbol: &str) -> Option<f64> {
        self.get_contract(vt_symbol).map(|c| c.pricetick)
    }

    /// Get the size multiplier for a symbol
    pub fn get_size(&self, vt_symbol: &str) -> Option<f64> {
        self.get_contract(vt_symbol).map(|c| c.size)
    }

    /// Get the min volume for a symbol
    pub fn get_min_volume(&self, vt_symbol: &str) -> Option<f64> {
        self.get_contract(vt_symbol).map(|c| c.min_volume)
    }

    // ========================================================================
    // Price / Volume rounding
    // ========================================================================

    /// Round a price to the nearest valid tick for the given symbol.
    ///
    /// Returns the rounded price, or `None` if the symbol is not found
    /// or the pricetick is zero.
    pub fn round_to_price_tick(&self, vt_symbol: &str, price: f64) -> Option<f64> {
        let pricetick = self.get_pricetick(vt_symbol)?;
        if pricetick <= 0.0 {
            return None;
        }
        let rounded = (price / pricetick).round() * pricetick;
        Some(rounded)
    }

    /// Round a volume to the min_volume step for the given symbol.
    ///
    /// Returns the rounded volume, or `None` if the symbol is not found
    /// or the min_volume is zero.
    pub fn round_to_volume(&self, vt_symbol: &str, volume: f64) -> Option<f64> {
        let min_vol = self.get_min_volume(vt_symbol)?;
        if min_vol <= 0.0 {
            return None;
        }
        let rounded = (volume / min_vol).floor() * min_vol;
        Some(rounded)
    }

    // ========================================================================
    // Order validation
    // ========================================================================

    /// Validate an order request against contract specifications.
    ///
    /// Checks:
    /// - Contract exists for the symbol
    /// - Price is properly rounded to tick
    /// - Volume is properly rounded to min_volume step
    /// - Volume is within min/max bounds
    ///
    /// Returns `Ok(())` if valid, or `Err(reason)` with a description.
    pub fn validate_order(&self, req: &OrderRequest) -> Result<(), String> {
        let vt_symbol = req.vt_symbol();

        let contract = self.get_contract(&vt_symbol).ok_or_else(|| {
            format!("合约 {} 不存在，无法验证委托", vt_symbol)
        })?;

        // Validate price rounding
        if contract.pricetick > 0.0 && req.price > 0.0 {
            let expected = (req.price / contract.pricetick).round() * contract.pricetick;
            let diff = (req.price - expected).abs();
            if diff > contract.pricetick * 0.01 {
                return Err(format!(
                    "价格 {} 不符合最小变动价位 {} (合约 {})",
                    req.price, contract.pricetick, vt_symbol
                ));
            }
        }

        // Validate min volume
        if contract.min_volume > 0.0 && req.volume > 0.0 && req.volume < contract.min_volume {
            return Err(format!(
                "数量 {} 低于最小下单量 {} (合约 {})",
                req.volume, contract.min_volume, vt_symbol
            ));
        }

        // Validate max volume (if specified)
        if let Some(max_vol) = contract.max_volume {
            if req.volume > max_vol {
                return Err(format!(
                    "数量 {} 超过最大下单量 {} (合约 {})",
                    req.volume, max_vol, vt_symbol
                ));
            }
        }

        // Validate volume step (using division + rounding to avoid floating point modulo issues)
        if contract.min_volume > 0.0 && req.volume > 0.0 && req.volume >= contract.min_volume {
            let steps = (req.volume / contract.min_volume).round();
            let expected_vol = steps * contract.min_volume;
            let diff = (req.volume - expected_vol).abs();
            // Tolerance: 0.1% of min_volume or at least 1e-10
            let tolerance = (contract.min_volume * 0.001).max(1e-10);
            if diff > tolerance {
                return Err(format!(
                    "数量 {} 不符合最小下单量步长 {} (合约 {})",
                    req.volume, contract.min_volume, vt_symbol
                ));
            }
        }

        Ok(())
    }

    /// Auto-correct an order request's price and volume to valid values.
    ///
    /// Rounds price up/down to nearest tick and volume down to nearest
    /// min_volume step. Returns the corrected request, or `None` if
    /// the symbol has no contract data.
    pub fn correct_order(&self, req: &OrderRequest) -> Option<OrderRequest> {
        let vt_symbol = req.vt_symbol();
        let contract = self.get_contract(&vt_symbol)?;

        let mut corrected = req.clone();

        // Round price
        if contract.pricetick > 0.0 && corrected.price > 0.0 {
            corrected.price = (corrected.price / contract.pricetick).round() * contract.pricetick;
        }

        // Floor volume
        if contract.min_volume > 0.0 && corrected.volume > 0.0 {
            corrected.volume = (corrected.volume / contract.min_volume).floor() * contract.min_volume;
        }

        Some(corrected)
    }

    // ========================================================================
    // Manual contract registration (for testing / backtesting)
    // ========================================================================

    /// Manually register a contract
    pub fn add_contract(&self, contract: ContractData) {
        self.on_contract(contract);
    }

    /// Remove a contract from the cache
    pub fn remove_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        let mut contracts = self.contracts.write().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.remove(vt_symbol)
    }

    /// Clear all cached contracts
    pub fn clear(&self) {
        let mut contracts = self.contracts.write().unwrap_or_else(|e| {
            warn!("ContractManager lock poisoned, recovering");
            e.into_inner()
        });
        contracts.clear();
    }
}

impl Default for ContractManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BaseEngine integration
// ============================================================================

use super::engine::BaseEngine;

impl BaseEngine for ContractManager {
    fn engine_name(&self) -> &str {
        "contract_manager"
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        self.process_event(event_type, event);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Direction, Exchange, Offset, OrderType, Product};

    fn make_test_contract(vt_symbol: &str, pricetick: f64, size: f64, min_volume: f64) -> ContractData {
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        let symbol = parts.first().map(|s| s.to_string()).unwrap_or_default();
        let exchange = if parts.len() > 1 && parts[1].eq_ignore_ascii_case("BINANCE") {
            Exchange::Binance
        } else {
            Exchange::Local
        };

        ContractData::new(
            "test_gateway".to_string(),
            symbol,
            exchange,
            vt_symbol.to_string(),
            Product::Spot,
            size,
            pricetick,
        )
        .with_min_volume(min_volume)
    }

    /// Helper trait to set min_volume on ContractData (builder pattern)
    trait ContractDataExt {
        fn with_min_volume(self, min_volume: f64) -> Self;
    }

    impl ContractDataExt for ContractData {
        fn with_min_volume(mut self, min_volume: f64) -> Self {
            self.min_volume = min_volume;
            self
        }
    }

    #[test]
    fn test_contract_manager_empty() {
        let mgr = ContractManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
        assert!(mgr.get_contract("BTCUSDT.BINANCE").is_none());
    }

    #[test]
    fn test_contract_manager_add_and_get() {
        let mgr = ContractManager::new();
        let contract = make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001);

        mgr.add_contract(contract);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get_contract("BTCUSDT.BINANCE").is_some());

        let c = mgr.get_contract("BTCUSDT.BINANCE").unwrap();
        assert_eq!(c.pricetick, 0.01);
        assert_eq!(c.size, 1.0);
        assert_eq!(c.min_volume, 0.001);
    }

    #[test]
    fn test_contract_manager_remove() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        let removed = mgr.remove_contract("BTCUSDT.BINANCE");
        assert!(removed.is_some());
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_contract_manager_clear() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));
        mgr.add_contract(make_test_contract("ETHUSDT.BINANCE", 0.01, 1.0, 0.001));

        assert_eq!(mgr.len(), 2);
        mgr.clear();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_round_to_price_tick() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        // 50000.005 should round to 50000.01
        let rounded = mgr.round_to_price_tick("BTCUSDT.BINANCE", 50000.005).unwrap();
        assert!((rounded - 50000.01).abs() < 0.0001);

        // 50000.004 should round to 50000.0
        let rounded = mgr.round_to_price_tick("BTCUSDT.BINANCE", 50000.004).unwrap();
        assert!((rounded - 50000.0).abs() < 0.0001);

        // Unknown symbol
        assert!(mgr.round_to_price_tick("UNKNOWN.BINANCE", 100.0).is_none());
    }

    #[test]
    fn test_round_to_volume() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        // 0.0015 should floor to 0.001
        let rounded = mgr.round_to_volume("BTCUSDT.BINANCE", 0.0015).unwrap();
        assert!((rounded - 0.001).abs() < 0.00001);

        // 0.003 should stay 0.003
        let rounded = mgr.round_to_volume("BTCUSDT.BINANCE", 0.003).unwrap();
        assert!((rounded - 0.003).abs() < 0.00001);
    }

    #[test]
    fn test_validate_order_ok() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        let mut req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            0.01,
        );
        req.price = 50000.01;
        req.offset = Offset::None;

        assert!(mgr.validate_order(&req).is_ok());
    }

    #[test]
    fn test_validate_order_bad_price() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        let mut req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            0.001,
        );
        req.price = 50000.005; // Not aligned to 0.01 tick
        req.offset = Offset::None;

        let result = mgr.validate_order(&req);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("最小变动价位"));
    }

    #[test]
    fn test_validate_order_bad_volume() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        let mut req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            0.0005, // Below min_volume of 0.001
        );
        req.price = 50000.01;
        req.offset = Offset::None;

        let result = mgr.validate_order(&req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("低于最小下单量"));
    }

    #[test]
    fn test_validate_order_unknown_contract() {
        let mgr = ContractManager::new();

        let req = OrderRequest::new(
            "UNKNOWN".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );

        let result = mgr.validate_order(&req);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("合约") && err.contains("不存在"));
    }

    #[test]
    fn test_correct_order() {
        let mgr = ContractManager::new();
        mgr.add_contract(make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001));

        let mut req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            0.0015, // Should floor to 0.001
        );
        req.price = 50000.005; // Should round to 50000.01 (nearest)
        req.offset = Offset::None;

        let corrected = mgr.correct_order(&req).unwrap();
        assert!((corrected.price - 50000.01).abs() < 0.0001);
        assert!((corrected.volume - 0.001).abs() < 0.00001);
    }

    #[test]
    fn test_process_event_contract() {
        let mgr = ContractManager::new();
        let contract = make_test_contract("ETHUSDT.BINANCE", 0.01, 1.0, 0.001);

        // Simulate a gateway contract event
        let event = GatewayEvent::Contract(contract);
        mgr.process_event("eContract", &event);

        assert_eq!(mgr.len(), 1);
        assert!(mgr.get_contract("ETHUSDT.BINANCE").is_some());
    }

    #[test]
    fn test_process_event_non_contract_ignored() {
        let mgr = ContractManager::new();
        let tick = crate::trader::object::TickData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        let event = GatewayEvent::Tick(tick);
        mgr.process_event("eTick.BTCUSDT.BINANCE", &event);

        assert!(mgr.is_empty());
    }

    #[test]
    fn test_base_engine_impl() {
        let mgr = ContractManager::new();
        assert_eq!(mgr.engine_name(), "contract_manager");
    }

    #[test]
    fn test_max_volume_validation() {
        let mgr = ContractManager::new();
        let mut contract = make_test_contract("BTCUSDT.BINANCE", 0.01, 1.0, 0.001);
        contract.max_volume = Some(100.0);
        mgr.add_contract(contract);

        let mut req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            200.0, // Exceeds max
        );
        req.price = 50000.01;

        let result = mgr.validate_order(&req);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("超过最大下单量"), "Unexpected error: {}", err);
    }
}
