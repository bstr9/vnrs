//! Technical indicators for charting.

use egui::{Color32, Pos2, Stroke};
use crate::trader::object::BarData;
use crate::trader::utility::ArrayManager;

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
            LineStyle::Dashed => Stroke::new(width, color), // egui doesn't support dashed natively
            LineStyle::Dotted => Stroke::new(width, color), // egui doesn't support dotted natively
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

/// Base trait for all indicators
pub trait Indicator: Send + Sync {
    /// Get indicator name
    fn name(&self) -> &str;
    
    /// Get display location (main or sub chart)
    fn location(&self) -> IndicatorLocation;
    
    /// Calculate indicator values for given bar data
    fn calculate(&mut self, bars: &[BarData]);
    
    /// Get the number of result series
    fn series_count(&self) -> usize;
    
    /// Get values for a specific bar index and series
    fn get_value(&self, bar_index: usize, series_index: usize) -> Option<f64>;
    
    /// Get line configuration for a series
    fn get_line_config(&self, series_index: usize) -> Option<&IndicatorLineConfig>;
    
    /// Get Y-axis range for this indicator
    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)>;
}

/// Moving Average (MA)
pub struct MA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period || self.period == 0 {
            return;
        }
        
        for i in (self.period - 1)..bars.len() {
            let start_ix = i.saturating_sub(self.period - 1);
            let sum: f64 = bars[start_ix..=i]
                .iter()
                .map(|b| b.close_price)
                .sum();
            self.values[i] = Some(sum / self.period as f64);
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Exponential Moving Average (EMA)
pub struct EMA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
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
        }
    }
}

impl Indicator for EMA {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn location(&self) -> IndicatorLocation {
        self.location
    }
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period || self.period == 0 {
            return;
        }
        
        let multiplier = 2.0 / (self.period as f64 + 1.0);
        
        // First EMA is a simple average
        let initial_sum: f64 = bars[0..self.period]
            .iter()
            .map(|b| b.close_price)
            .sum();
        let mut ema = initial_sum / self.period as f64;
        if self.period > 0 && self.period <= bars.len() {
            self.values[self.period - 1] = Some(ema);
        }
        
        // Calculate subsequent EMAs
        for i in self.period..bars.len() {
            ema = (bars[i].close_price * multiplier) + (ema * (1.0 - multiplier));
            self.values[i] = Some(ema);
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
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
                name: format!("BOLL上轨"),
                color: Color32::from_rgb(255, 100, 100),
                style: LineStyle::Solid,
                width: 1.0,
            },
            middle_config: IndicatorLineConfig {
                name: format!("BOLL中轨"),
                color: Color32::from_rgb(255, 255, 100),
                style: LineStyle::Solid,
                width: 1.5,
            },
            lower_config: IndicatorLineConfig {
                name: format!("BOLL下轨"),
                color: Color32::from_rgb(100, 255, 100),
                style: LineStyle::Solid,
                width: 1.0,
            },
            location,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.upper.clear();
        self.middle.clear();
        self.lower.clear();
        
        self.upper.resize(bars.len(), None);
        self.middle.resize(bars.len(), None);
        self.lower.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period || self.period == 0 {
            return;
        }
        
        for i in (self.period - 1)..bars.len() {
            let start_ix = i.saturating_sub(self.period - 1);
            let window = &bars[start_ix..=i];
            let sum: f64 = window.iter().map(|b| b.close_price).sum();
            let mean = sum / self.period as f64;
            
            let variance: f64 = window
                .iter()
                .map(|b| (b.close_price - mean).powi(2))
                .sum::<f64>() / self.period as f64;
            let std = variance.sqrt();
            
            self.middle[i] = Some(mean);
            self.upper[i] = Some(mean + self.std_dev * std);
            self.lower[i] = Some(mean - self.std_dev * std);
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
        if min_ix > max_ix {
            return None;
        }
        
        let mut all_values = Vec::new();
        
        for series in [&self.upper, &self.middle, &self.lower] {
            if series.is_empty() || min_ix >= series.len() {
                continue;
            }
            let end_ix = max_ix.min(series.len().saturating_sub(1));
            all_values.extend(
                series[min_ix..=end_ix]
                    .iter()
                    .filter_map(|v| *v)
            );
        }
        
        if all_values.is_empty() {
            return None;
        }
        
        let min = all_values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = all_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Weighted Moving Average (WMA)
pub struct WMA {
    period: usize,
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period || self.period == 0 {
            return;
        }
        
        let weights: Vec<f64> = (1..=self.period).map(|i| i as f64).collect();
        let weight_sum: f64 = weights.iter().sum();
        
        if weight_sum == 0.0 {
            return;
        }
        
        for i in (self.period - 1)..bars.len() {
            let start_ix = i.saturating_sub(self.period - 1);
            let weighted_sum: f64 = bars[start_ix..=i]
                .iter()
                .enumerate()
                .map(|(j, b)| b.close_price * weights[j])
                .sum();
            self.values[i] = Some(weighted_sum / weight_sum);
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Volume Weighted Average Price (VWAP)
pub struct VWAP {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.is_empty() {
            return;
        }
        
        let mut cumulative_pv = 0.0;
        let mut cumulative_v = 0.0;
        
        for (i, bar) in bars.iter().enumerate() {
            let typical_price = (bar.high_price + bar.low_price + bar.close_price) / 3.0;
            cumulative_pv += typical_price * bar.volume;
            cumulative_v += bar.volume;
            
            if cumulative_v > 0.0 {
                self.values[i] = Some(cumulative_pv / cumulative_v);
            }
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Parabolic SAR (Stop and Reverse)
pub struct SAR {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
    acceleration: f64,
    maximum: f64,
}

impl SAR {
    pub fn new(acceleration: f64, maximum: f64, color: Color32, location: IndicatorLocation) -> Self {
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.len() < 2 {
            return;
        }
        
        let mut is_long = true;
        let mut sar = bars[0].low_price;
        let mut ep = bars[0].high_price;
        let mut af = self.acceleration;
        
        self.values[0] = Some(sar);
        
        for i in 1..bars.len() {
            let high = bars[i].high_price;
            let low = bars[i].low_price;
            
            sar = sar + af * (ep - sar);
            
            if is_long {
                if i >= 2 {
                    sar = sar.min(bars[i-1].low_price).min(bars[i-2].low_price);
                } else if i >= 1 {
                    sar = sar.min(bars[i-1].low_price);
                }
                
                if low < sar {
                    is_long = false;
                    sar = ep;
                    ep = low;
                    af = self.acceleration;
                } else if high > ep {
                    ep = high;
                    af = (af + self.acceleration).min(self.maximum);
                }
            } else {
                if i >= 2 {
                    sar = sar.max(bars[i-1].high_price).max(bars[i-2].high_price);
                } else if i >= 1 {
                    sar = sar.max(bars[i-1].high_price);
                }
                
                if high > sar {
                    is_long = true;
                    sar = ep;
                    ep = high;
                    af = self.acceleration;
                } else if low < ep {
                    ep = low;
                    af = (af + self.acceleration).min(self.maximum);
                }
            }
            
            self.values[i] = Some(sar);
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Average Price Line (AVL)
pub struct AVL {
    values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    location: IndicatorLocation,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        
        if bars.is_empty() {
            return;
        }
        
        for (i, bar) in bars.iter().enumerate() {
            // AVL = (High + Low + Close) / 3
            let avg_price = (bar.high_price + bar.low_price + bar.close_price) / 3.0;
            self.values[i] = Some(avg_price);
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let values: Vec<f64> = self.values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// Triple Exponential Smoothed Moving Average (TRIX)
pub struct TRIX {
    values: Vec<Option<f64>>,
    signal_values: Vec<Option<f64>>,
    config: IndicatorLineConfig,
    signal_config: IndicatorLineConfig,
    location: IndicatorLocation,
    period: usize,
    signal_period: usize,
}

impl TRIX {
    pub fn new(period: usize, signal_period: usize, color: Color32, signal_color: Color32, location: IndicatorLocation) -> Self {
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
            signal_period,
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.values.clear();
        self.values.resize(bars.len(), None);
        self.signal_values.clear();
        self.signal_values.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period * 3 || self.period == 0 {
            return;
        }
        
        let multiplier = 2.0 / (self.period as f64 + 1.0);
        
        // First EMA
        let mut ema1 = vec![None; bars.len()];
        let first_sum: f64 = bars.iter().take(self.period).map(|b| b.close_price).sum();
        ema1[self.period - 1] = Some(first_sum / self.period as f64);
        
        for i in self.period..bars.len() {
            if let Some(prev_ema) = ema1[i - 1] {
                ema1[i] = Some((bars[i].close_price - prev_ema) * multiplier + prev_ema);
            }
        }
        
        // Second EMA
        let mut ema2 = vec![None; bars.len()];
        let start_idx = self.period * 2 - 1;
        if start_idx < bars.len() {
            let second_sum: f64 = ema1[self.period - 1..self.period * 2 - 1]
                .iter()
                .filter_map(|v| *v)
                .sum();
            ema2[start_idx] = Some(second_sum / self.period as f64);
            
            for i in (start_idx + 1)..bars.len() {
                if let (Some(curr_ema1), Some(prev_ema2)) = (ema1[i], ema2[i - 1]) {
                    ema2[i] = Some((curr_ema1 - prev_ema2) * multiplier + prev_ema2);
                }
            }
        }
        
        // Third EMA
        let mut ema3 = vec![None; bars.len()];
        let start_idx3 = self.period * 3 - 1;
        if start_idx3 < bars.len() {
            let third_sum: f64 = ema2[start_idx..start_idx3]
                .iter()
                .filter_map(|v| *v)
                .sum();
            ema3[start_idx3] = Some(third_sum / self.period as f64);
            
            for i in (start_idx3 + 1)..bars.len() {
                if let (Some(curr_ema2), Some(prev_ema3)) = (ema2[i], ema3[i - 1]) {
                    ema3[i] = Some((curr_ema2 - prev_ema3) * multiplier + prev_ema3);
                }
            }
        }
        
        // Calculate TRIX: ((EMA3[i] - EMA3[i-1]) / EMA3[i-1]) * 10000
        for i in (start_idx3 + 1)..bars.len() {
            if let (Some(curr), Some(prev)) = (ema3[i], ema3[i - 1]) {
                if prev.abs() > 1e-10 {
                    self.values[i] = Some(((curr - prev) / prev) * 10000.0);
                }
            }
        }
        
        // Calculate signal line (EMA of TRIX)
        if self.signal_period > 0 {
            let signal_multiplier = 2.0 / (self.signal_period as f64 + 1.0);
            let mut signal_start = start_idx3 + 1;
            
            // Find first valid TRIX value for signal calculation
            while signal_start < bars.len() && self.values[signal_start].is_none() {
                signal_start += 1;
            }
            
            if signal_start + self.signal_period <= bars.len() {
                let signal_sum: f64 = self.values[signal_start..signal_start + self.signal_period]
                    .iter()
                    .filter_map(|v| *v)
                    .sum();
                self.signal_values[signal_start + self.signal_period - 1] = Some(signal_sum / self.signal_period as f64);
                
                for i in (signal_start + self.signal_period)..bars.len() {
                    if let (Some(curr_trix), Some(prev_signal)) = (self.values[i], self.signal_values[i - 1]) {
                        self.signal_values[i] = Some((curr_trix - prev_signal) * signal_multiplier + prev_signal);
                    }
                }
            }
        }
    }
    
    fn series_count(&self) -> usize {
        2  // TRIX line + Signal line
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
        if self.values.is_empty() || min_ix > max_ix || min_ix >= self.values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.values.len().saturating_sub(1));
        let mut all_values = Vec::new();
        
        // Collect TRIX values
        all_values.extend(
            self.values[min_ix..=end_ix]
                .iter()
                .filter_map(|v| *v)
        );
        
        // Collect Signal values
        all_values.extend(
            self.signal_values[min_ix..=end_ix]
                .iter()
                .filter_map(|v| *v)
        );
        
        if all_values.is_empty() {
            return None;
        }
        
        let min = all_values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = all_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
}

/// SuperTrend Indicator
pub struct SUPER {
    trend_values: Vec<Option<f64>>,  // SuperTrend line values
    trend_direction: Vec<Option<i32>>,  // 1 for uptrend, -1 for downtrend
    config_up: IndicatorLineConfig,
    config_down: IndicatorLineConfig,
    location: IndicatorLocation,
    period: usize,
    multiplier: f64,
}

impl SUPER {
    pub fn new(period: usize, multiplier: f64, up_color: Color32, down_color: Color32, location: IndicatorLocation) -> Self {
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
    
    fn calculate(&mut self, bars: &[BarData]) {
        self.trend_values.clear();
        self.trend_values.resize(bars.len(), None);
        self.trend_direction.clear();
        self.trend_direction.resize(bars.len(), None);
        
        if bars.is_empty() || bars.len() < self.period || self.period == 0 {
            return;
        }
        
        // Calculate ATR (Average True Range)
        let mut atr_values = vec![None; bars.len()];
        let mut tr_values = vec![0.0; bars.len()];
        
        for i in 0..bars.len() {
            if i == 0 {
                tr_values[i] = bars[i].high_price - bars[i].low_price;
            } else {
                let high_low = bars[i].high_price - bars[i].low_price;
                let high_close = (bars[i].high_price - bars[i - 1].close_price).abs();
                let low_close = (bars[i].low_price - bars[i - 1].close_price).abs();
                tr_values[i] = high_low.max(high_close).max(low_close);
            }
        }
        
        // Calculate ATR using SMA
        for i in (self.period - 1)..bars.len() {
            let start_ix = i.saturating_sub(self.period - 1);
            let sum: f64 = tr_values[start_ix..=i].iter().sum();
            atr_values[i] = Some(sum / self.period as f64);
        }
        
        // Calculate basic bands
        let mut basic_upper = vec![None; bars.len()];
        let mut basic_lower = vec![None; bars.len()];
        
        for i in (self.period - 1)..bars.len() {
            if let Some(atr) = atr_values[i] {
                let hl_avg = (bars[i].high_price + bars[i].low_price) / 2.0;
                basic_upper[i] = Some(hl_avg + self.multiplier * atr);
                basic_lower[i] = Some(hl_avg - self.multiplier * atr);
            }
        }
        
        // Calculate final bands
        let mut final_upper = vec![None; bars.len()];
        let mut final_lower = vec![None; bars.len()];
        
        for i in (self.period - 1)..bars.len() {
            if let Some(bu) = basic_upper[i] {
                if i == self.period - 1 {
                    final_upper[i] = Some(bu);
                } else if let Some(prev_fu) = final_upper[i - 1] {
                    final_upper[i] = Some(if bu < prev_fu || bars[i - 1].close_price > prev_fu {
                        bu
                    } else {
                        prev_fu
                    });
                } else {
                    final_upper[i] = Some(bu);
                }
            }
            
            if let Some(bl) = basic_lower[i] {
                if i == self.period - 1 {
                    final_lower[i] = Some(bl);
                } else if let Some(prev_fl) = final_lower[i - 1] {
                    final_lower[i] = Some(if bl > prev_fl || bars[i - 1].close_price < prev_fl {
                        bl
                    } else {
                        prev_fl
                    });
                } else {
                    final_lower[i] = Some(bl);
                }
            }
        }
        
        // Determine trend and SuperTrend values
        for i in (self.period - 1)..bars.len() {
            if i == self.period - 1 {
                // Initial trend
                if let (Some(fu), Some(fl)) = (final_upper[i], final_lower[i]) {
                    if bars[i].close_price <= fu {
                        self.trend_direction[i] = Some(1);
                        self.trend_values[i] = Some(fu);
                    } else {
                        self.trend_direction[i] = Some(-1);
                        self.trend_values[i] = Some(fl);
                    }
                }
            } else {
                if let (Some(prev_dir), Some(fu), Some(fl)) = (self.trend_direction[i - 1], final_upper[i], final_lower[i]) {
                    if prev_dir == 1 {
                        if bars[i].close_price <= fu {
                            self.trend_direction[i] = Some(1);
                            self.trend_values[i] = Some(fu);
                        } else {
                            self.trend_direction[i] = Some(-1);
                            self.trend_values[i] = Some(fl);
                        }
                    } else {
                        if bars[i].close_price >= fl {
                            self.trend_direction[i] = Some(-1);
                            self.trend_values[i] = Some(fl);
                        } else {
                            self.trend_direction[i] = Some(1);
                            self.trend_values[i] = Some(fu);
                        }
                    }
                }
            }
        }
    }
    
    fn series_count(&self) -> usize {
        1  // SuperTrend line (color changes based on direction)
    }
    
    fn get_value(&self, bar_index: usize, _series_index: usize) -> Option<f64> {
        self.trend_values.get(bar_index).and_then(|v| *v)
    }
    
    fn get_line_config(&self, _series_index: usize) -> Option<&IndicatorLineConfig> {
        // Return config based on current trend direction at last bar
        if let Some(Some(dir)) = self.trend_direction.last() {
            if *dir == 1 {
                Some(&self.config_up)
            } else {
                Some(&self.config_down)
            }
        } else {
            Some(&self.config_up)
        }
    }
    
    fn get_y_range(&self, min_ix: usize, max_ix: usize) -> Option<(f64, f64)> {
        if self.trend_values.is_empty() || min_ix > max_ix || min_ix >= self.trend_values.len() {
            return None;
        }
        
        let end_ix = max_ix.min(self.trend_values.len().saturating_sub(1));
        let values: Vec<f64> = self.trend_values[min_ix..=end_ix]
            .iter()
            .filter_map(|v| *v)
            .collect();
        
        if values.is_empty() {
            return None;
        }
        
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((min, max))
    }
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
