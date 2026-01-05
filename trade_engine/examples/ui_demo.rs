//! Example application demonstrating the trading UI.
//!
//! Run with: cargo run --example ui_demo --features gui

use eframe::egui;
use trade_engine::trader::ui::MainWindow;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Trade Engine - Demo"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Trade Engine",
        options,
        Box::new(|cc| {
            // Set up custom fonts if needed
            // let mut fonts = egui::FontDefinitions::default();
            // cc.egui_ctx.set_fonts(fonts);
            
            Ok(Box::new(DemoApp::new(cc)))
        }),
    )
}

struct DemoApp {
    main_window: MainWindow,
}

impl DemoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            main_window: MainWindow::new("Trade Engine Demo"),
        };
        
        // Set up demo gateways
        app.main_window.set_gateways(vec![
            "BINANCE".to_string(),
            "CTP".to_string(),
        ]);
        
        // Apply dark theme
        app.main_window.setup_style(&cc.egui_ctx);
        
        // Add some demo data
        app.add_demo_data();
        
        app
    }
    
    fn add_demo_data(&mut self) {
        use chrono::Utc;
        use trade_engine::trader::object::*;
        use trade_engine::trader::constant::*;
        
        // Demo tick data
        let tick = TickData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            name: "BTC/USDT".to_string(),
            volume: 12345.67,
            turnover: 0.0,
            open_interest: 0.0,
            last_price: 43250.50,
            last_volume: 1.5,
            limit_up: 0.0,
            limit_down: 0.0,
            open_price: 42800.0,
            high_price: 43500.0,
            low_price: 42500.0,
            pre_close: 42900.0,
            bid_price_1: 43249.0,
            bid_price_2: 43248.0,
            bid_price_3: 43247.0,
            bid_price_4: 43246.0,
            bid_price_5: 43245.0,
            ask_price_1: 43251.0,
            ask_price_2: 43252.0,
            ask_price_3: 43253.0,
            ask_price_4: 43254.0,
            ask_price_5: 43255.0,
            bid_volume_1: 2.5,
            bid_volume_2: 3.0,
            bid_volume_3: 1.8,
            bid_volume_4: 4.2,
            bid_volume_5: 5.0,
            ask_volume_1: 1.2,
            ask_volume_2: 2.8,
            ask_volume_3: 3.5,
            ask_volume_4: 2.1,
            ask_volume_5: 4.0,
            localtime: None,
            extra: None,
        };
        self.main_window.tick_monitor.update(&tick);
        
        // Demo order data
        let order = OrderData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "12345".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 43200.0,
            volume: 0.5,
            traded: 0.2,
            status: Status::PartTraded,
            datetime: Some(Utc::now()),
            reference: "Demo".to_string(),
            extra: None,
        };
        self.main_window.order_monitor.update(&order);
        self.main_window.active_order_monitor.update(&order);
        
        // Demo trade data
        let trade = TradeData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "12345".to_string(),
            tradeid: "T001".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 43200.0,
            volume: 0.2,
            datetime: Some(Utc::now()),
            extra: None,
        };
        self.main_window.trade_monitor.update(&trade);
        
        // Demo position data
        let position = PositionData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            volume: 1.5,
            frozen: 0.0,
            price: 42500.0,
            pnl: 1125.75,
            yd_volume: 1.0,
            extra: None,
        };
        self.main_window.position_monitor.update(&position);
        
        // Demo account data
        let account = AccountData {
            gateway_name: "BINANCE".to_string(),
            accountid: "demo_account".to_string(),
            balance: 100000.0,
            frozen: 5000.0,
            extra: None,
        };
        self.main_window.account_monitor.update(&account);
        
        // Demo log data
        let log = LogData::new("BINANCE".to_string(), "Demo UI started".to_string());
        self.main_window.log_monitor.update(&log);
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.main_window.show(ctx);
        
        // Handle pending actions
        if let Some((gateway, settings)) = self.main_window.take_connect() {
            println!("Connecting to {}: {:?}", gateway, settings);
        }
        
        if let Some(orderid) = self.main_window.take_cancel_order() {
            println!("Canceling order: {}", orderid);
        }
        
        if self.main_window.should_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        
        // Handle trading widget actions
        if let Some(req) = self.main_window.trading.take_subscribe() {
            println!("Subscribe: {}.{}", req.symbol, req.exchange);
        }
        
        if let Some((order, gateway)) = self.main_window.trading.take_order() {
            println!("Send order to {}: {} {} {} @ {}", 
                gateway, order.symbol, order.direction, order.volume, order.price);
        }
        
        if self.main_window.trading.take_cancel_all() {
            println!("Cancel all orders");
        }
    }
}
