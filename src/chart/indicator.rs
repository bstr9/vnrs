//! Technical indicators for charting.

use crate::trader::object::BarData;
use egui::{Color32, Stroke};
use std::collections::{HashMap, VecDeque};

/// Indicator display location
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorLocation {
    /// Main price chart
    Main,
    /// Sub chart below main
    Sub,
}

/// Line style for indicators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    pub fn to_stroke(&self, width: f32, color: Color32) -> Stroke {
        match self {
            LineStyle::Solid => Stroke::new(width, color),
            LineStyle::Dashed => Stroke::new(width, color), // use draw_dashed_line instead
            LineStyle::Dotted => Stroke::new(width, color), // use draw_dotted_line instead
        }
    }

    /// Draw a dashed line between consecutive points
    pub fn draw_dashed_line(
        painter: &egui::Painter,
        points: &[egui::Pos2],
        width: f32,
        color: egui::Color32,
    ) {
        if points.len() < 2 {
            return;
        }
        let dash_len = 8.0_f32;
        let gap_len = 4.0_f32;

        for window in points.windows(2) {
            let p1 = window[0];
            let p2 = window[1];
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let segment_len = dx.hypot(dy);
            if segment_len < 0.1 {
                continue;
            }
            let nx = dx / segment_len;
            let ny = dy / segment_len;

            let mut pos = 0.0_f32;
            while pos < segment_len {
                let dash_end = (pos + dash_len).min(segment_len);
                let start = egui::Pos2::new(p1.x + nx * pos, p1.y + ny * pos);
                let end = egui::Pos2::new(p1.x + nx * dash_end, p1.y + ny * dash_end);
                painter.line_segment([start, end], egui::Stroke::new(width, color));
                pos = dash_end + gap_len;
            }
        }
    }

    /// Draw a dotted line between consecutive points
    pub fn draw_dotted_line(
        painter: &egui::Painter,
        points: &[egui::Pos2],
        width: f32,
        color: egui::Color32,
    ) {
        if points.len() < 2 {
            return;
        }
        let dot_spacing = 4.0_f32;

        for window in points.windows(2) {
            let p1 = window[0];
            let p2 = window[1];
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let segment_len = dx.hypot(dy);
            if segment_len < 0.1 {
                continue;
            }
            let nx = dx / segment_len;
            let ny = dy / segment_len;

            let mut pos = 0.0_f32;
            while pos < segment_len {
                let center = egui::Pos2::new(p1.x + nx * pos, p1.y + ny * pos);
                painter.circle_filled(center, width * 0.8, color);
                pos += dot_spacing;
            }
        }
    }
}

/// Configuration for an indicator line
#[derive(Debug, Clone)]
pub struct IndicatorLineConfig {
    pub name: String,
    pub color: Color32,
    pub style: LineStyle,
    pub width: f32,
}

impl Default for IndicatorLineConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: Color32::YELLOW,
            style: LineStyle::Solid,
            width: 1.5,
        }
    }
}

/// Base state shared by all indicators
pub struct IndicatorBase {
    pub name: String,
    pub location: IndicatorLocation,
    pub has_inputs: bool,
    pub count: usize,
    pub initialized: bool,
    pub line_configs: Vec<IndicatorLineConfig>,
}

impl IndicatorBase {
    pub fn new(name: &str, location: IndicatorLocation) -> Self {
        Self {
            name: name.to_string(),
            location,
            has_inputs: false,
            count: 0,
            initialized: false,
            line_configs: Vec::new(),
        }
    }

    /// Check if we have enough inputs to be initialized
    pub fn check_initialized(&mut self, required_count: usize) -> bool {
        if !self.initialized && self.count >= required_count {
            self.initialized = true;
        }
        self.initialized
    }

    /// Common get_y_range implementation for a single value series
    pub fn get_y_range_for_values(
        values: &[Option<f64>],
        min_ix: usize,
        max_ix: usize,
    ) -> Option<(f64, f64)> {
        if values.is_empty() || min_ix > max_ix || min_ix >= values.len() {
            return None;
        }
        let end_ix = max_ix.min(values.len().saturating_sub(1));
        let min_val = values[min_ix..=end_ix]
            .iter()
            .filter_map(|&v| v)
            .fold(f64::INFINITY, f64::min);
        let max_val = values[min_ix..=end_ix]
            .iter()
            .filter_map(|&v| v)
            .fold(f64::NEG_INFINITY, f64::max);
        if min_val == f64::INFINITY || max_val == f64::NEG_INFINITY {
            None
        } else {
            Some((min_val, max_val))
        }
    }

    /// Combine multiple value series into a single y range
    pub fn get_y_range_for_multi_series(
        series_list: &[&[Option<f64>]],
        min_ix: usize,
        max_ix: usize,
    ) -> Option<(f64, f64)> {
        if min_ix > max_ix {
            return None;
        }
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for values in series_list {
            if values.is_empty() || min_ix >= values.len() {
                continue;
            }
            let end_ix = max_ix.min(values.len().saturating_sub(1));
            for val in values[min_ix..=end_ix].iter().flatten() {
                min_val = min_val.min(*val);
                max_val = max_val.max(*val);
            }
        }
        if min_val == f64::INFINITY || max_val == f64::NEG_INFINITY {
            None
        } else {
            Some((min_val, max_val))
        }
    }

    /// Reset base state
    pub fn reset_base(&mut self) {
        self.has_inputs = false;
        self.count = 0;
        self.initialized = false;
    }
}

/// Base trait for all indicators
pub trait Indicator: Send + Sync {
    /// Get indicator name
    fn name(&self) -> &str;

    /// Get display location (main or sub chart)
    fn location(&self) -> IndicatorLocation;

    /// Update indicator with a single bar (incremental, O(1))
    /// Returns true if the indicator produced a new value
    fn update(&mut self, bar: &BarData) -> bool;

    /// Check if the indicator has enough data to produce values
    fn is_ready(&self) -> bool;

    /// Get the current (latest) value of the primary series
    fn current_value(&self) -> Option<f64>;

    /// Reset indicator state (clear all computed values)
    fn reset(&mut self);

    /// Calculate indicator values for given bar data.
    /// Default implementation uses reset() + update() loop for backward compatibility.
    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    /// Get the number of result series
    fn series_count(&self) -> usize;

    /// Get values for a specific bar index and series
    fn get_value(&self, bar_index: usize, series_index: usize) -> Option<f64>;

    /// Get line configuration for a series
    fn get_line_config(&self, series_index: usize) -> Option<&IndicatorLineConfig>;

    /// Get Y-axis range for this indicator
    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)>;

    /// Get the configurable parameters of this indicator as a HashMap
    fn get_parameters(&self) -> HashMap<String, f64> {
        HashMap::new()
    }
}

/// Moving Average (MA)
pub struct MA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
    window: VecDeque<f64>,
}

impl MA {
    pub fn new(period: usize, color: Color32, location: IndicatorLocation) -> Self {
        Self {
            period,
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: format!("MA{}", period),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new(&format!("MA{}", period), location),
            window: VecDeque::new(),
        }
    }
}

impl Indicator for MA {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        let value = bar.close_price;
        self.base.count += 1;
        self.base.has_inputs = true;

        self.window.push_back(value);
        if self.window.len() > self.period {
            self.window.pop_front();
        }

        if self.window.len() >= self.period {
            let sum: f64 = self.window.iter().sum();
            self.values.push(Some(sum / self.period as f64));
            self.base.check_initialized(self.period);
        } else {
            self.values.push(None);
        }

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.window.clear();
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params
    }
}

/// Exponential Moving Average (EMA)
pub struct EMA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
    prev_ema: f64,
    initial_sum: f64,
}

impl EMA {
    pub fn new(period: usize, color: Color32, location: IndicatorLocation) -> Self {
        Self {
            period,
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: format!("EMA{}", period),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new(&format!("EMA{}", period), location),
            prev_ema: 0.0,
            initial_sum: 0.0,
        }
    }

    /// Update with a raw value (for use as a composable sub-indicator)
    pub fn update_raw(&mut self, value: f64) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;

        if self.base.count == 1 {
            self.prev_ema = value;
            self.values.push(None);
        } else if self.base.count <= self.period {
            self.initial_sum += value;
            if self.base.count == self.period {
                let seed = (self.initial_sum + self.prev_ema) / self.period as f64;
                self.prev_ema = seed;
                self.values.push(Some(seed));
                self.base.check_initialized(self.period);
            } else {
                self.values.push(None);
            }
        } else {
            let k = 2.0 / (self.period as f64 + 1.0);
            self.prev_ema = value * k + self.prev_ema * (1.0 - k);
            self.values.push(Some(self.prev_ema));
        }

        self.base.initialized
    }

    /// Get current EMA value (for use as a composable sub-indicator)
    pub fn current_ema(&self) -> Option<f64> {
        if self.base.initialized {
            Some(self.prev_ema)
        } else {
            None
        }
    }

    /// Reset EMA state (for use as a composable sub-indicator)
    pub fn reset_ema(&mut self) {
        self.values.clear();
        self.base.reset_base();
        self.prev_ema = 0.0;
        self.initial_sum = 0.0;
    }
}

impl Indicator for EMA {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.update_raw(bar.close_price)
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.reset_ema();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params
    }
}

/// Bollinger Bands (BOLL)
pub struct BOLL {
    period: usize,
    std_dev: f64,
    upper: Vec<Option<f64>>,
    middle: Vec<Option<f64>>,
    lower: Vec<Option<f64>>,
    upper_config: IndicatorLineConfig,
    middle_config: IndicatorLineConfig,
    lower_config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
    // Incremental state: Welford's running stats for the window
    window: VecDeque<f64>,
    running_count: f64,
    running_mean: f64,
    running_m2: f64,
}

impl BOLL {
    pub fn new(period: usize, std_dev: f64, location: IndicatorLocation) -> Self {
        Self {
            period,
            std_dev,
            upper: Vec::new(),
            middle: Vec::new(),
            lower: Vec::new(),
            upper_config: IndicatorLineConfig {
                name: "BOLL上轨".to_string(),
                color: Color32::from_rgb(255, 100, 100),
                style: LineStyle::Solid,
                width: 1.0,
            },
            middle_config: IndicatorLineConfig {
                name: "BOLL中轨".to_string(),
                color: Color32::from_rgb(255, 255, 100),
                style: LineStyle::Solid,
                width: 1.5,
            },
            lower_config: IndicatorLineConfig {
                name: "BOLL下轨".to_string(),
                color: Color32::from_rgb(100, 255, 100),
                style: LineStyle::Solid,
                width: 1.0,
            },
            location,
            base: IndicatorBase::new("BOLL", location),
            window: VecDeque::new(),
            running_count: 0.0,
            running_mean: 0.0,
            running_m2: 0.0,
        }
    }
}

impl Indicator for BOLL {
    fn name(&self) -> &str {
        "BOLL"
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        let value = bar.close_price;
        self.base.count += 1;
        self.base.has_inputs = true;

        // Welford's online algorithm for running mean and variance
        self.running_count += 1.0;
        let delta = value - self.running_mean;
        self.running_mean += delta / self.running_count;
        let delta2 = value - self.running_mean;
        self.running_m2 += delta * delta2;

        // Keep window for exact SMA (consistent with original behavior)
        self.window.push_back(value);
        if self.window.len() > self.period {
            let old = self
                .window
                .pop_front()
                .expect("window guaranteed non-empty since len > period");
            // Remove from running stats
            self.running_count -= 1.0;
            if self.running_count > 0.0 {
                let d = old - self.running_mean;
                self.running_mean -= d / self.running_count;
                let d2 = old - self.running_mean;
                self.running_m2 -= d * d2;
                // running_m2 can go slightly negative due to floating point
                if self.running_m2 < 0.0 {
                    self.running_m2 = 0.0;
                }
            }
        }

        if self.window.len() >= self.period {
            let mid = self.running_mean;
            let variance = if self.running_count > 1.0 {
                self.running_m2 / (self.running_count - 1.0)
            } else {
                0.0
            };
            let std = variance.sqrt();
            let upper = mid + self.std_dev * std;
            let lower = mid - self.std_dev * std;
            self.middle.push(Some(mid));
            self.upper.push(Some(upper));
            self.lower.push(Some(lower));
            self.base.check_initialized(self.period);
        } else {
            self.middle.push(None);
            self.upper.push(None);
            self.lower.push(None);
        }

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.middle.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.upper.clear();
        self.middle.clear();
        self.lower.clear();
        self.window.clear();
        self.running_count = 0.0;
        self.running_mean = 0.0;
        self.running_m2 = 0.0;
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        3
    }

    fn get_value(&self, bar_index: usize, series_index: usize) -> Option<f64> {
        match series_index {
            0 => self.upper.get(bar_index).and_then(|v| *v),
            1 => self.middle.get(bar_index).and_then(|v| *v),
            2 => self.lower.get(bar_index).and_then(|v| *v),
            _ => None,
        }
    }

    fn get_line_config(&self, series_index: usize) -> Option<&IndicatorLineConfig> {
        match series_index {
            0 => Some(&self.upper_config),
            1 => Some(&self.middle_config),
            2 => Some(&self.lower_config),
            _ => None,
        }
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_multi_series(
            &[&self.upper, &self.middle, &self.lower],
            min_ix,
            max_ix,
        )
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params.insert("multiplier".to_string(), self.std_dev);
        params
    }
}

/// Weighted Moving Average (WMA)
pub struct WMA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
    window: VecDeque<f64>,
}

impl WMA {
    pub fn new(period: usize, color: Color32, location: IndicatorLocation) -> Self {
        Self {
            period,
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: format!("WMA{}", period),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new(&format!("WMA{}", period), location),
            window: VecDeque::new(),
        }
    }
}

impl Indicator for WMA {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        let value = bar.close_price;
        self.base.count += 1;
        self.base.has_inputs = true;

        self.window.push_back(value);
        if self.window.len() > self.period {
            self.window.pop_front();
        }

        if self.window.len() >= self.period {
            let weight_sum: f64 = (1..=self.period).map(|i| i as f64).sum();
            let weighted_sum: f64 = self
                .window
                .iter()
                .enumerate()
                .map(|(j, &v)| v * ((j + 1) as f64))
                .sum();
            self.values.push(Some(weighted_sum / weight_sum));
            self.base.check_initialized(self.period);
        } else {
            self.values.push(None);
        }

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.window.clear();
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params
    }
}

/// Volume Weighted Average Price (VWAP)
pub struct VWAP {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
    cumulative_pv: f64,
    cumulative_v: f64,
}

impl VWAP {
    pub fn new(color: Color32, location: IndicatorLocation) -> Self {
        Self {
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: "VWAP".to_string(),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new("VWAP", location),
            cumulative_pv: 0.0,
            cumulative_v: 0.0,
        }
    }
}

impl Indicator for VWAP {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;

        let typical_price = (bar.high_price + bar.low_price + bar.close_price) / 3.0;
        self.cumulative_pv += typical_price * bar.volume;
        self.cumulative_v += bar.volume;

        if self.cumulative_v > 0.0 {
            self.values
                .push(Some(self.cumulative_pv / self.cumulative_v));
            self.base.check_initialized(1);
        } else {
            self.values.push(None);
        }

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.cumulative_pv = 0.0;
        self.cumulative_v = 0.0;
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }
}

/// Parabolic SAR (Stop and Reverse)
pub struct SAR {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    acceleration: f64,
    maximum: f64,
    base: IndicatorBase,
    // Incremental state
    is_long: bool,
    sar: f64,
    ep: f64,
    af: f64,
    prev_low: Option<f64>,
    prev_prev_low: Option<f64>,
    prev_high: Option<f64>,
    prev_prev_high: Option<f64>,
}

impl SAR {
    pub fn new(
        acceleration: f64,
        maximum: f64,
        color: Color32,
        location: IndicatorLocation,
    ) -> Self {
        Self {
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: "SAR".to_string(),
                color,
                style: LineStyle::Solid,
                width: 2.0,
            },
            location,
            acceleration,
            maximum,
            base: IndicatorBase::new("SAR", location),
            is_long: true,
            sar: 0.0,
            ep: 0.0,
            af: acceleration,
            prev_low: None,
            prev_prev_low: None,
            prev_high: None,
            prev_prev_high: None,
        }
    }
}

impl Indicator for SAR {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        let high = bar.high_price;
        let low = bar.low_price;

        if self.base.count == 0 {
            // First bar: initialize
            self.sar = low;
            self.ep = high;
            self.af = self.acceleration;
            self.is_long = true;
            self.values.push(Some(self.sar));
            self.base.count += 1;
            self.base.has_inputs = true;
            self.base.check_initialized(2);
            // Store for next iteration
            self.prev_prev_low = self.prev_low;
            self.prev_low = Some(low);
            self.prev_prev_high = self.prev_high;
            self.prev_high = Some(high);
            return self.base.initialized;
        }

        // Compute SAR
        self.sar = self.sar + self.af * (self.ep - self.sar);

        if self.is_long {
            // Constrain SAR by previous lows
            if let Some(pl) = self.prev_low {
                self.sar = self.sar.min(pl);
            }
            if let Some(ppl) = self.prev_prev_low {
                self.sar = self.sar.min(ppl);
            }

            if low < self.sar {
                // Flip to short
                self.is_long = false;
                self.sar = self.ep;
                self.ep = low;
                self.af = self.acceleration;
            } else if high > self.ep {
                self.ep = high;
                self.af = (self.af + self.acceleration).min(self.maximum);
            }
        } else {
            // Constrain SAR by previous highs
            if let Some(ph) = self.prev_high {
                self.sar = self.sar.max(ph);
            }
            if let Some(pph) = self.prev_prev_high {
                self.sar = self.sar.max(pph);
            }

            if high > self.sar {
                // Flip to long
                self.is_long = true;
                self.sar = self.ep;
                self.ep = high;
                self.af = self.acceleration;
            } else if low < self.ep {
                self.ep = low;
                self.af = (self.af + self.acceleration).min(self.maximum);
            }
        }

        self.values.push(Some(self.sar));
        self.base.count += 1;
        self.base.check_initialized(2);

        // Shift previous values
        self.prev_prev_low = self.prev_low;
        self.prev_low = Some(low);
        self.prev_prev_high = self.prev_high;
        self.prev_high = Some(high);

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.is_long = true;
        self.sar = 0.0;
        self.ep = 0.0;
        self.af = self.acceleration;
        self.prev_low = None;
        self.prev_prev_low = None;
        self.prev_high = None;
        self.prev_prev_high = None;
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("multiplier".to_string(), self.acceleration);
        params.insert("max_af".to_string(), self.maximum);
        params
    }
}

/// Average Price Line (AVL)
pub struct AVL {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
}

impl AVL {
    pub fn new(color: Color32, location: IndicatorLocation) -> Self {
        Self {
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: "AVL".to_string(),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new("AVL", location),
        }
    }
}

impl Indicator for AVL {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;

        let avg_price = (bar.high_price + bar.low_price + bar.close_price) / 3.0;
        self.values.push(Some(avg_price));
        self.base.check_initialized(1);

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }
}

/// Internal EMA state for composition (tracks state without allocating value vectors)
struct EmaState {
    period: usize,
    prev_ema: f64,
    initial_sum: f64,
    count: usize,
    initialized: bool,
}

impl EmaState {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev_ema: 0.0,
            initial_sum: 0.0,
            count: 0,
            initialized: false,
        }
    }

    /// Returns Some(ema_value) when the EMA has enough data, None otherwise.
    /// The returned value is the NEW EMA output (only produced after initialization).
    fn update_raw(&mut self, value: f64) -> Option<f64> {
        self.count += 1;

        if self.count == 1 {
            self.prev_ema = value;
            None
        } else if self.count <= self.period {
            self.initial_sum += value;
            if self.count == self.period {
                let seed = (self.initial_sum + self.prev_ema) / self.period as f64;
                self.prev_ema = seed;
                self.initialized = true;
                Some(seed)
            } else {
                None
            }
        } else {
            let k = 2.0 / (self.period as f64 + 1.0);
            self.prev_ema = value * k + self.prev_ema * (1.0 - k);
            Some(self.prev_ema)
        }
    }

    fn reset(&mut self) {
        self.prev_ema = 0.0;
        self.initial_sum = 0.0;
        self.count = 0;
        self.initialized = false;
    }

    #[allow(dead_code)]
    fn current(&self) -> Option<f64> {
        if self.initialized {
            Some(self.prev_ema)
        } else {
            None
        }
    }
}

/// Triple Exponential Smoothed Moving Average (TRIX)
/// Uses composition: three internal EMA states + signal EMA state
pub struct TRIX {
    values: Vec<Option<f64>>,
    signal_values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    signal_config: IndicatorLineConfig,
    location: IndicatorLocation,
    period: usize,
    _signal_period: usize,
    base: IndicatorBase,
    // Composition: three EMA states + signal EMA
    ema1: EmaState,
    ema2: EmaState,
    ema3: EmaState,
    signal_ema: EmaState,
    // Track previous EMA3 output for TRIX calculation
    prev_ema3: Option<f64>,
}

impl TRIX {
    pub fn new(
        period: usize,
        signal_period: usize,
        color: Color32,
        signal_color: Color32,
        location: IndicatorLocation,
    ) -> Self {
        Self {
            values: Vec::new(),
            signal_values: Vec::new(),
            config: IndicatorLineConfig {
                name: "TRIX".to_string(),
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            signal_config: IndicatorLineConfig {
                name: "SIGNAL".to_string(),
                color: signal_color,
                style: LineStyle::Dashed,
                width: 1.0,
            },
            location,
            period,
            _signal_period: signal_period,
            base: IndicatorBase::new("TRIX", location),
            ema1: EmaState::new(period),
            ema2: EmaState::new(period),
            ema3: EmaState::new(period),
            signal_ema: EmaState::new(signal_period),
            prev_ema3: None,
        }
    }
}

impl Indicator for TRIX {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;

        let value = bar.close_price;

        // Feed through three EMA layers (composition pattern)
        let ema1_val = self.ema1.update_raw(value);

        let ema2_val = match ema1_val {
            Some(v1) => self.ema2.update_raw(v1),
            None => None,
        };

        let ema3_val = match ema2_val {
            Some(v2) => self.ema3.update_raw(v2),
            None => None,
        };

        // TRIX = ((EMA3[i] - EMA3[i-1]) / EMA3[i-1]) * 10000
        let trix_val = match (ema3_val, self.prev_ema3) {
            (Some(curr), Some(prev)) if prev.abs() > 1e-10 => {
                Some(((curr - prev) / prev) * 10000.0)
            }
            _ => None,
        };

        // Update prev_ema3 for next iteration
        if ema3_val.is_some() {
            self.prev_ema3 = ema3_val;
        }

        // Update signal line (EMA of TRIX values)
        let signal_val = match trix_val {
            Some(tv) => self.signal_ema.update_raw(tv),
            None => None,
        };

        self.values.push(trix_val);
        self.signal_values.push(signal_val);

        // TRIX requires period*3 bars for first EMA3 output, plus 1 more for rate of change
        let required = self.period * 3 + 1;
        if trix_val.is_some() {
            self.base.check_initialized(required);
        }

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.signal_values.clear();
        self.ema1.reset();
        self.ema2.reset();
        self.ema3.reset();
        self.signal_ema.reset();
        self.prev_ema3 = None;
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        2 // TRIX line + Signal line
    }

    fn get_value(&self, bar_index: usize, series_index: usize) -> Option<f64> {
        match series_index {
            0 => self.values.get(bar_index).and_then(|v| *v),
            1 => self.signal_values.get(bar_index).and_then(|v| *v),
            _ => None,
        }
    }

    fn get_line_config(&self, series_index: usize) -> Option<&IndicatorLineConfig> {
        match series_index {
            0 => Some(&self.config),
            1 => Some(&self.signal_config),
            _ => None,
        }
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_multi_series(
            &[&self.values, &self.signal_values],
            min_ix,
            max_ix,
        )
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params.insert("signal_period".to_string(), self._signal_period as f64);
        params
    }
}

/// SuperTrend Indicator
pub struct SUPER {
    trend_values: Vec<Option<f64>>,
    trend_direction: Vec<Option<i32>>,
    config_up: IndicatorLineConfig,
    config_down: IndicatorLineConfig,
    location: IndicatorLocation,
    period: usize,
    multiplier: f64,
    base: IndicatorBase,
    // Incremental state
    tr_window: VecDeque<f64>,
    atr_value: Option<f64>,
    // Previous bar data for TR calculation
    prev_close: Option<f64>,
    // Previous final bands for continuity
    prev_final_upper: Option<f64>,
    prev_final_lower: Option<f64>,
    prev_direction: Option<i32>,
}

impl SUPER {
    pub fn new(
        period: usize,
        multiplier: f64,
        up_color: Color32,
        down_color: Color32,
        location: IndicatorLocation,
    ) -> Self {
        Self {
            trend_values: Vec::new(),
            trend_direction: Vec::new(),
            config_up: IndicatorLineConfig {
                name: "SUPER_UP".to_string(),
                color: up_color,
                style: LineStyle::Solid,
                width: 2.0,
            },
            config_down: IndicatorLineConfig {
                name: "SUPER_DOWN".to_string(),
                color: down_color,
                style: LineStyle::Solid,
                width: 2.0,
            },
            location,
            period,
            multiplier,
            base: IndicatorBase::new("SUPER", location),
            tr_window: VecDeque::new(),
            atr_value: None,
            prev_close: None,
            prev_final_upper: None,
            prev_final_lower: None,
            prev_direction: None,
        }
    }
}

impl Indicator for SUPER {
    fn name(&self) -> &str {
        "SUPER"
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;

        // Calculate True Range
        let tr = match self.prev_close {
            Some(pc) => {
                let high_low = bar.high_price - bar.low_price;
                let high_close = (bar.high_price - pc).abs();
                let low_close = (bar.low_price - pc).abs();
                high_low.max(high_close).max(low_close)
            }
            None => bar.high_price - bar.low_price,
        };

        // Maintain TR window for SMA-based ATR
        self.tr_window.push_back(tr);
        if self.tr_window.len() > self.period {
            self.tr_window.pop_front();
        }

        // Calculate ATR from window
        self.atr_value = None;
        if self.tr_window.len() >= self.period {
            let sum: f64 = self.tr_window.iter().sum();
            self.atr_value = Some(sum / self.period as f64);
        }

        // Calculate SuperTrend if we have ATR
        if let Some(atr) = self.atr_value {
            let hl_avg = (bar.high_price + bar.low_price) / 2.0;
            let basic_upper = hl_avg + self.multiplier * atr;
            let basic_lower = hl_avg - self.multiplier * atr;

            // Calculate final bands
            let final_upper = match self.prev_final_upper {
                Some(prev_fu) => {
                    if basic_upper < prev_fu || self.prev_close.is_some_and(|pc| pc > prev_fu) {
                        basic_upper
                    } else {
                        prev_fu
                    }
                }
                None => basic_upper,
            };

            let final_lower = match self.prev_final_lower {
                Some(prev_fl) => {
                    if basic_lower > prev_fl || self.prev_close.is_some_and(|pc| pc < prev_fl) {
                        basic_lower
                    } else {
                        prev_fl
                    }
                }
                None => basic_lower,
            };

            // Determine trend direction
            let (direction, trend_value) = match self.prev_direction {
                Some(1) => {
                    // Was uptrend
                    if bar.close_price < self.prev_final_lower.unwrap_or(final_lower) {
                        (-1, final_upper)
                    } else {
                        (1, final_lower)
                    }
                }
                Some(-1) => {
                    // Was downtrend
                    if bar.close_price > self.prev_final_upper.unwrap_or(final_upper) {
                        (1, final_lower)
                    } else {
                        (-1, final_upper)
                    }
                }
                _ => {
                    // Initial trend
                    if bar.close_price > final_upper {
                        (1, final_lower)
                    } else {
                        (-1, final_upper)
                    }
                }
            };

            self.trend_direction.push(Some(direction));
            self.trend_values.push(Some(trend_value));

            // Store state for next iteration
            self.prev_final_upper = Some(final_upper);
            self.prev_final_lower = Some(final_lower);
            self.prev_direction = Some(direction);

            self.base.check_initialized(self.period);
        } else {
            self.trend_direction.push(None);
            self.trend_values.push(None);
        }

        self.prev_close = Some(bar.close_price);

        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.trend_values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.trend_values.clear();
        self.trend_direction.clear();
        self.tr_window.clear();
        self.atr_value = None;
        self.prev_close = None;
        self.prev_final_upper = None;
        self.prev_final_lower = None;
        self.prev_direction = None;
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        2 // Series 0: uptrend, Series 1: downtrend
    }

    fn get_value(&self, bar_index: usize, series_index: usize) -> Option<f64> {
        let dir = self.trend_direction.get(bar_index).and_then(|v| *v);
        match (series_index, dir) {
            (0, Some(1)) => self.trend_values.get(bar_index).and_then(|v| *v),
            (1, Some(-1)) => self.trend_values.get(bar_index).and_then(|v| *v),
            _ => None,
        }
    }

    fn get_line_config(&self, series_index: usize) -> Option<&IndicatorLineConfig> {
        match series_index {
            0 => Some(&self.config_up),
            1 => Some(&self.config_down),
            _ => None,
        }
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.trend_values, min_ix, max_ix)
    }

    fn get_parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("period".to_string(), self.period as f64);
        params.insert("multiplier".to_string(), self.multiplier);
        params
    }
}

/// Custom indicator built from a user-supplied expression.
///
/// Supported variables: `open`, `high`, `low`, `close`, `volume`
/// Supported operators: `+`, `-`, `*`, `/`, `()` and numeric constants.
/// Example expressions: `close * 1.02`, `(high + low) / 2`, `volume / 1000`
pub struct CustomIndicator {
    name: String,
    expression: String,
    /// Cached AST parsed from expression, avoids re-tokenizing/re-parsing on every bar.
    parsed: Option<ExprNode>,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    base: IndicatorBase,
}

impl CustomIndicator {
    /// Parse the expression and cache the AST. Returns None if expression is invalid.
    fn parse_expression(expr: &str) -> Option<ExprNode> {
        let tokens = tokenize(expr).ok()?;
        let mut parser = ExprParser::new(&tokens);
        let result = parser.parse_expr();
        if parser.pos != tokens.len() {
            return None; // leftover tokens → parse error
        }
        result
    }

    pub fn new(
        name: String,
        expression: String,
        color: Color32,
        location: IndicatorLocation,
    ) -> Self {
        let parsed = Self::parse_expression(&expression);
        Self {
            name,
            expression,
            parsed,
            values: Vec::new(),
            config: IndicatorLineConfig {
                name: String::new(), // filled from self.name in Indicator::name()
                color,
                style: LineStyle::Solid,
                width: 1.5,
            },
            location,
            base: IndicatorBase::new("Custom", location),
        }
    }

    /// Evaluate the cached AST for a single bar.
    fn evaluate_expr(&self, bar: &BarData) -> Option<f64> {
        self.parsed.as_ref().map(|node| node.eval(bar))
    }
}

impl Indicator for CustomIndicator {
    fn name(&self) -> &str {
        &self.name
    }

    fn location(&self) -> IndicatorLocation {
        self.location
    }

    fn update(&mut self, bar: &BarData) -> bool {
        self.base.count += 1;
        self.base.has_inputs = true;
        self.values.push(self.evaluate_expr(bar));
        self.base.check_initialized(1);
        self.base.initialized
    }

    fn is_ready(&self) -> bool {
        self.base.initialized
    }

    fn current_value(&self) -> Option<f64> {
        self.values.last().and_then(|v| *v)
    }

    fn reset(&mut self) {
        self.values.clear();
        self.base.reset_base();
    }

    fn calculate(&mut self, bars: &[BarData]) {
        self.reset();
        for bar in bars {
            self.update(bar);
        }
    }

    fn series_count(&self) -> usize {
        1
    }

    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.values.get(bar_index).and_then(|v| *v)
    }

    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        Some(&self.config)
    }

    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        IndicatorBase::get_y_range_for_values(&self.values, min_ix, max_ix)
    }
}

// ---------------------------------------------------------------------------
// Simple recursive-descent expression parser
// ---------------------------------------------------------------------------

/// Tokens produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Var(String), // open, high, low, close, volume
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

/// Tokenise an expression string.
fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            '0'..='9' | '.' => {
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num_str.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let val: f64 = num_str
                    .parse()
                    .map_err(|_| format!("Invalid number: {}", num_str))?;
                tokens.push(Token::Number(val));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        ident.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match ident.as_str() {
                    "open" | "high" | "low" | "close" | "volume" => {
                        tokens.push(Token::Var(ident));
                    }
                    other => return Err(format!("Unknown variable: {}", other)),
                }
            }
            other => return Err(format!("Unexpected character: {}", other)),
        }
    }
    Ok(tokens)
}

/// AST node for the expression.
#[derive(Debug, Clone)]
enum ExprNode {
    Number(f64),
    Var(String),
    BinaryOp {
        op: BinOp,
        left: Box<ExprNode>,
        right: Box<ExprNode>,
    },
    Negate(Box<ExprNode>),
}

#[derive(Debug, Clone, Copy)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl ExprNode {
    fn eval(&self, bar: &BarData) -> f64 {
        match self {
            ExprNode::Number(v) => *v,
            ExprNode::Var(name) => match name.as_str() {
                "open" => bar.open_price,
                "high" => bar.high_price,
                "low" => bar.low_price,
                "close" => bar.close_price,
                "volume" => bar.volume,
                _ => f64::NAN,
            },
            ExprNode::BinaryOp { op, left, right } => {
                let l = left.eval(bar);
                let r = right.eval(bar);
                match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => {
                        if r.abs() < 1e-12 {
                            f64::NAN
                        } else {
                            l / r
                        }
                    }
                }
            }
            ExprNode::Negate(inner) => -inner.eval(bar),
        }
    }
}

/// Recursive-descent parser.
///
/// Grammar (precedence low→high):
///   expr   = term (('+' | '-') term)*
///   term   = unary (('*' | '/') unary)*
///   unary  = '-' unary | atom
///   atom   = NUMBER | VAR | '(' expr ')'
struct ExprParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    /// Parse a full expression.
    fn parse_expr(&mut self) -> Option<ExprNode> {
        let mut left = self.parse_term()?;
        while let Some(Token::Plus | Token::Minus) = self.peek() {
            let op = match self.advance()? {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_term()?;
            left = ExprNode::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn parse_term(&mut self) -> Option<ExprNode> {
        let mut left = self.parse_unary()?;
        while let Some(Token::Star | Token::Slash) = self.peek() {
            let op = match self.advance()? {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            left = ExprNode::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<ExprNode> {
        if let Some(Token::Minus) = self.peek() {
            self.advance();
            let inner = self.parse_unary()?;
            Some(ExprNode::Negate(Box::new(inner)))
        } else {
            self.parse_atom()
        }
    }

    fn parse_atom(&mut self) -> Option<ExprNode> {
        match self.peek()? {
            Token::Number(_) => {
                if let Some(Token::Number(v)) = self.advance().cloned() {
                    Some(ExprNode::Number(v))
                } else {
                    None
                }
            }
            Token::Var(_) => {
                if let Some(Token::Var(name)) = self.advance().cloned() {
                    Some(ExprNode::Var(name))
                } else {
                    None
                }
            }
            Token::LParen => {
                self.advance(); // consume '('
                let node = self.parse_expr();
                if let Some(Token::RParen) = self.peek() {
                    self.advance(); // consume ')'
                }
                node
            }
            _ => None,
        }
    }
}

/// Validate an expression string. Returns `Ok(())` if it tokenises and parses.
pub fn validate_expression(expr: &str) -> Result<(), String> {
    let tokens = tokenize(expr)?;
    let mut parser = ExprParser::new(&tokens);
    let node = parser
        .parse_expr()
        .ok_or_else(|| "Failed to parse expression".to_string())?;
    if parser.pos != tokens.len() {
        return Err("Unexpected tokens after expression".to_string());
    }
    // Quick-evaluate on a dummy bar to make sure variables resolve without panic.
    let dummy = BarData::new(
        String::new(),
        String::new(),
        crate::trader::constant::Exchange::Local,
        chrono::Utc::now(),
    );
    let _val = node.eval(&dummy);
    Ok(())
}

/// Indicator type enum for UI selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorType {
    MA,
    EMA,
    WMA,
    BOLL,
    VWAP,
    AVL,
    TRIX,
    SAR,
    SUPER,
}

impl IndicatorType {
    pub fn display_name(&self) -> &'static str {
        match self {
            IndicatorType::MA => "MA - 移动平均线",
            IndicatorType::EMA => "EMA - 指数移动平均线",
            IndicatorType::WMA => "WMA - 加权移动平均线",
            IndicatorType::BOLL => "BOLL - 布林线",
            IndicatorType::VWAP => "VWAP - 成交量加权平均价格",
            IndicatorType::AVL => "AVL - 均价线",
            IndicatorType::TRIX => "TRIX - 三重指数平滑移动平均线",
            IndicatorType::SAR => "SAR - 抛物线转向指标",
            IndicatorType::SUPER => "SUPER - SUPERTREND",
        }
    }

    pub fn all() -> Vec<IndicatorType> {
        vec![
            IndicatorType::MA,
            IndicatorType::EMA,
            IndicatorType::WMA,
            IndicatorType::BOLL,
            IndicatorType::VWAP,
            IndicatorType::AVL,
            IndicatorType::TRIX,
            IndicatorType::SAR,
            IndicatorType::SUPER,
        ]
    }
}
