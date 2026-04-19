//! Margin Model for Futures Trading
//!
//! Calculates initial and maintenance margin for futures positions.
//! Follows the FeeModel pattern from simulated_exchange.rs for pluggable models.
//!
//! Supported models:
//! - `LinearMarginModel`: Constant rate margin (standard USDT-M futures)
//! - `TieredMarginModel`: Binance-style tiered margin with brackets
//! - `NoMarginModel`: No margin checks (spot-style, always passes)
//! - `CannedMarginModel`: Fixed margin rate for all positions

use std::fmt;

use crate::trader::{Direction, Product};

// ============================================================================
// Margin Check Result
// ============================================================================

/// Result of a margin sufficiency check.
#[derive(Debug, Clone)]
pub struct MarginCheckResult {
    /// Whether available balance covers the initial margin
    pub is_sufficient: bool,
    /// Required initial margin in quote currency
    pub initial_margin: f64,
    /// Required maintenance margin in quote currency
    pub maintenance_margin: f64,
    /// Available balance in quote currency
    pub available_balance: f64,
    /// Reason for insufficiency (if any)
    pub reason: Option<String>,
}

impl MarginCheckResult {
    /// Create a sufficient result.
    pub fn sufficient(
        initial_margin: f64,
        maintenance_margin: f64,
        available_balance: f64,
    ) -> Self {
        Self {
            is_sufficient: true,
            initial_margin,
            maintenance_margin,
            available_balance,
            reason: None,
        }
    }

    /// Create an insufficient result with reason.
    pub fn insufficient(
        initial_margin: f64,
        maintenance_margin: f64,
        available_balance: f64,
        reason: &str,
    ) -> Self {
        Self {
            is_sufficient: false,
            initial_margin,
            maintenance_margin,
            available_balance,
            reason: Some(reason.to_string()),
        }
    }
}

// ============================================================================
// Margin Bracket (for TieredMarginModel)
// ============================================================================

/// A single bracket in a tiered margin schedule.
///
/// Each bracket defines a notional value range and the margin rates that
/// apply within that range. The `addl_margin` field accumulates initial
/// margin from previous brackets, and `addl_maintenance` accumulates
/// maintenance margin from previous brackets for fast calculation.
#[derive(Debug, Clone)]
pub struct MarginBracket {
    /// Lower bound of notional value (inclusive)
    pub notional_floor: f64,
    /// Upper bound of notional value (exclusive, f64::MAX for last bracket)
    pub notional_cap: f64,
    /// Initial margin rate for this bracket
    pub initial_rate: f64,
    /// Maintenance margin rate for this bracket
    pub maintenance_rate: f64,
    /// Cumulative additional initial margin from previous brackets
    pub addl_margin: f64,
    /// Cumulative additional maintenance margin from previous brackets
    pub addl_maintenance: f64,
}

impl MarginBracket {
    /// Create a new margin bracket.
    pub fn new(
        notional_floor: f64,
        notional_cap: f64,
        initial_rate: f64,
        maintenance_rate: f64,
        addl_margin: f64,
    ) -> Self {
        Self {
            notional_floor,
            notional_cap,
            initial_rate,
            maintenance_rate,
            addl_margin,
            addl_maintenance: 0.0,
        }
    }

    /// Check if a notional value falls within this bracket.
    pub fn contains(&self, notional: f64) -> bool {
        notional >= self.notional_floor && notional < self.notional_cap
    }
}

// ============================================================================
// Margin Model Trait
// ============================================================================

/// Trait for margin calculation models.
///
/// Follows the same pluggable pattern as `FeeModel` from simulated_exchange.rs.
/// Each model calculates initial and maintenance margin, and can check whether
/// available balance is sufficient for a new position.
pub trait MarginModel: Send + Sync + fmt::Debug {
    /// Calculate initial margin for opening a position.
    ///
    /// Returns margin required in quote currency.
    fn initial_margin(&self, qty: f64, price: f64, direction: Direction, product: Product) -> f64;

    /// Calculate maintenance margin for an open position.
    ///
    /// Returns margin required in quote currency.
    fn maintenance_margin(&self, qty: f64, price: f64, direction: Direction, product: Product) -> f64;

    /// Check if margin is sufficient for a new order.
    fn check_margin(
        &self,
        qty: f64,
        price: f64,
        direction: Direction,
        product: Product,
        available_balance: f64,
    ) -> MarginCheckResult;

    /// Clone the model (for trait objects).
    fn clone_box(&self) -> Box<dyn MarginModel>;
}

impl Clone for Box<dyn MarginModel> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// ============================================================================
// Linear Margin Model
// ============================================================================

/// Standard linear margin model with constant rates.
///
/// Used for USDT-M futures where margin is a fixed percentage of notional value:
/// - `initial_margin = qty * price * initial_rate`
/// - `maintenance_margin = qty * price * maintenance_rate`
#[derive(Debug, Clone)]
pub struct LinearMarginModel {
    /// Initial margin rate (e.g., 0.10 = 10%)
    pub initial_rate: f64,
    /// Maintenance margin rate (e.g., 0.05 = 5%)
    pub maintenance_rate: f64,
}

impl LinearMarginModel {
    /// Create a new linear margin model.
    pub fn new(initial_rate: f64, maintenance_rate: f64) -> Self {
        Self {
            initial_rate,
            maintenance_rate,
        }
    }

    /// Default USDT-M futures margin model (10% initial, 5% maintenance).
    pub fn default_usdtm() -> Self {
        Self::new(0.10, 0.05)
    }
}

impl MarginModel for LinearMarginModel {
    fn initial_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        qty * price * self.initial_rate
    }

    fn maintenance_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        qty * price * self.maintenance_rate
    }

    fn check_margin(
        &self,
        qty: f64,
        price: f64,
        direction: Direction,
        product: Product,
        available_balance: f64,
    ) -> MarginCheckResult {
        let im = self.initial_margin(qty, price, direction, product);
        let mm = self.maintenance_margin(qty, price, direction, product);

        if available_balance >= im {
            MarginCheckResult::sufficient(im, mm, available_balance)
        } else {
            MarginCheckResult::insufficient(
                im,
                mm,
                available_balance,
                &format!(
                    "保证金不足: 需要初始保证金 {:.2}, 可用余额 {:.2}",
                    im, available_balance
                ),
            )
        }
    }

    fn clone_box(&self) -> Box<dyn MarginModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Tiered Margin Model
// ============================================================================

/// Binance-style tiered margin model with brackets.
///
/// Each bracket defines a notional value range and different margin rates.
/// Margin calculation for a given notional value `N` in bracket `B`:
/// - `margin = (N - B.notional_floor) * B.rate + B.addl_margin`
///
/// Where `addl_margin` is the cumulative margin from all previous brackets.
#[derive(Debug, Clone)]
pub struct TieredMarginModel {
    /// Ordered list of margin brackets (ascending by notional_floor)
    pub brackets: Vec<MarginBracket>,
    /// Whether to apply margin checks for non-futures products
    pub futures_only: bool,
}

impl TieredMarginModel {
    /// Create a new tiered margin model from a list of brackets.
    ///
    /// Brackets should be ordered by ascending `notional_floor`.
    /// The `addl_margin` values will be recalculated automatically.
    pub fn new(brackets: Vec<MarginBracket>) -> Self {
        let mut model = Self {
            brackets,
            futures_only: true,
        };
        model.recalc_addl_margin();
        model
    }

    /// Create with pre-computed addl_margin values (skip recalculation).
    pub fn new_raw(brackets: Vec<MarginBracket>) -> Self {
        Self {
            brackets,
            futures_only: true,
        }
    }

    /// Recalculate cumulative `addl_margin` and `addl_maintenance` for each bracket.
    fn recalc_addl_margin(&mut self) {
        // Sort brackets by notional_floor
        self.brackets.sort_by(|a, b| {
            a.notional_floor
                .partial_cmp(&b.notional_floor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut cumulative_initial = 0.0;
        let mut cumulative_maintenance = 0.0;

        for i in 0..self.brackets.len() {
            // Compute the accumulated margin from the notional range of the previous bracket
            if i > 0 {
                let prev_bracket = &self.brackets[i - 1];
                let current_floor = self.brackets[i].notional_floor;
                let range = current_floor - prev_bracket.notional_floor;
                cumulative_initial += range * prev_bracket.initial_rate;
                cumulative_maintenance += range * prev_bracket.maintenance_rate;
            }

            // Set cumulative values for this bracket
            self.brackets[i].addl_margin = cumulative_initial;
            self.brackets[i].addl_maintenance = cumulative_maintenance;
        }
    }

    /// Find the bracket that contains the given notional value.
    fn find_bracket(&self, notional: f64) -> Option<&MarginBracket> {
        self.brackets.iter().find(|b| b.contains(notional))
    }

    /// Calculate initial margin using tiered rates for a given notional value.
    fn calc_tiered_initial_margin(&self, notional: f64) -> f64 {
        match self.find_bracket(notional) {
            Some(bracket) => {
                let excess = notional - bracket.notional_floor;
                bracket.addl_margin + excess * bracket.initial_rate
            }
            None => {
                // If notional exceeds all brackets, use the last bracket
                if let Some(last) = self.brackets.last() {
                    let excess = notional - last.notional_floor;
                    last.addl_margin + excess * last.initial_rate
                } else {
                    0.0
                }
            }
        }
    }

    /// Calculate maintenance margin using tiered rates for a given notional value.
    fn calc_tiered_maintenance_margin(&self, notional: f64) -> f64 {
        match self.find_bracket(notional) {
            Some(bracket) => {
                let excess = notional - bracket.notional_floor;
                bracket.addl_maintenance + excess * bracket.maintenance_rate
            }
            None => {
                // If notional exceeds all brackets, use the last bracket
                if let Some(last) = self.brackets.last() {
                    let excess = notional - last.notional_floor;
                    last.addl_maintenance + excess * last.maintenance_rate
                } else {
                    0.0
                }
            }
        }
    }
}

impl MarginModel for TieredMarginModel {
    fn initial_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        let notional = qty * price;
        self.calc_tiered_initial_margin(notional)
    }

    fn maintenance_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        let notional = qty * price;
        self.calc_tiered_maintenance_margin(notional)
    }

    fn check_margin(
        &self,
        qty: f64,
        price: f64,
        direction: Direction,
        product: Product,
        available_balance: f64,
    ) -> MarginCheckResult {
        // Skip margin check for non-futures products if futures_only is enabled
        if self.futures_only && product != Product::Futures {
            return MarginCheckResult::sufficient(0.0, 0.0, available_balance);
        }

        let im = self.initial_margin(qty, price, direction, product);
        let mm = self.maintenance_margin(qty, price, direction, product);

        if available_balance >= im {
            MarginCheckResult::sufficient(im, mm, available_balance)
        } else {
            MarginCheckResult::insufficient(
                im,
                mm,
                available_balance,
                &format!(
                    "保证金不足: 需要初始保证金 {:.2}, 可用余额 {:.2}",
                    im, available_balance
                ),
            )
        }
    }

    fn clone_box(&self) -> Box<dyn MarginModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// No Margin Model
// ============================================================================

/// No margin checks — spot-style, always returns sufficient.
///
/// Useful for spot trading or testing scenarios where margin is not applicable.
#[derive(Debug, Clone, Default)]
pub struct NoMarginModel;

impl NoMarginModel {
    pub fn new() -> Self {
        Self
    }
}

impl MarginModel for NoMarginModel {
    fn initial_margin(&self, _qty: f64, _price: f64, _direction: Direction, _product: Product) -> f64 {
        0.0
    }

    fn maintenance_margin(&self, _qty: f64, _price: f64, _direction: Direction, _product: Product) -> f64 {
        0.0
    }

    fn check_margin(
        &self,
        _qty: f64,
        _price: f64,
        _direction: Direction,
        _product: Product,
        available_balance: f64,
    ) -> MarginCheckResult {
        MarginCheckResult::sufficient(0.0, 0.0, available_balance)
    }

    fn clone_box(&self) -> Box<dyn MarginModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Canned Margin Model
// ============================================================================

/// Fixed margin rate for all positions — simple canned model.
///
/// Uses the same rate for both initial and maintenance margin.
/// `margin = qty * price * rate`
#[derive(Debug, Clone)]
pub struct CannedMarginModel {
    /// Fixed margin rate (e.g., 0.10 = 10%)
    pub rate: f64,
}

impl CannedMarginModel {
    pub fn new(rate: f64) -> Self {
        Self { rate }
    }

    /// Default canned model with 10% margin rate.
    pub fn default_rate() -> Self {
        Self::new(0.10)
    }
}

impl MarginModel for CannedMarginModel {
    fn initial_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        qty * price * self.rate
    }

    fn maintenance_margin(&self, qty: f64, price: f64, _direction: Direction, _product: Product) -> f64 {
        qty * price * self.rate
    }

    fn check_margin(
        &self,
        qty: f64,
        price: f64,
        direction: Direction,
        product: Product,
        available_balance: f64,
    ) -> MarginCheckResult {
        let im = self.initial_margin(qty, price, direction, product);
        let mm = self.maintenance_margin(qty, price, direction, product);

        if available_balance >= im {
            MarginCheckResult::sufficient(im, mm, available_balance)
        } else {
            MarginCheckResult::insufficient(
                im,
                mm,
                available_balance,
                &format!(
                    "保证金不足: 需要初始保证金 {:.2}, 可用余额 {:.2}",
                    im, available_balance
                ),
            )
        }
    }

    fn clone_box(&self) -> Box<dyn MarginModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Binance USDT-M Default Margin Model
// ============================================================================

/// Binance USDT-M futures margin model with typical bracket structure.
///
/// Uses simplified Binance-style tiers where margin rates increase
/// with position notional value to limit exchange risk exposure.
#[derive(Debug, Clone)]
pub struct BinanceUsdmMarginModel {
    inner: TieredMarginModel,
}

impl BinanceUsdmMarginModel {
    /// Create with default Binance USDT-M bracket structure.
    ///
    /// Simplified brackets based on typical Binance tier structure:
    /// - 0 - 50,000 USDT: 0.4% initial, 0.4% maintenance (low risk tier)
    /// - 50,000 - 250,000 USDT: 0.5% initial, 0.5% maintenance
    /// - 250,000 - 1,000,000 USDT: 1.0% initial, 1.0% maintenance
    /// - 1,000,000 - 5,000,000 USDT: 2.5% initial, 2.5% maintenance
    /// - 5,000,000 - 25,000,000 USDT: 5.0% initial, 5.0% maintenance
    /// - 25,000,000+ USDT: 10.0% initial, 10.0% maintenance
    ///
    /// Note: Real Binance rates vary per symbol. This is a representative model.
    pub fn default_brackets() -> Vec<MarginBracket> {
        vec![
            MarginBracket::new(0.0, 50_000.0, 0.004, 0.004, 0.0),
            MarginBracket::new(50_000.0, 250_000.0, 0.005, 0.005, 0.0),
            MarginBracket::new(250_000.0, 1_000_000.0, 0.01, 0.01, 0.0),
            MarginBracket::new(1_000_000.0, 5_000_000.0, 0.025, 0.025, 0.0),
            MarginBracket::new(5_000_000.0, 25_000_000.0, 0.05, 0.05, 0.0),
            MarginBracket::new(25_000_000.0, f64::MAX, 0.10, 0.10, 0.0),
        ]
    }

    /// Create with 20x leverage equivalent brackets (5% initial, 2.5% maintenance).
    pub fn leverage_20x() -> Self {
        let brackets = vec![
            MarginBracket::new(0.0, 50_000.0, 0.05, 0.025, 0.0),
            MarginBracket::new(50_000.0, 250_000.0, 0.06, 0.03, 0.0),
            MarginBracket::new(250_000.0, 1_000_000.0, 0.10, 0.05, 0.0),
            MarginBracket::new(1_000_000.0, 5_000_000.0, 0.125, 0.0625, 0.0),
            MarginBracket::new(5_000_000.0, 25_000_000.0, 0.25, 0.125, 0.0),
            MarginBracket::new(25_000_000.0, f64::MAX, 0.50, 0.25, 0.0),
        ];
        Self {
            inner: TieredMarginModel::new(brackets),
        }
    }

    /// Create with 10x leverage equivalent brackets (10% initial, 5% maintenance).
    pub fn leverage_10x() -> Self {
        let brackets = vec![
            MarginBracket::new(0.0, 100_000.0, 0.10, 0.05, 0.0),
            MarginBracket::new(100_000.0, 500_000.0, 0.12, 0.06, 0.0),
            MarginBracket::new(500_000.0, 2_000_000.0, 0.15, 0.075, 0.0),
            MarginBracket::new(2_000_000.0, 10_000_000.0, 0.25, 0.125, 0.0),
            MarginBracket::new(10_000_000.0, f64::MAX, 0.50, 0.25, 0.0),
        ];
        Self {
            inner: TieredMarginModel::new(brackets),
        }
    }
}

impl Default for BinanceUsdmMarginModel {
    fn default() -> Self {
        let brackets = Self::default_brackets();
        Self {
            inner: TieredMarginModel::new(brackets),
        }
    }
}

impl MarginModel for BinanceUsdmMarginModel {
    fn initial_margin(&self, qty: f64, price: f64, direction: Direction, product: Product) -> f64 {
        self.inner.initial_margin(qty, price, direction, product)
    }

    fn maintenance_margin(&self, qty: f64, price: f64, direction: Direction, product: Product) -> f64 {
        self.inner.maintenance_margin(qty, price, direction, product)
    }

    fn check_margin(
        &self,
        qty: f64,
        price: f64,
        direction: Direction,
        product: Product,
        available_balance: f64,
    ) -> MarginCheckResult {
        self.inner
            .check_margin(qty, price, direction, product, available_balance)
    }

    fn clone_box(&self) -> Box<dyn MarginModel> {
        Box::new(self.clone())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // === Linear Margin Model Tests ===

    #[test]
    fn test_linear_initial_margin() {
        let model = LinearMarginModel::new(0.10, 0.05);
        // 1 BTC * $50,000 * 10% = $5,000
        let im = model.initial_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((im - 5_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_linear_maintenance_margin() {
        let model = LinearMarginModel::new(0.10, 0.05);
        // 1 BTC * $50,000 * 5% = $2,500
        let mm = model.maintenance_margin(1.0, 50_000.0, Direction::Short, Product::Futures);
        assert!((mm - 2_500.0).abs() < 1e-6);
    }

    #[test]
    fn test_linear_margin_check_pass() {
        let model = LinearMarginModel::new(0.10, 0.05);
        let result = model.check_margin(1.0, 50_000.0, Direction::Long, Product::Futures, 10_000.0);
        assert!(result.is_sufficient);
        assert!((result.initial_margin - 5_000.0).abs() < 1e-6);
        assert!((result.maintenance_margin - 2_500.0).abs() < 1e-6);
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_linear_margin_check_fail() {
        let model = LinearMarginModel::new(0.10, 0.05);
        let result = model.check_margin(1.0, 50_000.0, Direction::Long, Product::Futures, 3_000.0);
        assert!(!result.is_sufficient);
        assert!((result.initial_margin - 5_000.0).abs() < 1e-6);
        assert!(result.reason.is_some());
        assert!(result.reason.as_ref().map_or(false, |r| r.contains("保证金不足")));
    }

    // === Tiered Margin Model Tests ===

    #[test]
    fn test_tiered_margin_first_bracket() {
        let brackets = vec![
            MarginBracket::new(0.0, 50_000.0, 0.05, 0.025, 0.0),
            MarginBracket::new(50_000.0, 250_000.0, 0.10, 0.05, 0.0),
            MarginBracket::new(250_000.0, f64::MAX, 0.20, 0.10, 0.0),
        ];
        let model = TieredMarginModel::new(brackets);

        // Notional = 30,000 in first bracket
        // initial_margin = (30,000 - 0) * 0.05 + 0 = 1,500
        let im = model.initial_margin(1.0, 30_000.0, Direction::Long, Product::Futures);
        assert!((im - 1_500.0).abs() < 1e-6);
    }

    #[test]
    fn test_tiered_margin_second_bracket() {
        let brackets = vec![
            MarginBracket::new(0.0, 50_000.0, 0.05, 0.025, 0.0),
            MarginBracket::new(50_000.0, 250_000.0, 0.10, 0.05, 0.0),
            MarginBracket::new(250_000.0, f64::MAX, 0.20, 0.10, 0.0),
        ];
        let model = TieredMarginModel::new(brackets);

        // Notional = 100,000 in second bracket
        // addl_margin for second bracket = (50,000 - 0) * 0.05 = 2,500
        // initial_margin = (100,000 - 50,000) * 0.10 + 2,500 = 5,000 + 2,500 = 7,500
        let im = model.initial_margin(1.0, 100_000.0, Direction::Long, Product::Futures);
        assert!((im - 7_500.0).abs() < 1e-6);
    }

    #[test]
    fn test_tiered_margin_futures_only_skips_spot() {
        let brackets = vec![
            MarginBracket::new(0.0, 50_000.0, 0.05, 0.025, 0.0),
        ];
        let model = TieredMarginModel::new(brackets);

        let result = model.check_margin(1.0, 50_000.0, Direction::Long, Product::Spot, 100.0);
        // Spot product should pass when futures_only is true
        assert!(result.is_sufficient);
    }

    // === No Margin Model Tests ===

    #[test]
    fn test_no_margin_always_sufficient() {
        let model = NoMarginModel::new();
        let result = model.check_margin(100.0, 50_000.0, Direction::Long, Product::Spot, 0.0);
        assert!(result.is_sufficient);
        assert!((result.initial_margin - 0.0).abs() < 1e-10);
        assert!((result.maintenance_margin - 0.0).abs() < 1e-10);
    }

    // === Canned Margin Model Tests ===

    #[test]
    fn test_canned_margin_calculation() {
        let model = CannedMarginModel::new(0.10);
        // 1 BTC * $50,000 * 10% = $5,000
        let im = model.initial_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((im - 5_000.0).abs() < 1e-6);
        // Canned uses same rate for both initial and maintenance
        let mm = model.maintenance_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((mm - 5_000.0).abs() < 1e-6);
    }

    // === Binance USDT-M Default Tests ===

    #[test]
    fn test_binance_usdm_default_first_tier() {
        let model = BinanceUsdmMarginModel::default();
        // Notional = 10,000 in first bracket (0.4% rate)
        // initial_margin = 10,000 * 0.004 = 40
        let im = model.initial_margin(1.0, 10_000.0, Direction::Long, Product::Futures);
        assert!((im - 40.0).abs() < 1e-4);
    }

    #[test]
    fn test_binance_usdm_leverage_20x() {
        let model = BinanceUsdmMarginModel::leverage_20x();
        // Notional = 10,000 in first bracket (5% rate)
        // initial_margin = 10,000 * 0.05 = 500
        let im = model.initial_margin(1.0, 10_000.0, Direction::Long, Product::Futures);
        assert!((im - 500.0).abs() < 1e-4);
    }

    // === Edge Case Tests ===

    #[test]
    fn test_zero_qty_initial_margin() {
        let model = LinearMarginModel::new(0.10, 0.05);
        let im = model.initial_margin(0.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((im - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_zero_price_initial_margin() {
        let model = LinearMarginModel::new(0.10, 0.05);
        let im = model.initial_margin(1.0, 0.0, Direction::Long, Product::Futures);
        assert!((im - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_zero_qty_margin_check() {
        let model = LinearMarginModel::new(0.10, 0.05);
        let result = model.check_margin(0.0, 50_000.0, Direction::Long, Product::Futures, 100.0);
        assert!(result.is_sufficient);
    }

    #[test]
    fn test_margin_check_result_insufficient() {
        let result = MarginCheckResult::insufficient(1000.0, 500.0, 200.0, "test reason");
        assert!(!result.is_sufficient);
        assert!((result.initial_margin - 1000.0).abs() < 1e-10);
        assert!((result.maintenance_margin - 500.0).abs() < 1e-10);
        assert!((result.available_balance - 200.0).abs() < 1e-10);
        assert_eq!(result.reason.as_deref(), Some("test reason"));
    }

    #[test]
    fn test_margin_model_clone_box() {
        let model: Box<dyn MarginModel> = Box::new(LinearMarginModel::new(0.10, 0.05));
        let cloned = model.clone();
        let im = cloned.initial_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((im - 5_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_tiered_margin_exceeds_all_brackets() {
        let brackets = vec![
            MarginBracket::new(0.0, 100_000.0, 0.05, 0.025, 0.0),
            MarginBracket::new(100_000.0, 500_000.0, 0.10, 0.05, 0.0),
        ];
        let model = TieredMarginModel::new(brackets);

        // Notional = 1,000,000 exceeds all brackets, should use last bracket
        let im = model.initial_margin(1.0, 1_000_000.0, Direction::Long, Product::Futures);
        // Last bracket: addl_margin = (100,000 - 0) * 0.05 = 5,000
        // im = (1,000,000 - 100,000) * 0.10 + 5,000 = 90,000 + 5,000 = 95,000
        assert!((im - 95_000.0).abs() < 1e-4);
    }

    #[test]
    fn test_margin_bracket_contains() {
        let bracket = MarginBracket::new(100.0, 500.0, 0.05, 0.025, 0.0);
        assert!(!bracket.contains(50.0));
        assert!(bracket.contains(100.0));
        assert!(bracket.contains(250.0));
        assert!(!bracket.contains(500.0));
        assert!(!bracket.contains(600.0));
    }

    #[test]
    fn test_tiered_maintenance_margin_second_bracket() {
        let brackets = vec![
            MarginBracket::new(0.0, 50_000.0, 0.05, 0.025, 0.0),
            MarginBracket::new(50_000.0, 250_000.0, 0.10, 0.05, 0.0),
            MarginBracket::new(250_000.0, f64::MAX, 0.20, 0.10, 0.0),
        ];
        let model = TieredMarginModel::new(brackets);

        // Notional = 100,000 in second bracket
        // addl_maintenance for second bracket = (50,000 - 0) * 0.025 = 1,250
        // maintenance_margin = (100,000 - 50,000) * 0.05 + 1,250 = 2,500 + 1,250 = 3,750
        let mm = model.maintenance_margin(1.0, 100_000.0, Direction::Long, Product::Futures);
        assert!((mm - 3_750.0).abs() < 1e-6);
    }

    #[test]
    fn test_binance_usdm_leverage_10x() {
        let model = BinanceUsdmMarginModel::leverage_10x();
        // Notional = 50,000 in first bracket (10% initial, 5% maintenance)
        let im = model.initial_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((im - 5_000.0).abs() < 1e-4);

        let mm = model.maintenance_margin(1.0, 50_000.0, Direction::Long, Product::Futures);
        assert!((mm - 2_500.0).abs() < 1e-4);
    }
}
