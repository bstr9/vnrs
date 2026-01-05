//! Bar data manager for the chart module.
//! 
//! Manages bar data with datetime indexing and provides efficient lookup
//! and range queries for price and volume data.

use std::collections::HashMap;
use chrono::{DateTime, Utc};

use crate::trader::object::BarData;
use super::base::to_int;

/// Manages bar data with datetime-based indexing
pub struct BarManager {
    /// Bar data indexed by datetime
    bars: HashMap<DateTime<Utc>, BarData>,
    /// Map from datetime to index
    datetime_index_map: HashMap<DateTime<Utc>, usize>,
    /// Map from index to datetime
    index_datetime_map: HashMap<usize, DateTime<Utc>>,
    /// Ordered list of bar data
    ordered_bars: Vec<BarData>,
    /// Cached price ranges
    price_ranges: HashMap<(usize, usize), (f64, f64)>,
    /// Cached volume ranges
    volume_ranges: HashMap<(usize, usize), (f64, f64)>,
}

impl Default for BarManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BarManager {
    /// Create a new BarManager
    pub fn new() -> Self {
        Self {
            bars: HashMap::new(),
            datetime_index_map: HashMap::new(),
            index_datetime_map: HashMap::new(),
            ordered_bars: Vec::new(),
            price_ranges: HashMap::new(),
            volume_ranges: HashMap::new(),
        }
    }
    
    /// Update with a list of bar data
    pub fn update_history(&mut self, history: Vec<BarData>) {
        // Put all new bars into dict
        for bar in history {
            self.bars.insert(bar.datetime, bar);
        }
        
        // Sort bars by datetime
        let mut sorted_bars: Vec<_> = self.bars.values().cloned().collect();
        sorted_bars.sort_by_key(|bar| bar.datetime);
        
        // Update index maps
        self.datetime_index_map.clear();
        self.index_datetime_map.clear();
        
        for (ix, bar) in sorted_bars.iter().enumerate() {
            self.datetime_index_map.insert(bar.datetime, ix);
            self.index_datetime_map.insert(ix, bar.datetime);
        }
        
        self.ordered_bars = sorted_bars;
        
        // Clear cache
        self.clear_cache();
    }
    
    /// Update with a single bar
    pub fn update_bar(&mut self, bar: BarData) {
        let dt = bar.datetime;
        
        if !self.datetime_index_map.contains_key(&dt) {
            let ix = self.bars.len();
            self.datetime_index_map.insert(dt, ix);
            self.index_datetime_map.insert(ix, dt);
            self.ordered_bars.push(bar.clone());
        } else if let Some(&ix) = self.datetime_index_map.get(&dt) {
            if ix < self.ordered_bars.len() {
                self.ordered_bars[ix] = bar.clone();
            }
        }
        
        self.bars.insert(dt, bar);
        self.clear_cache();
    }
    
    /// Get total number of bars
    pub fn get_count(&self) -> usize {
        self.ordered_bars.len()
    }
    
    /// Get index for a datetime
    pub fn get_index(&self, dt: DateTime<Utc>) -> Option<usize> {
        self.datetime_index_map.get(&dt).copied()
    }
    
    /// Get datetime for an index
    pub fn get_datetime(&self, ix: f64) -> Option<DateTime<Utc>> {
        let ix = to_int(ix) as usize;
        self.index_datetime_map.get(&ix).copied()
    }
    
    /// Get bar data for an index
    pub fn get_bar(&self, ix: f64) -> Option<&BarData> {
        let ix = to_int(ix) as usize;
        self.ordered_bars.get(ix)
    }
    
    /// Get all bar data
    pub fn get_all_bars(&self) -> &[BarData] {
        &self.ordered_bars
    }
    
    /// Get price range for given index range
    pub fn get_price_range(&self, min_ix: Option<usize>, max_ix: Option<usize>) -> (f64, f64) {
        if self.ordered_bars.is_empty() {
            return (0.0, 1.0);
        }
        
        let min_ix = min_ix.unwrap_or(0);
        let max_ix = max_ix.unwrap_or(self.ordered_bars.len().saturating_sub(1));
        let max_ix = max_ix.min(self.ordered_bars.len().saturating_sub(1));
        
        if min_ix > max_ix {
            return (0.0, 1.0);
        }
        
        // Check cache
        if let Some(&range) = self.price_ranges.get(&(min_ix, max_ix)) {
            return range;
        }
        
        let bars = &self.ordered_bars[min_ix..=max_ix];
        if bars.is_empty() {
            return (0.0, 1.0);
        }
        
        let mut min_price = bars[0].low_price;
        let mut max_price = bars[0].high_price;
        
        for bar in bars.iter().skip(1) {
            min_price = min_price.min(bar.low_price);
            max_price = max_price.max(bar.high_price);
        }
        
        (min_price, max_price)
    }
    
    /// Get volume range for given index range
    pub fn get_volume_range(&self, min_ix: Option<usize>, max_ix: Option<usize>) -> (f64, f64) {
        if self.ordered_bars.is_empty() {
            return (0.0, 1.0);
        }
        
        let min_ix = min_ix.unwrap_or(0);
        let max_ix = max_ix.unwrap_or(self.ordered_bars.len().saturating_sub(1));
        let max_ix = max_ix.min(self.ordered_bars.len().saturating_sub(1));
        
        if min_ix > max_ix {
            return (0.0, 1.0);
        }
        
        // Check cache
        if let Some(&range) = self.volume_ranges.get(&(min_ix, max_ix)) {
            return range;
        }
        
        let bars = &self.ordered_bars[min_ix..=max_ix];
        if bars.is_empty() {
            return (0.0, 1.0);
        }
        
        let min_volume = 0.0;
        let mut max_volume = bars[0].volume;
        
        for bar in bars.iter().skip(1) {
            max_volume = max_volume.max(bar.volume);
        }
        
        (min_volume, max_volume)
    }
    
    /// Cache price range
    pub fn cache_price_range(&mut self, min_ix: usize, max_ix: usize, range: (f64, f64)) {
        self.price_ranges.insert((min_ix, max_ix), range);
    }
    
    /// Cache volume range
    pub fn cache_volume_range(&mut self, min_ix: usize, max_ix: usize, range: (f64, f64)) {
        self.volume_ranges.insert((min_ix, max_ix), range);
    }
    
    /// Clear cached range data
    fn clear_cache(&mut self) {
        self.price_ranges.clear();
        self.volume_ranges.clear();
    }
    
    /// Clear all data
    pub fn clear_all(&mut self) {
        self.bars.clear();
        self.datetime_index_map.clear();
        self.index_datetime_map.clear();
        self.ordered_bars.clear();
        self.clear_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Exchange, Interval};
    
    fn create_test_bar(datetime: DateTime<Utc>, open: f64, high: f64, low: f64, close: f64, volume: f64) -> BarData {
        BarData {
            symbol: "TEST".to_string(),
            exchange: Exchange::Binance,
            datetime,
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover: 0.0,
            open_interest: 0.0,
            gateway_name: "".to_string(),
            extra: None,
        }
    }
    
    #[test]
    fn test_bar_manager_update_history() {
        let mut manager = BarManager::new();
        
        let bars = vec![
            create_test_bar(Utc::now(), 100.0, 105.0, 95.0, 102.0, 1000.0),
        ];
        
        manager.update_history(bars);
        assert_eq!(manager.get_count(), 1);
    }
    
    #[test]
    fn test_bar_manager_price_range() {
        let mut manager = BarManager::new();
        
        let now = Utc::now();
        let bars = vec![
            create_test_bar(now, 100.0, 105.0, 95.0, 102.0, 1000.0),
            create_test_bar(now + chrono::Duration::minutes(1), 102.0, 110.0, 98.0, 108.0, 1500.0),
        ];
        
        manager.update_history(bars);
        
        let (min_price, max_price) = manager.get_price_range(None, None);
        assert_eq!(min_price, 95.0);
        assert_eq!(max_price, 110.0);
    }
}
