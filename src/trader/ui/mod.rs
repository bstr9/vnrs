//! UI module for the trading platform.
//!
//! This module provides a graphical user interface for the trading engine using egui.
//! It includes various widgets for monitoring market data, orders, trades, positions,
//! and accounts, as well as a trading panel for manual order entry.

pub mod widget;
pub mod trading;
pub mod dialogs;
pub mod main_window;
pub mod style;
pub mod backtesting_panel;
pub mod strategy_panel;
pub mod dashboard;
pub mod bracket_panel;
pub mod indicator_panel;
pub mod advanced_orders_panel;
#[cfg(feature = "alpha")]
pub mod alpha_panel;

// Re-export commonly used types
pub use widget::*;
pub use trading::TradingWidget;
pub use dialogs::*;
pub use main_window::MainWindow;
pub use style::*;
pub use backtesting_panel::{BacktestingPanel, FillModelType};
pub use strategy_panel::StrategyPanel;
pub use dashboard::{DashboardPanel, DashboardAction};
pub use bracket_panel::BracketOrderPanel;
pub use indicator_panel::IndicatorPanel;
pub use advanced_orders_panel::AdvancedOrdersPanel;
#[cfg(feature = "alpha")]
pub use alpha_panel::AlphaPanel;
