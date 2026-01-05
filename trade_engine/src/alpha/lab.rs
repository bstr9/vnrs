//! Alpha Lab module for managing alpha research data

use crate::alpha::logger::AlphaLogger;
use crate::alpha::dataset::AlphaDataset;
use crate::alpha::model::AlphaModel;
use crate::alpha::types::AlphaBarData;
#[cfg(feature = "alpha")]
use polars::prelude::*;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::collections::HashMap;

/// Alpha Lab for managing alpha research data
pub struct AlphaLab {
    pub daily_path: String,
    pub minute_path: String,
    pub datasets: HashMap<String, AlphaDataset>,
    pub models: HashMap<String, Box<dyn AlphaModel>>,
    pub logger: AlphaLogger,
}

impl AlphaLab {
    /// Create a new AlphaLab instance
    pub fn new() -> Self {
        AlphaLab {
            daily_path: "./data/daily".to_string(),
            minute_path: "./data/minute".to_string(),
            datasets: HashMap::new(),
            models: HashMap::new(),
            logger: AlphaLogger,
        }
    }

    /// Save bar data to parquet file
    #[cfg(feature = "alpha")]
    pub fn save_bar_data(&self, bars: Vec<AlphaBarData>) -> Result<(), Box<dyn std::error::Error>> {
        if bars.is_empty() {
            return Ok(());
        }

        let first_bar = &bars[0];
        let interval = first_bar.interval.unwrap_or(crate::trader::Interval::Minute);
        let folder_path = if interval == crate::trader::Interval::Daily {
            &self.daily_path
        } else {
            &self.minute_path
        };

        // Create directory if it doesn't exist
        if !Path::new(folder_path).exists() {
            std::fs::create_dir_all(folder_path)?;
        }

        let file_path = format!("{}/{}.parquet", folder_path, first_bar.vt_symbol());
        
        // Convert bars to DataFrame
        let mut datetimes = Vec::new();
        let mut opens = Vec::new();
        let mut highs = Vec::new();
        let mut lows = Vec::new();
        let mut closes = Vec::new();
        let mut volumes = Vec::new();

        for bar in &bars {
            datetimes.push(bar.datetime.timestamp_millis());
            opens.push(bar.open);
            highs.push(bar.high);
            lows.push(bar.low);
            closes.push(bar.close);
            volumes.push(bar.volume);
        }

        let mut df = DataFrame::new(vec![
            Column::new("datetime".into(), datetimes),
            Column::new("open".into(), opens),
            Column::new("high".into(), highs),
            Column::new("low".into(), lows),
            Column::new("close".into(), closes),
            Column::new("volume".into(), volumes),
        ])?;

        let file_path_clone = file_path.clone();
        let mut file = std::fs::File::create(&file_path)?;
        ParquetWriter::new(&mut file).finish(&mut df)?;
        
        self.logger.info(&format!("Saved {} bars to {}", bars.len(), file_path_clone));
        Ok(())
    }

    #[cfg(not(feature = "alpha"))]
    pub fn save_bar_data(&self, _bars: Vec<AlphaBarData>) -> Result<(), Box<dyn std::error::Error>> {
        Err("Alpha feature not enabled".into())
    }

    /// Load bar data from parquet file
    #[cfg(feature = "alpha")]
    pub fn load_bar_data(
        &self,
        vt_symbol: &str,
        interval: crate::trader::Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<AlphaBarData>, Box<dyn std::error::Error>> {
        let folder_path = if interval == crate::trader::Interval::Daily {
            &self.daily_path
        } else {
            &self.minute_path
        };

        let file_path = format!("{}/{}.parquet", folder_path, vt_symbol);
        if !Path::new(&file_path).exists() {
            self.logger.error(&format!("File {} does not exist", file_path));
            return Ok(Vec::new());
        }

        let df = LazyFrame::scan_parquet(&file_path, Default::default())?.collect()?;

        let mut bars = Vec::new();
        for idx in 0..df.height() {
            let datetime_ts = df.column("datetime")?.i64()?.get(idx).unwrap_or(0);
            let dt = DateTime::from_timestamp_millis(datetime_ts).unwrap_or(Utc::now());
            
            let bar = AlphaBarData {
                symbol: vt_symbol.split('.').next().unwrap_or("").to_string(),
                exchange: crate::trader::Exchange::Binance,
                datetime: dt,
                interval: Some(interval),
                volume: df.column("volume")?.f64()?.get(idx).unwrap_or(0.0),
                turnover: 0.0,
                open_interest: 0.0,
                open: df.column("open")?.f64()?.get(idx).unwrap_or(0.0),
                high: df.column("high")?.f64()?.get(idx).unwrap_or(0.0),
                low: df.column("low")?.f64()?.get(idx).unwrap_or(0.0),
                close: df.column("close")?.f64()?.get(idx).unwrap_or(0.0),
                gateway_name: "DB".to_string(),
            };
            
            // Filter by datetime
            if bar.datetime >= start && bar.datetime <= end {
                bars.push(bar);
            }
        }

        self.logger.info(&format!("Loaded {} bars from {}", bars.len(), file_path));
        Ok(bars)
    }

    #[cfg(not(feature = "alpha"))]
    pub fn load_bar_data(
        &self,
        _vt_symbol: &str,
        _interval: crate::trader::Interval,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<AlphaBarData>, Box<dyn std::error::Error>> {
        Err("Alpha feature not enabled".into())
    }

    /// Load contract settings
    pub fn load_contract_settings(&self) -> Result<HashMap<String, crate::trader::ContractData>, Box<dyn std::error::Error>> {
        // Simplified - return empty map
        Ok(HashMap::new())
    }

    /// List all datasets
    pub fn list_all_datasets(&self) -> Vec<String> {
        self.datasets.keys().cloned().collect()
    }

    /// List all models
    pub fn list_all_models(&self) -> Vec<String> {
        self.models.keys().cloned().collect()
    }

    /// List all signals
    pub fn list_all_signals(&self) -> Vec<String> {
        // Simplified - return empty list
        Vec::new()
    }
}

impl Default for AlphaLab {
    fn default() -> Self {
        Self::new()
    }
}