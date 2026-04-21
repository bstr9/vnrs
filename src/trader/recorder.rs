//! DataRecorder engine for automatically recording tick/bar data to database.
//!
//! Inspired by vn.py's DataRecorder module. Subscribes to market data events
//! and persists them to a database for backtesting and analysis.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use super::constant::{Exchange, Interval};
use super::database::BaseDatabase;
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{BarData, TickData};

/// Default flush interval in seconds
const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 60;

/// Default batch size before forcing a flush
const DEFAULT_BATCH_SIZE: usize = 1000;

/// Record status for tracking data collection
#[derive(Debug, Clone)]
pub struct RecordStatus {
    pub symbol: String,
    pub exchange: Exchange,
    pub interval: Option<Interval>,
    pub count: u64,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

/// Recorder configuration
#[derive(Debug, Clone)]
pub struct RecorderConfig {
    /// Flush interval in seconds
    pub flush_interval_secs: u64,
    /// Batch size before forcing flush
    pub batch_size: usize,
    /// Enable tick recording
    pub record_ticks: bool,
    /// Enable bar recording
    pub record_bars: bool,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: DEFAULT_FLUSH_INTERVAL_SECS,
            batch_size: DEFAULT_BATCH_SIZE,
            record_ticks: true,
            record_bars: true,
        }
    }
}

/// DataRecorder engine
///
/// Automatically records tick and bar data to a database.
/// Buffers data in memory and flushes periodically or when batch size is reached.
pub struct DataRecorder {
    /// Engine name
    name: String,
    /// Database backend
    database: Arc<dyn BaseDatabase>,
    /// Configuration
    config: RecorderConfig,
    /// Active recording symbols for ticks (vt_symbol)
    tick_symbols: RwLock<HashSet<String>>,
    /// Active recording symbols for bars (vt_symbol with interval)
    bar_symbols: RwLock<HashSet<String>>,
    /// Tick buffer
    tick_buffer: RwLock<Vec<TickData>>,
    /// Bar buffer
    bar_buffer: RwLock<Vec<BarData>>,
    /// Record status tracking
    status: RwLock<HashMap<String, RecordStatus>>,
    /// Running flag
    running: AtomicBool,
    /// Event receiver
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<RecorderEvent>>>,
    /// Event sender (for external use)
    event_tx: mpsc::UnboundedSender<RecorderEvent>,
}

/// Internal events for the recorder
#[allow(dead_code)]
pub enum RecorderEvent {
    Tick(Box<TickData>),
    Bar(BarData),
    Flush,
    Stop,
}

impl DataRecorder {
    /// Create a new DataRecorder with a database backend
    pub fn new(database: Arc<dyn BaseDatabase>) -> Self {
        Self::with_config(database, RecorderConfig::default())
    }

    /// Create a new DataRecorder with custom configuration
    pub fn with_config(database: Arc<dyn BaseDatabase>, config: RecorderConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            name: "DataRecorder".to_string(),
            database,
            config,
            tick_symbols: RwLock::new(HashSet::new()),
            bar_symbols: RwLock::new(HashSet::new()),
            tick_buffer: RwLock::new(Vec::new()),
            bar_buffer: RwLock::new(Vec::new()),
            status: RwLock::new(HashMap::new()),
            running: AtomicBool::new(false),
            event_rx: RwLock::new(Some(event_rx)),
            event_tx,
        }
    }

    /// Get the event sender for receiving tick/bar events
    pub fn get_sender(&self) -> mpsc::UnboundedSender<RecorderEvent> {
        self.event_tx.clone()
    }

    /// Subscribe to tick recording for a symbol
    pub async fn subscribe_tick(&self, symbol: &str, exchange: Exchange) {
        let vt_symbol = format!("{}.{}", symbol, exchange.value());
        let mut symbols = self.tick_symbols.write().await;
        if symbols.insert(vt_symbol.clone()) {
            info!("[DataRecorder] Subscribed to tick recording: {}", vt_symbol);
        }
    }

    /// Unsubscribe from tick recording
    pub async fn unsubscribe_tick(&self, symbol: &str, exchange: Exchange) {
        let vt_symbol = format!("{}.{}", symbol, exchange.value());
        let mut symbols = self.tick_symbols.write().await;
        if symbols.remove(&vt_symbol) {
            info!("[DataRecorder] Unsubscribed from tick recording: {}", vt_symbol);
        }
    }

    /// Subscribe to bar recording for a symbol with specific interval
    pub async fn subscribe_bar(&self, symbol: &str, exchange: Exchange, interval: Interval) {
        let vt_symbol = format!("{}.{}.{}", symbol, exchange.value(), interval.value());
        let mut symbols = self.bar_symbols.write().await;
        if symbols.insert(vt_symbol.clone()) {
            info!("[DataRecorder] Subscribed to bar recording: {}", vt_symbol);
        }
    }

    /// Unsubscribe from bar recording
    pub async fn unsubscribe_bar(&self, symbol: &str, exchange: Exchange, interval: Interval) {
        let vt_symbol = format!("{}.{}.{}", symbol, exchange.value(), interval.value());
        let mut symbols = self.bar_symbols.write().await;
        if symbols.remove(&vt_symbol) {
            info!("[DataRecorder] Unsubscribed from bar recording: {}", vt_symbol);
        }
    }

    /// Record a tick (called from event handler)
    pub async fn on_tick(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();
        let symbols = self.tick_symbols.read().await;
        if !symbols.contains(&vt_symbol) {
            return;
        }
        drop(symbols);

        // Add to buffer
        let mut buffer = self.tick_buffer.write().await;
        buffer.push(tick.clone());

        // Update status
        self.update_tick_status(tick).await;

        // Check batch size
        if buffer.len() >= self.config.batch_size {
            drop(buffer);
            self.flush_ticks().await;
        }
    }

    /// Record a bar (called from event handler)
    pub async fn on_bar(&self, bar: &BarData) {
        let interval = bar.interval.unwrap_or(Interval::Minute);
        let vt_symbol = format!("{}.{}", bar.vt_symbol(), interval.value());
        
        let symbols = self.bar_symbols.read().await;
        if !symbols.contains(&vt_symbol) {
            return;
        }
        drop(symbols);

        // Add to buffer
        let mut buffer = self.bar_buffer.write().await;
        buffer.push(bar.clone());

        // Update status
        self.update_bar_status(bar).await;

        // Check batch size
        if buffer.len() >= self.config.batch_size {
            drop(buffer);
            self.flush_bars().await;
        }
    }

    /// Update tick recording status
    async fn update_tick_status(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();
        let mut status = self.status.write().await;
        
        let entry = status.entry(vt_symbol.clone()).or_insert_with(|| RecordStatus {
            symbol: tick.symbol.clone(),
            exchange: tick.exchange,
            interval: None,
            count: 0,
            start: None,
            end: None,
        });
        
        entry.count += 1;
        if entry.start.is_none() || tick.datetime < entry.start.unwrap_or_default() {
            entry.start = Some(tick.datetime);
        }
        if entry.end.is_none() || tick.datetime > entry.end.unwrap_or_default() {
            entry.end = Some(tick.datetime);
        }
        if entry.end.is_none() || tick.datetime > entry.end.unwrap_or_default() {
            entry.end = Some(tick.datetime);
        }
    }

    /// Update bar recording status
    async fn update_bar_status(&self, bar: &BarData) {
        let interval = bar.interval.unwrap_or(Interval::Minute);
        let key = format!("{}.{}", bar.vt_symbol(), interval.value());
        
        let mut status = self.status.write().await;
        
        let entry = status.entry(key.clone()).or_insert_with(|| RecordStatus {
            symbol: bar.symbol.clone(),
            exchange: bar.exchange,
            interval: Some(interval),
            count: 0,
            start: None,
            end: None,
        });
        
        entry.count += 1;
        if entry.start.is_none() || bar.datetime < entry.start.unwrap_or_default() {
            entry.start = Some(bar.datetime);
        }
        if entry.end.is_none() || bar.datetime > entry.end.unwrap_or_default() {
            entry.end = Some(bar.datetime);
        }
        if entry.end.is_none() || bar.datetime > entry.end.unwrap_or_default() {
            entry.end = Some(bar.datetime);
        }
    }

    /// Flush tick buffer to database
    pub async fn flush_ticks(&self) {
        let mut buffer = self.tick_buffer.write().await;
        if buffer.is_empty() {
            return;
        }

        let ticks: Vec<TickData> = buffer.drain(..).collect();
        let count = ticks.len();
        
        info!("[DataRecorder] Flushing {} ticks to database", count);
        
        match self.database.save_tick_data(ticks, false).await {
            Ok(true) => {
                debug!("[DataRecorder] Successfully saved {} ticks", count);
            }
            Ok(false) => {
                warn!("[DataRecorder] Database returned false for tick save");
            }
            Err(e) => {
                error!("[DataRecorder] Failed to save ticks: {}", e);
            }
        }
    }

    /// Flush bar buffer to database
    pub async fn flush_bars(&self) {
        let mut buffer = self.bar_buffer.write().await;
        if buffer.is_empty() {
            return;
        }

        let bars: Vec<BarData> = buffer.drain(..).collect();
        let count = bars.len();
        
        info!("[DataRecorder] Flushing {} bars to database", count);
        
        match self.database.save_bar_data(bars, false).await {
            Ok(true) => {
                debug!("[DataRecorder] Successfully saved {} bars", count);
            }
            Ok(false) => {
                warn!("[DataRecorder] Database returned false for bar save");
            }
            Err(e) => {
                error!("[DataRecorder] Failed to save bars: {}", e);
            }
        }
    }

    /// Flush all buffers
    pub async fn flush(&self) {
        self.flush_ticks().await;
        self.flush_bars().await;
    }

    /// Get recording status
    pub async fn get_status(&self) -> Vec<RecordStatus> {
        let status = self.status.read().await;
        status.values().cloned().collect()
    }

    /// Get buffer sizes
    pub async fn get_buffer_sizes(&self) -> (usize, usize) {
        let tick_buffer = self.tick_buffer.read().await;
        let bar_buffer = self.bar_buffer.read().await;
        (tick_buffer.len(), bar_buffer.len())
    }

    /// Start the recorder event loop
    pub async fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            info!("[DataRecorder] Already running");
            return;
        }

        info!("[DataRecorder] Starting with flush interval {}s", self.config.flush_interval_secs);

        // Take the receiver
        let rx = {
            let mut rx_lock = self.event_rx.write().await;
            rx_lock.take()
        };

        if let Some(mut rx) = rx {
            let flush_interval = tokio::time::Duration::from_secs(self.config.flush_interval_secs);
            let mut flush_timer = tokio::time::interval(flush_interval);

            while self.running.load(Ordering::SeqCst) {
                tokio::select! {
                    Some(event) = rx.recv() => {
                        match event {
                            RecorderEvent::Tick(tick) => {
                                self.on_tick(&tick).await;
                            }
                            RecorderEvent::Bar(bar) => {
                                self.on_bar(&bar).await;
                            }
                            RecorderEvent::Flush => {
                                self.flush().await;
                            }
                            RecorderEvent::Stop => {
                                break;
                            }
                        }
                    }
                    _ = flush_timer.tick() => {
                        // Periodic flush
                        self.flush().await;
                    }
                }
            }

            // Final flush on exit
            self.flush().await;
            info!("[DataRecorder] Stopped");
        }
    }

    /// Stop the recorder
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.event_tx.send(RecorderEvent::Stop);
    }
}

impl BaseEngine for DataRecorder {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.stop();
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        // Route gateway events to the recorder's internal event channel
        // The async event loop will pick them up and call on_tick/on_bar
        match event {
            GatewayEvent::Tick(tick) => {
                let _ = self.event_tx.send(RecorderEvent::Tick(Box::new(tick.clone())));
            }
            GatewayEvent::Bar(bar) => {
                let _ = self.event_tx.send(RecorderEvent::Bar(bar.clone()));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::database::MemoryDatabase;

    #[tokio::test]
    async fn test_data_recorder_tick() {
        let db = Arc::new(MemoryDatabase::new());
        let recorder = DataRecorder::new(db.clone());
        
        // Subscribe to tick recording (must match TickData symbol case)
        recorder.subscribe_tick("btcusdt", Exchange::Binance).await;
        
        // Create test tick
        let tick = TickData::new(
            "test".to_string(),
            "btcusdt".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        // Record tick
        recorder.on_tick(&tick).await;
        
        // Flush
        recorder.flush().await;
        
        // Check status
        let status = recorder.get_status().await;
        assert!(!status.is_empty());
    }

    #[tokio::test]
    async fn test_data_recorder_bar() {
        let db = Arc::new(MemoryDatabase::new());
        let recorder = DataRecorder::new(db.clone());
        
        // Subscribe to bar recording (must match BarData symbol case)
        recorder.subscribe_bar("btcusdt", Exchange::Binance, Interval::Minute).await;
        
        // Create test bar
        let bar = BarData::new(
            "1m".to_string(),
            "btcusdt".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        // Record bar
        recorder.on_bar(&bar).await;
        
        // Flush
        recorder.flush().await;
        
        // Check database
        let overviews = db.get_bar_overview().await.expect("Should get overview");
        assert!(!overviews.is_empty());
    }
}
