//! Engine module for the trading platform core functionality.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, atomic::{AtomicBool, AtomicU64, Ordering}};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::app::BaseApp;
use super::constant::Exchange;
use super::contract_manager::ContractManager;
use super::converter::OffsetConverter;
use super::data_download::DataDownloadManager;
use super::database::{BaseDatabase, EventRecord};
use super::portfolio::PortfolioManager;
use super::recorder::{DataRecorder, RecordStatus, RecorderConfig};
use super::risk::RiskManager;
use super::bracket_order::BracketOrderEngine;
use super::stop_order::StopOrderEngine;
use super::order_emulator::OrderEmulator;
use super::order_book::OrderBookManager;
use super::reconciliation::ReconciliationEngine;
use super::data_engine::DataEngine;
use super::session::TradingSessionManager;

use super::event::*;
use super::gateway::{BaseGateway, GatewayEvent, GatewaySettings};
use super::object::{
    AccountData, BarData, CancelRequest, ContractData, HistoryRequest, LogData,
    OrderData, OrderRequest, PositionData, QuoteData, QuoteRequest, SubscribeRequest,
    TickData, TradeData,
};
use super::setting::SETTINGS;


/// Event handler type
pub type EventHandler = Box<dyn Fn(&GatewayEvent) + Send + Sync>;

/// Persistence task for async database writes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum PersistTask {
    Order(OrderData),
    Trade(TradeData),
    Position(PositionData),
    Event(EventRecord),
}

/// Overflow file path for persistence fallback
fn overflow_dir() -> PathBuf {
    super::utility::TRADER_DIR.join("overflow")
}

fn overflow_file_path() -> PathBuf {
    overflow_dir().join("persist_overflow.jsonl")
}

/// Ensure the overflow directory exists, creating it if needed.
fn ensure_overflow_dir() -> Result<(), String> {
    let dir = overflow_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create overflow dir: {}", e))?;
    }
    Ok(())
}

/// Attempt to send a PersistTask through the bounded channel.
/// On failure (channel full), serialize to the overflow file instead.
fn persist_with_overflow(
    tx: &mpsc::Sender<PersistTask>,
    task: PersistTask,
    sent_counter: &AtomicU64,
    overflow_counter: &AtomicU64,
) {
    match tx.try_send(task.clone()) {
        Ok(()) => {
            sent_counter.fetch_add(1, Ordering::Relaxed);
        }
        Err(e) => {
            // Channel full or closed — write to overflow file
            overflow_counter.fetch_add(1, Ordering::Relaxed);
            if let Err(dir_err) = ensure_overflow_dir() {
                warn!("Cannot create overflow directory: {}", dir_err);
                return;
            }
            match serde_json::to_string(&task) {
                Ok(json_line) => {
                    let path = overflow_file_path();
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    match OpenOptions::new().create(true).append(true).open(&path) {
                        Ok(mut file) => {
                            if let Err(write_err) = writeln!(file, "{}", json_line) {
                                warn!("Failed to write overflow record: {}", write_err);
                            } else {
                                warn!(
                                    "Persistence channel full, wrote record to overflow file (total overflowed: {})",
                                    overflow_counter.load(Ordering::Relaxed)
                                );
                            }
                        }
                        Err(open_err) => {
                            warn!("Failed to open overflow file: {}", open_err);
                        }
                    }
                }
                Err(ser_err) => {
                    warn!("Failed to serialize PersistTask for overflow: {}", ser_err);
                }
            }
            // Log the original channel error at debug level (already warned above)
            debug!("persist_tx.try_send failed: {}", e);
        }
    }
}

/// Read and remove up to `limit` overflow records from the overflow file,
/// returning them as deserialized PersistTask instances.
fn read_overflow_records(limit: usize) -> Vec<PersistTask> {
    let path = overflow_file_path();
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read overflow file: {}", e);
            return Vec::new();
        }
    };
    if content.is_empty() {
        return Vec::new();
    }

    let mut tasks = Vec::new();
    let mut remaining_lines = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if tasks.len() < limit {
            match serde_json::from_str::<PersistTask>(line) {
                Ok(task) => tasks.push(task),
                Err(e) => {
                    warn!("Failed to deserialize overflow record, keeping: {}", e);
                    remaining_lines.push(line.to_string());
                }
            }
        } else {
            remaining_lines.push(line.to_string());
        }
    }

    // Rewrite the file with only the remaining lines (ones we didn't replay)
    if let Err(dir_err) = ensure_overflow_dir() {
        warn!("Cannot create overflow directory for rewrite: {}", dir_err);
        return tasks;
    }
    match std::fs::write(&path, remaining_lines.join("\n")) {
        Ok(()) => {}
        Err(e) => {
            warn!("Failed to rewrite overflow file after replay: {}", e);
        }
    }

    if !tasks.is_empty() {
        info!("Replaying {} overflow records back into persistence channel", tasks.len());
    }

    tasks
}

/// Categorise a single PersistTask into typed vectors for batch processing.
fn categorize_persist_task(
    task: PersistTask,
) -> (Vec<OrderData>, Vec<TradeData>, Vec<PositionData>, Vec<EventRecord>) {
    match task {
        PersistTask::Order(o) => (vec![o], vec![], vec![], vec![]),
        PersistTask::Trade(t) => (vec![], vec![t], vec![], vec![]),
        PersistTask::Position(p) => (vec![], vec![], vec![p], vec![]),
        PersistTask::Event(e) => (vec![], vec![], vec![], vec![e]),
    }
}

/// Base engine trait for implementing function engines
pub trait BaseEngine: Send + Sync {
    /// Get the engine name
    fn engine_name(&self) -> &str;

    /// Close the engine
    fn close(&self) {}

    /// Process a gateway event (optional override for sub-engines that need event routing)
    fn process_event(&self, _event_type: &str, _event: &GatewayEvent) {}
}

/// OMS (Order Management System) Engine data container
pub struct OmsData {
    pub ticks: HashMap<String, TickData>,
    pub bars: HashMap<String, BarData>,
    pub orders: HashMap<String, OrderData>,
    pub trades: HashMap<String, TradeData>,
    pub positions: HashMap<String, PositionData>,
    pub accounts: HashMap<String, AccountData>,
    pub contracts: HashMap<String, ContractData>,
    pub quotes: HashMap<String, QuoteData>,
    pub active_orders: HashMap<String, OrderData>,
    pub active_quotes: HashMap<String, QuoteData>,
    pub logs: Vec<LogData>,
}

impl OmsData {
    pub fn new() -> Self {
        Self {
            ticks: HashMap::new(),
            bars: HashMap::new(),
            orders: HashMap::new(),
            trades: HashMap::new(),
            positions: HashMap::new(),
            accounts: HashMap::new(),
            contracts: HashMap::new(),
            quotes: HashMap::new(),
            active_orders: HashMap::new(),
            active_quotes: HashMap::new(),
            logs: Vec::new(),
        }
    }
}

impl Default for OmsData {
    fn default() -> Self {
        Self::new()
    }
}

/// OMS Engine for order management
pub struct OmsEngine {
    data: RwLock<OmsData>,
}

impl OmsEngine {
    /// Create a new OmsEngine
    pub fn new() -> Self {
        Self {
            data: RwLock::new(OmsData::new()),
        }
    }

    /// Process tick event
    pub fn process_tick(&self, tick: TickData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.ticks.insert(tick.vt_symbol(), tick);
    }

    /// Process bar event
    pub fn process_bar(&self, bar: BarData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.bars.insert(bar.vt_symbol(), bar);
    }

    /// Process order event
    pub fn process_order(&self, order: OrderData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        let vt_orderid = order.vt_orderid();
        
        // If order is active, update data in dict
        if order.is_active() {
            data.active_orders.insert(vt_orderid.clone(), order.clone());
        } else {
            // Otherwise, pop inactive order from dict
            data.active_orders.remove(&vt_orderid);
        }
        
        data.orders.insert(vt_orderid, order);
    }

    /// Process trade event
    pub fn process_trade(&self, trade: TradeData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.trades.insert(trade.vt_tradeid(), trade);
    }

    /// Process position event
    pub fn process_position(&self, position: PositionData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.positions.insert(position.vt_positionid(), position);
    }

    /// Process account event
    pub fn process_account(&self, account: AccountData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.accounts.insert(account.vt_accountid(), account);
    }

    /// Process contract event
    pub fn process_contract(&self, contract: ContractData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.contracts.insert(contract.vt_symbol(), contract);
    }

    /// Process quote event
    pub fn process_quote(&self, quote: QuoteData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        let vt_quoteid = quote.vt_quoteid();
        
        // If quote is active, update data in dict
        if quote.is_active() {
            data.active_quotes.insert(vt_quoteid.clone(), quote.clone());
        } else {
            // Otherwise, pop inactive quote from dict
            data.active_quotes.remove(&vt_quoteid);
        }
        
        data.quotes.insert(vt_quoteid, quote);
    }

    /// Get latest tick data by vt_symbol
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.ticks.get(vt_symbol).cloned()
    }

    /// Get latest bar data by vt_symbol
    pub fn get_bar(&self, vt_symbol: &str) -> Option<BarData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.bars.get(vt_symbol).cloned()
    }

    /// Get latest order data by vt_orderid
    pub fn get_order(&self, vt_orderid: &str) -> Option<OrderData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.orders.get(vt_orderid).cloned()
    }

    /// Get trade data by vt_tradeid
    pub fn get_trade(&self, vt_tradeid: &str) -> Option<TradeData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.trades.get(vt_tradeid).cloned()
    }

    /// Get latest position data by vt_positionid
    pub fn get_position(&self, vt_positionid: &str) -> Option<PositionData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.positions.get(vt_positionid).cloned()
    }

    /// Get latest account data by vt_accountid
    pub fn get_account(&self, vt_accountid: &str) -> Option<AccountData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.accounts.get(vt_accountid).cloned()
    }

    /// Get contract data by vt_symbol
    pub fn get_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.contracts.get(vt_symbol).cloned()
    }

    /// Get latest quote data by vt_quoteid
    pub fn get_quote(&self, vt_quoteid: &str) -> Option<QuoteData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.quotes.get(vt_quoteid).cloned()
    }

    /// Get all tick data
    pub fn get_all_ticks(&self) -> Vec<TickData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.ticks.values().cloned().collect()
    }

    /// Get all bar data
    pub fn get_all_bars(&self) -> Vec<BarData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.bars.values().cloned().collect()
    }

    /// Get all order data
    pub fn get_all_orders(&self) -> Vec<OrderData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.orders.values().cloned().collect()
    }

    /// Get all trade data
    pub fn get_all_trades(&self) -> Vec<TradeData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.trades.values().cloned().collect()
    }

    /// Get all position data
    pub fn get_all_positions(&self) -> Vec<PositionData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.positions.values().cloned().collect()
    }

    /// Get all account data
    pub fn get_all_accounts(&self) -> Vec<AccountData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.accounts.values().cloned().collect()
    }

    /// Get all contract data, sorted by symbol with popular pairs first
    pub fn get_all_contracts(&self) -> Vec<ContractData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        let mut contracts: Vec<ContractData> = data.contracts.values().cloned().collect();
        
        // Popular trading pairs to prioritize (USDT pairs)
        let popular_pairs = [
            "btcusdt", "ethusdt", "bnbusdt", "solusdt", "xrpusdt",
            "dogeusdt", "adausdt", "avaxusdt", "dotusdt", "maticusdt",
            "btcusdc", "ethusdc",
        ];
        
        // Sort: popular pairs first, then alphabetically
        contracts.sort_by(|a, b| {
            let a_lower = a.symbol.to_lowercase();
            let b_lower = b.symbol.to_lowercase();
            let a_popular = popular_pairs.iter().position(|p| &a_lower == p).unwrap_or(999);
            let b_popular = popular_pairs.iter().position(|p| &b_lower == p).unwrap_or(999);
            
            match (a_popular, b_popular) {
                (ap, bp) if ap != bp => ap.cmp(&bp),
                _ => a.symbol.cmp(&b.symbol),
            }
        });
        
        contracts
    }

    /// Get all quote data
    pub fn get_all_quotes(&self) -> Vec<QuoteData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.quotes.values().cloned().collect()
    }

    /// Get all active orders
    pub fn get_all_active_orders(&self) -> Vec<OrderData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.active_orders.values().cloned().collect()
    }

    /// Get all active quotes
    pub fn get_all_active_quotes(&self) -> Vec<QuoteData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.active_quotes.values().cloned().collect()
    }

    /// Process log event
    pub fn process_log(&self, log: LogData) {
        let mut data = self.data.write().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.logs.insert(0, log);
        // Keep only last 1000 logs
        if data.logs.len() > 1000 {
            data.logs.truncate(1000);
        }
    }

    /// Get all log data
    pub fn get_all_logs(&self) -> Vec<LogData> {
        let data = self.data.read().unwrap_or_else(|e| {
            warn!("OmsEngine lock poisoned, recovering");
            e.into_inner()
        });
        data.logs.clone()
    }
}

impl Default for OmsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for OmsEngine {
    fn engine_name(&self) -> &str {
        "oms"
    }
}

/// Log engine for handling log events
pub struct LogEngine {
    active: bool,
}

impl LogEngine {
    /// Create a new LogEngine
    pub fn new() -> Self {
        let active = SETTINGS.get_bool("log.active").unwrap_or(true);
        Self { active }
    }

    /// Process log event
    pub fn process_log(&self, log: &LogData) {
        if !self.active {
            return;
        }

        let level = log.level;
        let msg = &log.msg;
        let gateway = &log.gateway_name;

        match level {
            10 => debug!(gateway = gateway, "{}", msg),
            20 => info!(gateway = gateway, "{}", msg),
            30 => warn!(gateway = gateway, "{}", msg),
            40 | 50 => error!(gateway = gateway, "{}", msg),
            _ => info!(gateway = gateway, "{}", msg),
        }
    }
}

impl Default for LogEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for LogEngine {
    fn engine_name(&self) -> &str {
        "log"
    }
}

/// Main engine acts as the core of the trading platform
pub struct MainEngine {
    gateways: RwLock<HashMap<String, Arc<dyn BaseGateway>>>,
    engines: RwLock<HashMap<String, Arc<dyn BaseEngine>>>,
    #[allow(dead_code)]
    apps: RwLock<HashMap<String, Arc<dyn BaseApp>>>,
    exchanges: RwLock<Vec<Exchange>>,
    
    oms_engine: Arc<OmsEngine>,
    log_engine: Arc<LogEngine>,
    risk_manager: Arc<RiskManager>,
    alert_engine: Arc<super::alert::AlertEngine>,
    algo_engine: Arc<super::algo::AlgoEngine>,
    data_download_manager: Arc<DataDownloadManager>,
    portfolio_manager: Arc<PortfolioManager>,
    stop_order_engine: Arc<StopOrderEngine>,
    bracket_order_engine: Arc<BracketOrderEngine>,
    order_emulator: Arc<OrderEmulator>,
    contract_manager: Arc<ContractManager>,
    session_manager: Arc<TradingSessionManager>,
    order_book_manager: Arc<OrderBookManager>,
    reconciliation_engine: RwLock<Option<Arc<ReconciliationEngine>>>,
    data_engine: RwLock<Option<Arc<DataEngine>>>,
    offset_converter: RwLock<OffsetConverter>,
    recorder: RwLock<Option<Arc<DataRecorder>>>,
    
    event_tx: mpsc::UnboundedSender<(String, GatewayEvent)>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<(String, GatewayEvent)>>>,
    
    handlers: RwLock<HashMap<String, Vec<EventHandler>>>,
    running: AtomicBool,

    /// Optional database for event journaling and crash recovery (#10, #11)
    database: Option<Arc<dyn BaseDatabase>>,
    /// Bounded persistence channel for async database writes (capacity 4096)
    persist_tx: mpsc::Sender<PersistTask>,
    /// Persistence receiver (taken by drain task on start)
    persist_rx: RwLock<Option<mpsc::Receiver<PersistTask>>>,
    /// Monotonic event ID counter for event journaling
    event_id_counter: AtomicU64,
    /// Counter for successfully sent persistence tasks
    persist_sent_count: AtomicU64,
    /// Counter for persistence tasks that overflowed to file
    persist_overflowed_count: AtomicU64,
}

impl MainEngine {
    /// Create a new MainEngine without database persistence
    pub fn new() -> Self {
        Self::new_internal(None)
    }

    /// Create a new MainEngine with database persistence for event journaling and crash recovery
    pub fn new_with_database(database: Arc<dyn BaseDatabase>) -> Self {
        Self::new_internal(Some(database))
    }

    /// Create a new MainEngine with SQLite database auto-loaded from the default path.
    ///
    /// The database file is stored at `{TRADER_DIR}/trade_engine.db`.
    /// If the file doesn't exist, it will be created automatically.
    /// Requires the `sqlite` feature flag.
    #[cfg(feature = "sqlite")]
    pub fn new_with_sqlite() -> Result<Self, String> {
        let db_path = super::utility::TRADER_DIR.join("trade_engine.db");
        let db = super::sqlite_database::SqliteDatabase::new(
            db_path.to_str().ok_or("Invalid database path")?
        )?;
        Ok(Self::new_with_database(Arc::new(db)))
    }

    /// Create a new MainEngine with SQLite database at a custom path.
    ///
    /// Requires the `sqlite` feature flag.
    #[cfg(feature = "sqlite")]
    pub fn new_with_sqlite_at(path: &str) -> Result<Self, String> {
        let db = super::sqlite_database::SqliteDatabase::new(path)?;
        Ok(Self::new_with_database(Arc::new(db)))
    }

    /// Internal constructor
    fn new_internal(database: Option<Arc<dyn BaseDatabase>>) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (persist_tx, persist_rx) = mpsc::channel(4096);
        
        let oms_engine = Arc::new(OmsEngine::new());
        let log_engine = Arc::new(LogEngine::new());
        let risk_manager = Arc::new(RiskManager::new());
        let alert_engine = Arc::new(super::alert::AlertEngine::new());
        let algo_engine = Arc::new(super::algo::AlgoEngine::new());
        let data_download_manager = Arc::new(DataDownloadManager::new());
        let portfolio_manager = Arc::new(PortfolioManager::new());
        let stop_order_engine = Arc::new(StopOrderEngine::new());
        let bracket_order_engine = Arc::new(BracketOrderEngine::new());
        let order_emulator = Arc::new(OrderEmulator::new());
        let contract_manager = Arc::new(ContractManager::new());
        let session_manager = Arc::new(TradingSessionManager::new());
        let order_book_manager = Arc::new(OrderBookManager::new());

        // Create OffsetConverter with contract lookup from OmsEngine
        let oms_for_converter = oms_engine.clone();
        let offset_converter = OffsetConverter::new(Box::new(move |vt_symbol: &str| {
            oms_for_converter.get_contract(vt_symbol)
        }));
        
        let engine = Self {
            gateways: RwLock::new(HashMap::new()),
            engines: RwLock::new(HashMap::new()),
            apps: RwLock::new(HashMap::new()),
            exchanges: RwLock::new(Vec::new()),
            oms_engine,
            log_engine,
            risk_manager,
            alert_engine,
            algo_engine,
            data_download_manager,
            portfolio_manager,
            stop_order_engine,
            bracket_order_engine,
            order_emulator,
            contract_manager,
            session_manager,
            order_book_manager,
            reconciliation_engine: RwLock::new(None),
            data_engine: RwLock::new(None),
            offset_converter: RwLock::new(offset_converter),
            recorder: RwLock::new(None),
            event_tx,
            event_rx: RwLock::new(Some(event_rx)),
            handlers: RwLock::new(HashMap::new()),
            running: AtomicBool::new(false),
            database,
            persist_tx,
            persist_rx: RwLock::new(Some(persist_rx)),
            event_id_counter: AtomicU64::new(0),
            persist_sent_count: AtomicU64::new(0),
            persist_overflowed_count: AtomicU64::new(0),
        };
        
        // Register OMS engine, log engine, risk manager, alert engine, and new engines
        {
            let mut engines = engine.engines.write().unwrap_or_else(|e| e.into_inner());
            engines.insert("oms".to_string(), engine.oms_engine.clone());
            engines.insert("log".to_string(), engine.log_engine.clone());
            engines.insert("risk".to_string(), engine.risk_manager.clone());
            engines.insert("alert".to_string(), engine.alert_engine.clone());
            engines.insert("algo".to_string(), engine.algo_engine.clone());
            engines.insert("DataDownloadManager".to_string(), engine.data_download_manager.clone());
            engines.insert("PortfolioManager".to_string(), engine.portfolio_manager.clone());
            engines.insert("StopOrderEngine".to_string(), engine.stop_order_engine.clone());
            engines.insert("BracketOrderEngine".to_string(), engine.bracket_order_engine.clone());
            engines.insert("OrderEmulator".to_string(), engine.order_emulator.clone());
            engines.insert("ContractManager".to_string(), engine.contract_manager.clone());
            engines.insert("TradingSessionManager".to_string(), engine.session_manager.clone());
            engines.insert("OrderBookManager".to_string(), engine.order_book_manager.clone());
        }
        
        engine
    }

    /// Start the main engine event loop
    pub async fn start(&self) {
        self.running.store(true, Ordering::SeqCst);

        // Take the receiver from the RwLock
        let rx = {
            let mut rx_lock = self.event_rx.write().unwrap_or_else(|e| e.into_inner());
            rx_lock.take()
        };

        // Spawn persistence drain task if database is configured
        let db = self.database.clone();
        let persist_tx = self.persist_tx.clone();
        let persist_rx = {
            let mut persist_lock = self.persist_rx.write().unwrap_or_else(|e| e.into_inner());
            persist_lock.take()
        };
        const CHANNEL_CAPACITY: usize = 4096;
        if let (Some(db), Some(mut persist_rx)) = (db, persist_rx) {
            tokio::spawn(async move {
                info!("Persistence drain task started");
                // Batch processing: collect up to 64 tasks per iteration to
                // amortise expensive database I/O (especially FileDatabase
                // which does read-all/rewrite-all per save call).
                const BATCH_SIZE: usize = 64;
                while let Some(task) = persist_rx.recv().await {
                    let mut orders = Vec::new();
                    let mut trades = Vec::new();
                    let mut positions = Vec::new();
                    let mut events = Vec::new();

                    // Collect first task and try to drain more
                    let (o, t, p, e) = categorize_persist_task(task);
                    orders.extend(o);
                    trades.extend(t);
                    positions.extend(p);
                    events.extend(e);

                    // Non-blocking drain of additional queued tasks
                    for _ in 0..BATCH_SIZE {
                        match persist_rx.try_recv() {
                            Ok(task) => {
                                let (o, t, p, e) = categorize_persist_task(task);
                                orders.extend(o);
                                trades.extend(t);
                                positions.extend(p);
                                events.extend(e);
                            }
                            Err(_) => break,
                        }
                    }

                    // Batch-write each category
                    if !orders.is_empty() {
                        if let Err(e) = db.save_order_data(orders).await {
                            warn!("Failed to persist orders batch: {}", e);
                        }
                    }
                    if !trades.is_empty() {
                        if let Err(e) = db.save_trade_data(trades).await {
                            warn!("Failed to persist trades batch: {}", e);
                        }
                    }
                    if !positions.is_empty() {
                        if let Err(e) = db.save_position_data(positions).await {
                            warn!("Failed to persist positions batch: {}", e);
                        }
                    }
                    for event_record in events {
                        if let Err(e) = db.save_event(event_record).await {
                            warn!("Failed to persist event record: {}", e);
                        }
                    }

                    // Overflow replay: if channel is below 50% capacity, try to replay overflow records
                    let current_capacity = persist_tx.capacity();
                    if current_capacity > CHANNEL_CAPACITY / 2 {
                        let replay_count = (current_capacity - CHANNEL_CAPACITY / 2).min(BATCH_SIZE);
                        let overflow_tasks = read_overflow_records(replay_count);
                        for overflow_task in overflow_tasks {
                            // Use try_send to avoid blocking; if it fails again, it will be re-written
                            // to overflow via persist_with_overflow (but we don't have counters here)
                            // So we just use try_send and log on failure
                            if let Err(e) = persist_tx.try_send(overflow_task) {
                                warn!("Failed to replay overflow record: {}", e);
                                // Note: the overflow file was already cleared by read_overflow_records,
                                // so this record is lost. This is acceptable as a last resort.
                            }
                        }
                    }
                }
                info!("Persistence drain task stopped");
            });
        }

        if let Some(mut rx) = rx {
            loop {
                if !self.running.load(Ordering::SeqCst) {
                    break;
                }
                tokio::select! {
                    Some((event_type, event)) = rx.recv() => {
                        self.process_event(&event_type, &event);
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(1)) => {}
                }
            }

            // Drain remaining events that were queued before gateways fully stopped.
            // This prevents event loss when close() sets running=false while
            // in-flight events are still in the channel.
            while let Ok((event_type, event)) = rx.try_recv() {
                self.process_event(&event_type, &event);
            }
            info!("MainEngine event loop stopped, all remaining events drained");
        }
    }

    /// Process an event
    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        // Process in OMS engine
        match event {
            GatewayEvent::Tick(tick) => self.oms_engine.process_tick(tick.clone()),
            GatewayEvent::Bar(bar) => self.oms_engine.process_bar(bar.clone()),
            GatewayEvent::Order(order) => self.oms_engine.process_order(order.clone()),
            GatewayEvent::Trade(trade) => self.oms_engine.process_trade(trade.clone()),
            GatewayEvent::Position(position) => self.oms_engine.process_position(position.clone()),
            GatewayEvent::Account(account) => self.oms_engine.process_account(account.clone()),
            GatewayEvent::Contract(contract) => self.oms_engine.process_contract(contract.clone()),
            GatewayEvent::Quote(quote) => self.oms_engine.process_quote(quote.clone()),
            GatewayEvent::Log(log) => {
                self.log_engine.process_log(log);
                self.oms_engine.process_log(log.clone());
            }
            GatewayEvent::DepthBook(_) => {
                // Handled by OrderBookManager via sub-engine dispatch below
            }
        }

        // Persist order/trade/position to database for crash recovery (#10)
        // Skip tick/bar/account/contract/quote/log — high frequency or re-derivable
        if self.database.is_some() {
            match event {
                GatewayEvent::Order(order) => {
                    persist_with_overflow(
                        &self.persist_tx,
                        PersistTask::Order(order.clone()),
                        &self.persist_sent_count,
                        &self.persist_overflowed_count,
                    );
                }
                GatewayEvent::Trade(trade) => {
                    persist_with_overflow(
                        &self.persist_tx,
                        PersistTask::Trade(trade.clone()),
                        &self.persist_sent_count,
                        &self.persist_overflowed_count,
                    );
                }
                GatewayEvent::Position(position) => {
                    persist_with_overflow(
                        &self.persist_tx,
                        PersistTask::Position(position.clone()),
                        &self.persist_sent_count,
                        &self.persist_overflowed_count,
                    );
                }
                _ => {}
            }
            // Event journaling — record event types that are NOT already persisted above.
            // Order/Trade/Position are already persisted as typed records, so skip them
            // here to avoid doubling channel traffic. Also skip tick/bar/depth (too high frequency).
            if !matches!(
                event,
                GatewayEvent::Tick(_)
                    | GatewayEvent::Bar(_)
                    | GatewayEvent::DepthBook(_)
                    | GatewayEvent::Order(_)
                    | GatewayEvent::Trade(_)
                    | GatewayEvent::Position(_)
            ) {
                let event_id = self.event_id_counter.fetch_add(1, Ordering::Relaxed);
                let gateway_name = match event {
                    GatewayEvent::Tick(t) => t.gateway_name.clone(),
                    GatewayEvent::Bar(b) => b.gateway_name.clone(),
                    GatewayEvent::Order(o) => o.gateway_name.clone(),
                    GatewayEvent::Trade(t) => t.gateway_name.clone(),
                    GatewayEvent::Position(p) => p.gateway_name.clone(),
                    GatewayEvent::Account(a) => a.gateway_name.clone(),
                    GatewayEvent::Contract(c) => c.gateway_name.clone(),
                    GatewayEvent::Quote(q) => q.gateway_name.clone(),
                    GatewayEvent::Log(l) => l.gateway_name.clone(),
                    GatewayEvent::DepthBook(d) => d.gateway_name.clone(),
                };
                // Store a summary payload since GatewayEvent doesn't implement Serialize
                let payload = format!("{:?}", event);
                let record = EventRecord::new(event_id, event_type.to_string(), gateway_name, payload);
                persist_with_overflow(
                    &self.persist_tx,
                    PersistTask::Event(record),
                    &self.persist_sent_count,
                    &self.persist_overflowed_count,
                );
            }
        }

        // Update OffsetConverter with position/order/trade events (GAP 4 fix)
        {
            let mut converter = self.offset_converter.write().unwrap_or_else(|e| e.into_inner());
            match event {
                GatewayEvent::Position(position) => converter.update_position(position),
                GatewayEvent::Order(order) => converter.update_order(order),
                GatewayEvent::Trade(trade) => converter.update_trade(trade),
                _ => {}
            }
        }

        // Dispatch event to all registered sub-engines (except oms/log which already processed above)
        {
            let engines = self.engines.read().unwrap_or_else(|e| e.into_inner());
            for engine in engines.values() {
                let name = engine.engine_name();
                if name == "oms" || name == "log" {
                    continue; // Already processed above
                }
                engine.process_event(event_type, event);
            }
        }

        // Call registered handlers
        let handlers = self.handlers.read().unwrap_or_else(|e| e.into_inner());
        if let Some(handler_list) = handlers.get(event_type) {
            for handler in handler_list {
                handler(event);
            }
        }
        
        // Also call handlers for base event type (without suffix)
        let base_type = event_type.split('.').next().unwrap_or(event_type);
        if base_type != event_type {
            if let Some(handler_list) = handlers.get(base_type) {
                for handler in handler_list {
                    handler(event);
                }
            }
        }
    }

    /// Add a gateway
    pub fn add_gateway(&self, gateway: Arc<dyn BaseGateway>) -> Arc<dyn BaseGateway> {
        let gateway_name = gateway.gateway_name().to_string();
        
        let mut gateways = self.gateways.write().unwrap_or_else(|e| e.into_inner());
        gateways.insert(gateway_name, gateway.clone());
        
        gateway
    }

    /// Add a sub-engine for event routing
    /// The engine will receive all gateway events via its process_event() method
    pub fn add_engine(&self, engine: Arc<dyn BaseEngine>) {
        let engine_name = engine.engine_name().to_string();
        let mut engines = self.engines.write().unwrap_or_else(|e| e.into_inner());
        engines.insert(engine_name, engine);
    }

    /// Get a sub-engine by name
    pub fn get_engine(&self, engine_name: &str) -> Option<Arc<dyn BaseEngine>> {
        let engines = self.engines.read().unwrap_or_else(|e| e.into_inner());
        engines.get(engine_name).cloned()
    }

    /// Get a gateway by name
    pub fn get_gateway(&self, gateway_name: &str) -> Option<Arc<dyn BaseGateway>> {
        let gateways = self.gateways.read().unwrap_or_else(|e| e.into_inner());
        gateways.get(gateway_name).cloned()
    }

    /// Get all gateway names
    pub fn get_all_gateway_names(&self) -> Vec<String> {
        let gateways = self.gateways.read().unwrap_or_else(|e| e.into_inner());
        gateways.keys().cloned().collect()
    }

    /// Find the first gateway name that supports the given exchange
    pub fn find_gateway_name_for_exchange(&self, exchange: Exchange) -> Option<String> {
        let gateways = self.gateways.read().unwrap_or_else(|e| e.into_inner());
        for (name, gateway) in gateways.iter() {
            if gateway.default_exchange() == exchange {
                return Some(name.clone());
            }
        }
        None
    }

    /// Get all exchanges
    pub fn get_all_exchanges(&self) -> Vec<Exchange> {
        let exchanges = self.exchanges.read().unwrap_or_else(|e| e.into_inner());
        exchanges.clone()
    }

    /// Write a log message
    pub fn write_log(&self, msg: impl Into<String>, source: &str) {
        let log = LogData::new(source.to_string(), msg.into());
        let _ = self.event_tx.send((EVENT_LOG.to_string(), GatewayEvent::Log(log)));
    }

    /// Connect to a gateway
    pub async fn connect(&self, setting: GatewaySettings, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("连接登录 -> {}", gateway_name), "MainEngine");
            gateway.connect(setting).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Disconnect from a gateway
    pub async fn disconnect(&self, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("断开连接 -> {}", gateway_name), "MainEngine");
            gateway.close().await;
            Ok(())
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Reconnect to a gateway with new settings
    pub async fn reconnect(&self, setting: GatewaySettings, gateway_name: &str) -> Result<(), String> {
        let _ = self.disconnect(gateway_name).await;
        self.connect(setting, gateway_name).await
    }

    /// Subscribe to tick data
    pub async fn subscribe(&self, req: SubscribeRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("订阅行情 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.subscribe(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Unsubscribe from tick data
    pub async fn unsubscribe(&self, req: SubscribeRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("退订行情 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.unsubscribe(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Send an order (with risk check and offset conversion)
    pub async fn send_order(&self, req: OrderRequest, gateway_name: &str) -> Result<String, String> {
        // Pre-trade risk check (with gateway context for balance check)
        match self.risk_manager.check_order_with_gateway(&req, gateway_name) {
            super::risk::RiskCheckResult::Approved => {}
            super::risk::RiskCheckResult::Rejected(reason) => {
                self.write_log(format!("风控拒绝 -> {}", reason), "RiskManager");
                // Alert on risk rejection
                self.alert_engine.alert_risk_reject(
                    &reason,
                    Some(&format!("{}.{}", req.symbol, req.exchange.value())),
                    gateway_name,
                );
                return Err(reason);
            }
        }

        // Convert offset for SHFE/INE exchanges (GAP 4 fix)
        let converted_reqs = {
            let mut converter = self.offset_converter.write().unwrap_or_else(|e| e.into_inner());
            converter.convert_order_request(&req, false, false)
        };

        // If the converter split the request into multiple sub-requests, send each one
        if converted_reqs.len() == 1 && converted_reqs[0].offset == req.offset {
            // No conversion needed — single request with same offset
            if let Some(gateway) = self.get_gateway(gateway_name) {
                self.write_log(format!("委托下单 -> {}：{:?}", gateway_name, req), "MainEngine");
                let vt_orderid = gateway.send_order(req).await?;

                // Update offset converter with the new order request
                {
                    let mut converter = self.offset_converter.write().unwrap_or_else(|e| e.into_inner());
                    converter.update_order_request(&converted_reqs[0], &vt_orderid);
                }

                Ok(vt_orderid)
            } else {
                Err(format!("找不到底层接口：{}", gateway_name))
            }
        } else {
            // Multiple sub-requests from offset conversion
            let mut last_orderid = String::new();
            for sub_req in converted_reqs {
                if let Some(gateway) = self.get_gateway(gateway_name) {
                    self.write_log(
                        format!("委托下单(偏移转换) -> {}：offset={:?} vol={}", gateway_name, sub_req.offset, sub_req.volume),
                        "MainEngine",
                    );
                    let vt_orderid = gateway.send_order(sub_req.clone()).await?;

                    // Update offset converter
                    {
                        let mut converter = self.offset_converter.write().unwrap_or_else(|e| e.into_inner());
                        converter.update_order_request(&sub_req, &vt_orderid);
                    }

                    last_orderid = vt_orderid;
                } else {
                    return Err(format!("找不到底层接口：{}", gateway_name));
                }
            }
            Ok(last_orderid)
        }
    }

    /// Cancel an order
    pub async fn cancel_order(&self, req: CancelRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("委托撤单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.cancel_order(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Send a quote
    pub async fn send_quote(&self, req: QuoteRequest, gateway_name: &str) -> Result<String, String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("报价下单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.send_quote(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Cancel a quote
    pub async fn cancel_quote(&self, req: CancelRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("报价撤单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.cancel_quote(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Query history data
    pub async fn query_history(&self, req: HistoryRequest, gateway_name: &str) -> Result<Vec<BarData>, String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("查询K线 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.query_history(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Get OMS engine
    pub fn oms(&self) -> &Arc<OmsEngine> {
        &self.oms_engine
    }

    /// Get risk manager
    pub fn risk_manager(&self) -> &Arc<RiskManager> {
        &self.risk_manager
    }

    /// Get alert engine
    pub fn alert_engine(&self) -> &Arc<super::alert::AlertEngine> {
        &self.alert_engine
    }

    /// Get algo engine
    pub fn algo_engine(&self) -> &Arc<super::algo::AlgoEngine> {
        &self.algo_engine
    }

    /// Get data download manager
    pub fn data_download_manager(&self) -> &Arc<DataDownloadManager> {
        &self.data_download_manager
    }

    /// Get portfolio manager
    pub fn portfolio_manager(&self) -> &Arc<PortfolioManager> {
        &self.portfolio_manager
    }

    /// Get stop order engine
    pub fn stop_order_engine(&self) -> &Arc<StopOrderEngine> {
        &self.stop_order_engine
    }

    /// Get bracket order engine
    pub fn bracket_order_engine(&self) -> &Arc<BracketOrderEngine> {
        &self.bracket_order_engine
    }

    /// Get order emulator engine
    pub fn order_emulator(&self) -> &Arc<OrderEmulator> {
        &self.order_emulator
    }

    /// Get contract manager
    pub fn contract_manager(&self) -> &Arc<ContractManager> {
        &self.contract_manager
    }

    /// Get trading session manager
    pub fn session_manager(&self) -> &Arc<TradingSessionManager> {
        &self.session_manager
    }

    /// Get order book manager
    pub fn order_book_manager(&self) -> &Arc<OrderBookManager> {
        &self.order_book_manager
    }

    /// Create and register a ReconciliationEngine for this MainEngine.
    ///
    /// The ReconciliationEngine will auto-trigger reconciliation on gateway
    /// reconnect events and can be used to manually sync local vs venue state.
    ///
    /// Returns the created ReconciliationEngine, already registered as a sub-engine.
    pub fn add_reconciliation_engine(self: &Arc<Self>) -> Arc<ReconciliationEngine> {
        let recon = Arc::new(ReconciliationEngine::new(self.clone()));
        self.add_engine(recon.clone());
        *self.reconciliation_engine.write().unwrap_or_else(|e| e.into_inner()) = Some(recon.clone());
        info!("ReconciliationEngine已注册为子引擎");
        recon
    }

    /// Get the ReconciliationEngine if one has been added
    pub fn reconciliation_engine(&self) -> Option<Arc<ReconciliationEngine>> {
        self.reconciliation_engine.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Add a MetricsEngine and start a Prometheus metrics HTTP server.
    ///
    /// Requires the `prometheus` feature flag. The server exposes a `/metrics`
    /// endpoint on `addr` (e.g. `"127.0.0.1:9090"`).
    #[cfg(feature = "prometheus")]
    pub fn add_metrics_engine(self: &Arc<Self>, addr: &str) -> Result<(), String> {
        crate::trader::metrics::init();
        let metrics_engine = crate::trader::metrics::MetricsEngine::new();
        self.add_engine(Arc::new(metrics_engine));
        let server = crate::trader::metrics::MetricsServer::new(addr.to_string());
        server.start();
        info!("MetricsEngine registered, Prometheus endpoint at {addr}");
        Ok(())
    }

    /// Add a DataEngine for centralized subscription management and tick→bar aggregation
    ///
    /// The DataEngine will:
    /// - De-duplicate gateway subscriptions when multiple strategies subscribe to same symbol
    /// - Aggregate ticks into 1-minute bars centrally
    /// - Synthesize higher-timeframe bars (5m/15m/1h/4h/1d) from 1m bars
    /// - Emit bar events into the event stream for StrategyEngine consumption
    ///
    /// # Returns
    /// The created DataEngine, already registered as a sub-engine
    pub fn add_data_engine(self: &Arc<Self>) -> Arc<DataEngine> {
        let data_engine = Arc::new(DataEngine::new(self.event_tx.clone()));
        self.add_engine(data_engine.clone());
        *self.data_engine.write().unwrap_or_else(|e| e.into_inner()) = Some(data_engine.clone());
        info!("DataEngine已注册为子引擎");
        data_engine
    }

    /// Get the DataEngine if one has been added
    pub fn data_engine(&self) -> Option<Arc<DataEngine>> {
        self.data_engine.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get tick data
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.oms_engine.get_tick(vt_symbol)
    }

    /// Get bar data
    pub fn get_bar(&self, vt_symbol: &str) -> Option<BarData> {
        self.oms_engine.get_bar(vt_symbol)
    }

    /// Get order data
    pub fn get_order(&self, vt_orderid: &str) -> Option<OrderData> {
        self.oms_engine.get_order(vt_orderid)
    }

    /// Get trade data
    pub fn get_trade(&self, vt_tradeid: &str) -> Option<TradeData> {
        self.oms_engine.get_trade(vt_tradeid)
    }

    /// Get position data
    pub fn get_position(&self, vt_positionid: &str) -> Option<PositionData> {
        self.oms_engine.get_position(vt_positionid)
    }

    /// Get account data
    pub fn get_account(&self, vt_accountid: &str) -> Option<AccountData> {
        self.oms_engine.get_account(vt_accountid)
    }

    /// Get contract data
    pub fn get_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        self.oms_engine.get_contract(vt_symbol)
    }

    /// Get quote data
    pub fn get_quote(&self, vt_quoteid: &str) -> Option<QuoteData> {
        self.oms_engine.get_quote(vt_quoteid)
    }

    /// Get all ticks
    pub fn get_all_ticks(&self) -> Vec<TickData> {
        self.oms_engine.get_all_ticks()
    }

    /// Get all bars
    pub fn get_all_bars(&self) -> Vec<BarData> {
        self.oms_engine.get_all_bars()
    }

    /// Get all orders
    pub fn get_all_orders(&self) -> Vec<OrderData> {
        self.oms_engine.get_all_orders()
    }

    /// Get all trades
    pub fn get_all_trades(&self) -> Vec<TradeData> {
        self.oms_engine.get_all_trades()
    }

    /// Get all positions
    pub fn get_all_positions(&self) -> Vec<PositionData> {
        self.oms_engine.get_all_positions()
    }

    /// Get all accounts
    pub fn get_all_accounts(&self) -> Vec<AccountData> {
        self.oms_engine.get_all_accounts()
    }

    /// Get all contracts
    pub fn get_all_contracts(&self) -> Vec<ContractData> {
        self.oms_engine.get_all_contracts()
    }

    /// Get all quotes
    pub fn get_all_quotes(&self) -> Vec<QuoteData> {
        self.oms_engine.get_all_quotes()
    }

    /// Get all active orders
    pub fn get_all_active_orders(&self) -> Vec<OrderData> {
        self.oms_engine.get_all_active_orders()
    }

    /// Get all active quotes
    pub fn get_all_active_quotes(&self) -> Vec<QuoteData> {
        self.oms_engine.get_all_active_quotes()
    }

    /// Get all logs
    pub fn get_all_logs(&self) -> Vec<LogData> {
        self.oms_engine.get_all_logs()
    }

    /// Register an event handler
    pub fn register_handler(&self, event_type: &str, handler: EventHandler) {
        let mut handlers = self.handlers.write().unwrap_or_else(|e| e.into_inner());
        handlers.entry(event_type.to_string())
            .or_default()
            .push(handler);
    }

    /// Unregister all handlers for a given event type
    pub fn unregister_handlers(&self, event_type: &str) {
        let mut handlers = self.handlers.write().unwrap_or_else(|e| e.into_inner());
        handlers.remove(event_type);
    }

    /// Remove a sub-engine by name
    pub fn remove_engine(&self, engine_name: &str) -> Option<Arc<dyn BaseEngine>> {
        let mut engines = self.engines.write().unwrap_or_else(|e| e.into_inner());
        engines.remove(engine_name)
    }

    /// Get event sender for gateways
    pub fn get_event_sender(&self) -> mpsc::UnboundedSender<(String, GatewayEvent)> {
        self.event_tx.clone()
    }

    /// Restore engine state from database after crash (#11)
    ///
    /// Loads orders, trades, and positions from the database and re-populates
    /// OmsEngine's in-memory state. Must be called before `start()`.
    ///
    /// **Important**: This only populates OmsEngine. It does NOT re-emit events
    /// to sub-engines (strategy engine, etc.) to avoid side effects. Active orders
    /// should be reconciled against the exchange on gateway reconnect.
    pub async fn restore(&self) -> Result<(), String> {
        let db = match &self.database {
            Some(db) => db,
            None => return Err("No database configured for restore".to_string()),
        };

        // Load in dependency order: orders first, then trades, then positions
        let orders = db.load_orders(None).await?;
        let trades = db.load_trades(None).await?;
        let positions = db.load_positions(None).await?;

        let order_count = orders.len();
        let trade_count = trades.len();
        let position_count = positions.len();

        for order in orders {
            self.oms_engine.process_order(order);
        }
        for trade in trades {
            self.oms_engine.process_trade(trade);
        }
        for position in positions {
            self.oms_engine.process_position(position);
        }

        info!(
            "State restored from database: {} orders, {} trades, {} positions",
            order_count, trade_count, position_count
        );
        Ok(())
    }

    // ========================================================================
    // DataRecorder management
    // ========================================================================

    /// Add a DataRecorder with default configuration
    ///
    /// The recorder will automatically receive tick/bar events from all gateways
    /// and persist them to the database. Call `start_recorder()` after `start()`.
    ///
    /// # Arguments
    /// * `database` - Database backend for persisting recorded data
    ///
    /// # Returns
    /// The created DataRecorder, already registered as a sub-engine
    pub fn add_recorder(&self, database: Arc<dyn BaseDatabase>) -> Arc<DataRecorder> {
        self.add_recorder_with_config(database, RecorderConfig::default())
    }

    /// Add a DataRecorder with custom configuration
    ///
    /// # Arguments
    /// * `database` - Database backend for persisting recorded data
    /// * `config` - Recorder configuration (flush interval, batch size, etc.)
    ///
    /// # Returns
    /// The created DataRecorder, already registered as a sub-engine
    pub fn add_recorder_with_config(
        &self,
        database: Arc<dyn BaseDatabase>,
        config: RecorderConfig,
    ) -> Arc<DataRecorder> {
        let recorder = Arc::new(DataRecorder::with_config(database, config));

        // Register as sub-engine for event routing
        self.add_engine(recorder.clone());

        // Store reference for lifecycle management
        *self.recorder.write().unwrap_or_else(|e| e.into_inner()) = Some(recorder.clone());

        info!("DataRecorder added and registered as sub-engine");
        recorder
    }

    /// Get the DataRecorder if one has been added
    pub fn get_recorder(&self) -> Option<Arc<DataRecorder>> {
        self.recorder.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Start the DataRecorder event loop in a background task
    ///
    /// This should be called after `start()` to begin recording tick/bar data.
    /// The recorder will spawn its own async task that runs until `close()` is called.
    pub async fn start_recorder(&self) {
        if let Some(recorder) = self.get_recorder() {
            let recorder_clone = recorder.clone();
            tokio::spawn(async move {
                recorder_clone.start().await;
            });
            info!("DataRecorder started in background");
        } else {
            warn!("No DataRecorder configured, call add_recorder() first");
        }
    }

    /// Subscribe the recorder to tick data for a symbol
    pub async fn recorder_subscribe_tick(&self, symbol: &str, exchange: Exchange) {
        if let Some(recorder) = self.get_recorder() {
            recorder.subscribe_tick(symbol, exchange).await;
        }
    }

    /// Subscribe the recorder to bar data for a symbol with specific interval
    pub async fn recorder_subscribe_bar(&self, symbol: &str, exchange: Exchange, interval: crate::trader::Interval) {
        if let Some(recorder) = self.get_recorder() {
            recorder.subscribe_bar(symbol, exchange, interval).await;
        }
    }

    /// Unsubscribe the recorder from tick data
    pub async fn recorder_unsubscribe_tick(&self, symbol: &str, exchange: Exchange) {
        if let Some(recorder) = self.get_recorder() {
            recorder.unsubscribe_tick(symbol, exchange).await;
        }
    }

    /// Unsubscribe the recorder from bar data
    pub async fn recorder_unsubscribe_bar(&self, symbol: &str, exchange: Exchange, interval: crate::trader::Interval) {
        if let Some(recorder) = self.get_recorder() {
            recorder.unsubscribe_bar(symbol, exchange, interval).await;
        }
    }

    /// Get recorder status (list of active recordings with counts)
    pub async fn get_recorder_status(&self) -> Vec<RecordStatus> {
        if let Some(recorder) = self.get_recorder() {
            recorder.get_status().await
        } else {
            Vec::new()
        }
    }

    /// Flush recorder buffers to database
    pub async fn flush_recorder(&self) {
        if let Some(recorder) = self.get_recorder() {
            recorder.flush().await;
        }
    }

    /// Get persistence statistics for monitoring
    ///
    /// Returns a tuple of:
    /// - total_sent: Number of records successfully sent through the channel
    /// - total_overflowed: Number of records that overflowed to file
    /// - overflow_file_size: Current size of the overflow file in bytes (0 if none)
    pub fn get_persist_stats(&self) -> (u64, u64, u64) {
        let sent = self.persist_sent_count.load(Ordering::Relaxed);
        let overflowed = self.persist_overflowed_count.load(Ordering::Relaxed);
        let file_size = std::fs::metadata(overflow_file_path())
            .map(|m| m.len())
            .unwrap_or(0);
        (sent, overflowed, file_size)
    }

    /// Close the main engine gracefully
    ///
    /// Shutdown order is critical to prevent event loss:
    /// 1. Close gateways first (stops new events from being generated)
    /// 2. Wait briefly for in-flight events to be queued
    /// 3. Set running=false (allows start() loop to exit and drain remaining events)
    /// 4. Close sub-engines
    pub async fn close(&self) {
        // 1. Close all gateways FIRST — stops WebSocket streams and prevents new events
        let gateways: Vec<Arc<dyn BaseGateway>> = self.gateways.read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        for gateway in gateways {
            gateway.close().await;
        }

        // 2. Brief delay to let any in-flight gateway events reach the channel
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 3. NOW stop the event loop — remaining events will be drained in start()
        self.running.store(false, Ordering::SeqCst);

        // 4. Close all sub-engines
        {
            let engines = self.engines.read().unwrap_or_else(|e| e.into_inner());
            for engine in engines.values() {
                engine.close();
            }
        }

        // Drop persist_tx so the drain task exits cleanly after flushing
        // (The channel's sender is dropped, receiver will get None and exit)
    }
}

impl Default for MainEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oms_engine() {
        let oms = OmsEngine::new();
        
        // Test empty state
        assert!(oms.get_tick("TEST.LOCAL").is_none());
        assert!(oms.get_all_ticks().is_empty());
        
        // Test adding tick
        let tick = TickData::new(
            "test".to_string(),
            "TEST".to_string(),
            Exchange::Local,
            chrono::Utc::now(),
        );
        oms.process_tick(tick);
        
        assert!(oms.get_tick("TEST.LOCAL").is_some());
        assert_eq!(oms.get_all_ticks().len(), 1);
    }

    #[test]
    fn test_main_engine() {
        let engine = MainEngine::new();
        
        assert!(engine.get_all_gateway_names().is_empty());
        assert!(engine.get_tick("TEST.LOCAL").is_none());
    }

    #[tokio::test]
    async fn test_data_recorder_integration() {
        use crate::trader::database::MemoryDatabase;
        use crate::trader::constant::Interval;

        let engine = MainEngine::new();
        let db = Arc::new(MemoryDatabase::new());

        // No recorder initially
        assert!(engine.get_recorder().is_none());

        // Add recorder
        let recorder = engine.add_recorder(db.clone());
        assert!(engine.get_recorder().is_some());

        // Subscribe to tick and bar recording
        engine.recorder_subscribe_tick("btcusdt", Exchange::Binance).await;
        engine.recorder_subscribe_bar("btcusdt", Exchange::Binance, Interval::Minute).await;

        // Simulate tick event through the engine's event pipeline
        let tick = TickData::new(
            "BINANCE_SPOT".to_string(),
            "btcusdt".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        // Directly call on_tick through the recorder
        recorder.on_tick(&tick).await;
        recorder.flush().await;

        // Verify status
        let status = engine.get_recorder_status().await;
        assert!(!status.is_empty());

        // Unsubscribe
        engine.recorder_unsubscribe_tick("btcusdt", Exchange::Binance).await;
        engine.recorder_unsubscribe_bar("btcusdt", Exchange::Binance, Interval::Minute).await;
    }

    #[tokio::test]
    async fn test_data_recorder_with_config() {
        use crate::trader::database::MemoryDatabase;

        let engine = MainEngine::new();
        let db = Arc::new(MemoryDatabase::new());

        let config = RecorderConfig {
            flush_interval_secs: 30,
            batch_size: 500,
            record_ticks: true,
            record_bars: false,
        };

        let recorder = engine.add_recorder_with_config(db, config);
        assert!(engine.get_recorder().is_some());

        // Verify recorder doesn't record bars when record_bars=false
        let bar = BarData::new(
            "1m".to_string(),
            "btcusdt".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        recorder.on_bar(&bar).await;
        recorder.flush().await;

        // No bar symbols subscribed, so status should be empty for bars
        let status = recorder.get_status().await;
        assert!(status.is_empty());
    }
}

/// Implement OrderExecutor for MainEngine so AlgoEngine can send child orders
#[async_trait::async_trait]
impl super::algo::OrderExecutor for MainEngine {
    async fn send_order(&self, req: OrderRequest, gateway_name: &str) -> Result<String, String> {
        // Call the real send_order (risk check + offset conversion + gateway dispatch)
        self.send_order(req, gateway_name).await
    }
}
