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
mod manager;
mod item;
mod widget;
mod indicator;

pub use base::*;
pub use manager::BarManager;
pub use item::{ChartItem, CandleItem, VolumeItem};
pub use widget::{ChartWidget, ChartCursor};
pub use indicator::*;
