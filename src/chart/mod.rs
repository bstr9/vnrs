//! Chart module for displaying candlestick charts and volume.
//!
//! This module provides:
//! - `BarManager` - Data management for bar data with datetime indexing
//! - `CandleItem` - Candlestick chart rendering
//! - `VolumeItem` - Volume bar chart rendering
//! - `ChartWidget` - Main chart widget with cursor and zoom support
//!
//! # Example
//!
//! ```ignore
//! use trade_engine::chart::{ChartWidget, BarManager};
//! use trade_engine::trader::object::BarData;
//!
//! let mut chart = ChartWidget::new();
//! chart.update_history(bars);
//! ```

mod base;
mod indicator;
mod item;
mod manager;
mod widget;

pub use base::*;
pub use indicator::*;
pub use item::{CandleItem, ChartItem, VolumeItem};
pub use manager::BarManager;
// Re-export from trader module (SynchronizedBarGenerator is not a GUI component)
pub use crate::trader::{SynchronizedBarGenerator, SynchronizedBars};
pub use widget::{ChartCursor, ChartEvent, ChartWidget};
