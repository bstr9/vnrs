//! Trade Engine - Main Application Entry Point
//!
//! A high-performance trading platform similar to vnpy_evo,
//! implemented in Rust with egui GUI support.

use std::sync::Arc;
use std::error::Error;
use eframe::egui;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use trade_engine::trader::{MainEngine, GatewayEventSender};
use trade_engine::trader::recorder::DataRecorder;
use trade_engine::trader::database::FileDatabase;
use trade_engine::event::EventEngine;
use trade_engine::gateway::binance::{BinanceSpotGateway, BinanceUsdtGateway};

#[cfg(feature = "gui")]
use trade_engine::trader::ui::MainWindow;
#[cfg(feature = "gui")]
use trade_engine::mcp::{TradingMcpServer, UICommand};

/// Application state holding all trading components
struct TradeEngineApp {
    /// Main trading engine
    main_engine: Arc<MainEngine>,
    /// Event engine
    #[allow(dead_code)]
    event_engine: Arc<EventEngine>,
    /// Main window UI (GUI mode only)
    #[cfg(feature = "gui")]
    main_window: MainWindow,
    /// MCP Server (optional, only if GUI feature enabled)
    #[cfg(feature = "gui")]
    #[allow(dead_code)]
    mcp_server: Option<TradingMcpServer>,
    /// MCP command receiver
    #[cfg(feature = "gui")]
    mcp_command_rx: Option<tokio::sync::mpsc::UnboundedReceiver<UICommand>>,
    /// Runtime handle for async operations
    runtime: tokio::runtime::Handle,
}

impl TradeEngineApp {
    /// Create a new application instance
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Setup custom fonts if needed
        Self::setup_fonts(&cc.egui_ctx);
        
        // Create tokio runtime for async operations
        let runtime = tokio::runtime::Handle::current();
        
        // Create the event engine
        let event_engine = Arc::new(EventEngine::new(10)); // 10ms timer interval
        info!("✅ 事件引擎已创建");
        
        // Create the main engine
        let main_engine = Arc::new(MainEngine::new());
        info!("✅ 主引擎已创建");
        
        // Get event sender for gateways
        let event_sender = main_engine.get_event_sender();

        // Register Binance gateways
        let binance_spot = Arc::new(BinanceSpotGateway::new("BINANCE_SPOT"));
        let binance_usdt = Arc::new(BinanceUsdtGateway::new("BINANCE_USDT"));
        
        // Set event senders for gateways (run async task)
        {
            let spot_sender = GatewayEventSender::new("BINANCE_SPOT".to_string(), event_sender.clone());
            let usdt_sender = GatewayEventSender::new("BINANCE_USDT".to_string(), event_sender.clone());
            let spot = binance_spot.clone();
            let usdt = binance_usdt.clone();
            runtime.spawn(async move {
                spot.set_event_sender(spot_sender).await;
                usdt.set_event_sender(usdt_sender).await;
            });
        }
        
        main_engine.add_gateway(binance_spot);
        main_engine.add_gateway(binance_usdt);
        info!("✅ 已注册 Binance 网关");
        
        // Start the main engine event loop
        {
            let engine = main_engine.clone();
            runtime.spawn(async move {
                engine.start().await;
            });
        }
        
        // Create main window UI
        #[cfg(feature = "gui")]
        let mut main_window = MainWindow::new("Trade Engine");
        
        // Setup available gateways
        #[cfg(feature = "gui")]
        {
            main_window.set_gateways(vec![
                "BINANCE_SPOT".to_string(),
                "BINANCE_USDT".to_string(),
            ]);
            
            // Set main engine reference
            main_window.set_main_engine(main_engine.clone());
            
            // Create and wire strategy engine
            let strategy_engine = Arc::new(trade_engine::strategy::StrategyEngine::new(
                main_engine.clone(),
                event_engine.clone(),
            ));
            // Register strategy engine as a sub-engine of MainEngine so it receives
            // tick/bar/order/trade events directly (GAP 6 fix)
            main_engine.add_engine(strategy_engine.clone());
            {
                let se = strategy_engine.clone();
                runtime.spawn(async move {
                    se.init().await;
                });
            }
            main_window.set_strategy_engine(strategy_engine);
            
            // Create and wire DataRecorder engine for automatic tick/bar recording (GAP 5 fix)
            let data_recorder = Arc::new(DataRecorder::new(
                Arc::new(FileDatabase::with_default_dir()),
            ));
            main_engine.add_engine(data_recorder.clone());
            {
                let recorder = data_recorder.clone();
                runtime.spawn(async move {
                    recorder.start().await;
                });
            }
            info!("✅ DataRecorder 已注册并启动");
            
            // Apply dark theme
            main_window.setup_style(&cc.egui_ctx);
        }
        
        // Create MCP Server (after main_engine is created)
        #[cfg(feature = "gui")]
        let (mcp_server, mcp_command_rx) = TradingMcpServer::new(main_engine.clone());
        
        info!("✅ Trading Engine 启动完成");
        
        Self {
            main_engine,
            event_engine,
            #[cfg(feature = "gui")]
            main_window,
            #[cfg(feature = "gui")]
            mcp_server: Some(mcp_server),
            #[cfg(feature = "gui")]
            mcp_command_rx: Some(mcp_command_rx),
            runtime,
        }
    }
    
    /// Setup custom fonts for Chinese text support
    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        
        // Try to load Chinese font from system
        let font_paths = [
            "C:\\Windows\\Fonts\\msyh.ttc",      // Microsoft YaHei
            "C:\\Windows\\Fonts\\simsun.ttc",   // SimSun
            "C:\\Windows\\Fonts\\simhei.ttf",   // SimHei
        ];
        
        let mut font_loaded = false;
        for font_path in font_paths {
            if let Ok(font_data) = std::fs::read(font_path) {
                fonts.font_data.insert(
                    "chinese_font".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(font_data)),
                );
                
                // Add to proportional and monospace families
                fonts.families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "chinese_font".to_owned());
                    
                fonts.families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("chinese_font".to_owned());
                
                info!("✅ 已加载中文字体: {}", font_path);
                font_loaded = true;
                break;
            }
        }
        
        if !font_loaded {
            warn!("⚠️ 未能加载中文字体，将使用默认字体");
        }
        
        ctx.set_fonts(fonts);
    }
    
    /// Handle pending UI actions
    #[cfg(feature = "gui")]
    fn process_ui_actions(&mut self) {
        // Handle gateway connection requests
        if let Some((gateway_name, settings)) = self.main_window.take_connect() {
            info!("连接请求: {} with settings: {:?}", gateway_name, settings);
            
            // Convert settings to GatewaySettings
            let mut gateway_settings = trade_engine::trader::GatewaySettings::new();
            for (key, value) in settings {
                let setting_value = match value {
                    serde_json::Value::String(s) => trade_engine::trader::GatewaySettingValue::String(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            trade_engine::trader::GatewaySettingValue::Int(i)
                        } else if let Some(f) = n.as_f64() {
                            trade_engine::trader::GatewaySettingValue::Float(f)
                        } else {
                            continue;
                        }
                    }
                    serde_json::Value::Bool(b) => trade_engine::trader::GatewaySettingValue::Bool(b),
                    _ => continue,
                };
                gateway_settings.insert(key, setting_value);
            }
            
            // Spawn async connect task
            let engine = self.main_engine.clone();
            let gw_name = gateway_name.clone();
            self.runtime.spawn(async move {
                match engine.connect(gateway_settings, &gw_name).await {
                    Ok(_) => info!("✅ {} 连接成功", gw_name),
                    Err(e) => warn!("❌ {} 连接失败: {}", gw_name, e),
                }
            });
        }
        
        // Handle cancel order requests
        if let Some(vt_orderid) = self.main_window.take_cancel_order() {
            info!("撤单请求: {}", vt_orderid);
            // Parse vt_orderid and create cancel request
            let parts: Vec<&str> = vt_orderid.split('.').collect();
            if parts.len() >= 2 {
                let gateway_name = parts[0];
                let orderid = parts[1..].join(".");
                
                let req = trade_engine::trader::CancelRequest {
                    orderid,
                    symbol: String::new(),
                    exchange: trade_engine::trader::Exchange::Binance,
                };
                
                let engine = self.main_engine.clone();
                let gw_name = gateway_name.to_string();
                self.runtime.spawn(async move {
                    if let Err(e) = engine.cancel_order(req, &gw_name).await {
                        warn!("❌ 撤单失败: {}", e);
                    }
                });
            }
        }
        
        // Handle cancel quote requests
        if let Some(vt_quoteid) = self.main_window.take_cancel_quote() {
            info!("撤报价请求: {}", vt_quoteid);
            // Similar handling as cancel order
        }
        
        // Handle subscribe requests
        if let Some((req, gateway_name)) = self.main_window.take_subscribe() {
            info!("订阅行情请求: {:?} via {}", req, gateway_name);
            let engine = self.main_engine.clone();
            self.runtime.spawn(async move {
                match engine.subscribe(req, &gateway_name).await {
                    Ok(_) => info!("✅ 订阅成功"),
                    Err(e) => warn!("❌ 订阅失败: {}", e),
                }
            });
        }
        
        // Handle order requests
        if let Some((req, gateway_name)) = self.main_window.take_order() {
            info!("下单请求: {:?} via {}", req, gateway_name);
            let engine = self.main_engine.clone();
            self.runtime.spawn(async move {
                match engine.send_order(req, &gateway_name).await {
                    Ok(vt_orderid) => info!("✅ 下单成功: {}", vt_orderid),
                    Err(e) => warn!("❌ 下单失败: {}", e),
                }
            });
        }
    }
}

impl eframe::App for TradeEngineApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "gui")]
        {
            // Update data from main engine
            self.main_window.update_data();
            
            // Show main window UI
            self.main_window.show(ctx);
            
            // Process MCP commands (non-blocking)
            if let Some(ref mut rx) = self.mcp_command_rx {
                while let Ok(cmd) = rx.try_recv() {
                    self.main_window.handle_mcp_command(cmd);
                }
            }
            
            // Process any pending UI actions
            self.process_ui_actions();
            
            // Check if user requested close
            if self.main_window.should_close() {
                info!("🛑 用户请求退出...");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        
        #[cfg(not(feature = "gui"))]
        {
            // Headless mode - just show basic info
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Trade Engine (Headless Mode)");
                ui.label("GUI feature is disabled. Enable it with --features gui");
            });
        }
        
        // Request repaint for smooth updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        info!("🛑 正在关闭 Trading Engine...");
        
        // Close main engine
        let engine = self.main_engine.clone();
        self.runtime.block_on(async {
            engine.close().await;
        });
        
        info!("✅ Trading Engine 已关闭");
    }
}

/// Initialize logging system
fn setup_logging() {
    // Initialize tracing subscriber with env filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    // Note: Don't call init_logger() here as it will try to set another global default
    // The trader logger is only for the trader module's internal use
}

/// Create native window options
fn create_native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Trade Engine - Rust Trading Platform")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(load_app_icon()),
        ..Default::default()
    }
}

/// Load application icon
fn load_app_icon() -> egui::IconData {
    // Return a default icon - you can replace this with a custom icon
    egui::IconData::default()
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    
    // Setup logging
    setup_logging();
    
    println!("╔═══════════════════════════════════════════════════╗");
    println!("║     🚀 Trade Engine - Rust Trading Platform 🚀    ║");
    println!("║                  Version {}                    ║", trade_engine::VERSION);
    println!("╚═══════════════════════════════════════════════════╝");
    println!();
    
    info!("🚀 启动 Trade Engine...");
    info!("📦 版本: {}", trade_engine::VERSION);
    info!("🦀 Rust 版本: {}", rustc_version_runtime::version());
    
    // Check for MCP mode via environment variable
    #[cfg(feature = "gui")]
    if std::env::var("MCP_MODE").is_ok() {
        // Run in MCP stdio mode (for Claude Desktop)
        let event_engine = Arc::new(EventEngine::new(10));
        let main_engine = Arc::new(MainEngine::new());
        
        // Register Binance gateways
        let binance_spot = Arc::new(BinanceSpotGateway::new("BINANCE_SPOT"));
        let binance_usdt = Arc::new(BinanceUsdtGateway::new("BINANCE_USDT"));
        let event_sender = main_engine.get_event_sender();
        let spot_sender = GatewayEventSender::new("BINANCE_SPOT".to_string(), event_sender.clone());
        let usdt_sender = GatewayEventSender::new("BINANCE_USDT".to_string(), event_sender);
        let spot = binance_spot.clone();
        let usdt = binance_usdt.clone();
        runtime.spawn(async move {
            spot.set_event_sender(spot_sender).await;
            usdt.set_event_sender(usdt_sender).await;
        });
        main_engine.add_gateway(binance_spot);
        main_engine.add_gateway(binance_usdt);
        
        // Start event engine
        let engine = main_engine.clone();
        runtime.spawn(async move {
            engine.start().await;
        });
        drop(event_engine);
        
        let (mcp_server, _) = TradingMcpServer::new(main_engine);
        runtime.block_on(async {
            if let Err(e) = mcp_server.serve_stdio().await {
                tracing::error!("MCP Server error: {}", e);
            }
        });
        return Ok(());
    }
    
    // Run the application
    eframe::run_native(
        "Trade Engine",
        create_native_options(),
        Box::new(|cc| Ok(Box::new(TradeEngineApp::new(cc)))),
    ).map_err(|e| format!("Failed to run application: {}", e))?;
    
    Ok(())
}