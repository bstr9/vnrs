//! Python integration for the trading engine
//! Provides interfaces for Python-based trading strategies

#[cfg(feature = "python")]
pub mod strategy;
#[cfg(feature = "python")]
pub mod engine;
#[cfg(feature = "python")]
pub mod data_converter;
#[cfg(feature = "python")]
pub mod bindings;
#[cfg(feature = "python")]
pub mod strategy_bindings;
#[cfg(feature = "python")]
pub mod backtesting_bindings;
#[cfg(feature = "python")]
pub mod strategy_adapter;
#[cfg(feature = "python")]
pub mod portfolio;
#[cfg(feature = "python")]
pub mod portfolio_stats;
#[cfg(feature = "python")]
pub mod order_factory;
#[cfg(feature = "python")]
pub mod message_bus;
#[cfg(feature = "python")]
pub mod risk_manager;
#[cfg(feature = "python")]
pub mod sync_bar_bindings;
#[cfg(feature = "python")]
pub mod data_types;
#[cfg(feature = "python")]
pub mod context;

#[cfg(feature = "python")]
pub use strategy::{Strategy, PendingOrder, PendingStopOrder};
#[cfg(feature = "python")]
pub use engine::{PythonEngine, PythonEngineBridge};
#[cfg(feature = "python")]
#[allow(deprecated)]
pub use strategy_bindings::{PyStrategy, PyStrategyEngine};
#[cfg(feature = "python")]
pub use backtesting_bindings::{PyBacktestingEngine, PyBarData, PyBacktestingStatistics};
#[cfg(feature = "python")]
pub use strategy_adapter::{PythonStrategyAdapter, load_strategies_from_directory};
#[cfg(feature = "python")]
pub use portfolio::{PortfolioFacade, PyPosition, PositionSnapshot, PortfolioState};
#[cfg(feature = "python")]
pub use portfolio_stats::PyPortfolioStatistics;
#[cfg(feature = "python")]
pub use order_factory::{PyOrder, OrderFactory};
#[cfg(feature = "python")]
pub use message_bus::{MessageBus, PyMessage, Message, MessageBusInner};
#[cfg(feature = "python")]
pub use risk_manager::{PyRiskManager, PyRiskConfig, PyRiskCheckResult};
#[cfg(feature = "python")]
pub use bindings::StrategyEngineHandle;
#[cfg(feature = "python")]
pub use sync_bar_bindings::{PySyncBarGenerator, PySynchronizedBars};
#[cfg(feature = "python")]
pub use data_types::{PyDepthData, PyTickData, PyOrderData, PyTradeData};
#[cfg(feature = "python")]
pub use context::PyStrategyContext;

/// Setup Python sys.path for embedded interpreter.
///
/// When running as a native binary (not in a Python venv), the embedded interpreter
/// needs to know where to find:
/// 1. The `trade_engine` native module (registered via append_to_inittab)
/// 2. The project's `.venv/site-packages` for pure-Python dependencies
/// 3. The `strategies/` directory for strategy files
///
/// This function adds these paths to sys.path so that Python strategies can
/// do `from trade_engine import CtaStrategy` and `from cta_utils import BarGenerator`.
#[cfg(feature = "python")]
pub fn setup_embedded_python_path() -> Result<(), String> {
    use pyo3::prelude::*;

    Python::attach(|py| {
        let sys = py.import("sys")
            .map_err(|e| format!("Failed to import sys: {}", e))?;
        let path = sys.getattr("path")
            .map_err(|e| format!("Failed to get sys.path: {}", e))?;

        // Add project root directory (where strategies/ is located)
        // Try to detect from executable location, falling back to current dir
        let project_root = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        
        // Add strategies directory
        let strategies_dir = project_root.join("strategies");
        if strategies_dir.exists() {
            let strategies_str = strategies_dir.to_string_lossy().to_string();
            path.call_method1("append", (strategies_str,))
                .map_err(|e| format!("Failed to add strategies to sys.path: {}", e))?;
        }

        // Add .venv/Lib/site-packages (Windows) or .venv/lib/python*/site-packages (Unix)
        let venv_site_packages = project_root.join(".venv").join("Lib").join("site-packages");
        if venv_site_packages.exists() {
            let venv_str = venv_site_packages.to_string_lossy().to_string();
            path.call_method1("append", (venv_str,))
                .map_err(|e| format!("Failed to add .venv to sys.path: {}", e))?;
        } else {
            // Try Unix-style path: .venv/lib/python3.X/site-packages
            let venv_lib_dir = project_root.join(".venv").join("lib");
            if let Ok(entries) = std::fs::read_dir(&venv_lib_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() && entry_path.file_name().map(|n| n.to_string_lossy().starts_with("python")).unwrap_or(false) {
                        let site_packages = entry_path.join("site-packages");
                        if site_packages.exists() {
                            let site_str = site_packages.to_string_lossy().to_string();
                            path.call_method1("append", (site_str,))
                                .map_err(|e| format!("Failed to add .venv site-packages to sys.path: {}", e))?;
                            break;
                        }
                    }
                }
            }
        }

        Ok::<(), String>(())
    })
}
