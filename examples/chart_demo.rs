// Allow pedantic lints (personal project, pragmatic approach)
#![allow(
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::redundant_closure,
    clippy::filter_map_identity,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::unnecessary_wraps,
    clippy::unnecessary_debug_formatting,
    clippy::uninlined_format_args,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::wildcard_imports,
    clippy::if_not_else,
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::used_underscore_binding,
    clippy::unreadable_literal,
    clippy::non_std_lazy_statics,
    clippy::default_trait_access,
    clippy::semicolon_if_nothing_returned,
    clippy::redundant_closure_for_method_calls,
    clippy::float_cmp,
    clippy::implicit_hasher,
    clippy::unused_async,
    clippy::unnested_or_patterns,
    clippy::manual_let_else,
    clippy::single_char_pattern,
    clippy::assigning_clones,
    clippy::cloned_instead_of_copied,
    clippy::explicit_iter_loop,
    clippy::implicit_clone,
    clippy::trivially_copy_pass_by_ref,
    clippy::struct_excessive_bools,
    clippy::ignored_unit_patterns,
    clippy::match_wildcard_for_single_variants,
    clippy::unnecessary_literal_bound,
    clippy::manual_string_new,
    clippy::inefficient_to_string,
    clippy::no_effect_underscore_binding,
    clippy::doc_link_with_quotes,
    clippy::doc_lazy_continuation,
    clippy::format_push_string,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::enum_glob_use,
    clippy::unnecessary_map_or,
    clippy::comparison_chain,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::missing_fields_in_debug,
    clippy::field_reassign_with_default,
    clippy::derivable_impls,
    clippy::manual_midpoint,
    // Cast lints (trading code uses f64/i64 casts extensively)
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
//! Chart demo example showing candlestick and volume charts.
//!
//! Run with: cargo run --example chart_demo --features gui

use chrono::{Duration, Utc};
use eframe::egui;
use trade_engine::chart::ChartWidget;
use trade_engine::trader::object::BarData;
use trade_engine::trader::constant::{Exchange, Interval};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Trade Engine - Chart Demo"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Chart Demo",
        options,
        Box::new(|cc| {
            // Setup Chinese fonts
            trade_engine::trader::ui::style::setup_chinese_fonts(&cc.egui_ctx);
            trade_engine::trader::ui::style::apply_dark_theme(&cc.egui_ctx);
            
            Ok(Box::new(ChartDemoApp::new()))
        }),
    )
}

struct ChartDemoApp {
    chart: ChartWidget,
    auto_update: bool,
    last_bar_time: chrono::DateTime<Utc>,
}

impl ChartDemoApp {
    fn new() -> Self {
        let mut chart = ChartWidget::new();
        chart.set_price_decimals(2);
        chart.set_show_volume(true);
        chart.set_volume_height_ratio(0.25);
        
        // Generate sample data
        let bars = generate_sample_bars(500);
        let last_bar_time = bars.last().map(|b| b.datetime).unwrap_or_else(Utc::now);
        chart.update_history(bars);
        
        Self {
            chart,
            auto_update: false,
            last_bar_time,
        }
    }
    
    fn add_new_bar(&mut self) {
        let new_time = self.last_bar_time + Duration::minutes(1);
        
        // Get the last close price, handling the case when there are no bars
        let last_close = if self.chart.manager.get_count() > 0 {
            self.chart.manager.get_bar(
                (self.chart.manager.get_count() - 1) as f64
            ).map(|b| b.close_price).unwrap_or(100.0)
        } else {
            100.0
        };
        
        // Generate random OHLC based on last close
        let change = (rand_f64() - 0.5) * 2.0;
        let open = last_close;
        let close = last_close + change;
        let high = open.max(close) + rand_f64() * 0.5;
        let low = open.min(close) - rand_f64() * 0.5;
        let volume = 1000.0 + rand_f64() * 500.0;
        
        let bar = BarData {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: new_time,
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover: volume * close,
            open_interest: 0.0,
            gateway_name: "BINANCE".to_string(),
            extra: None,
        };
        
        self.chart.update_bar(bar);
        self.last_bar_time = new_time;
    }
}

impl eframe::App for ChartDemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Auto-update if enabled
        if self.auto_update {
            self.add_new_bar();
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("K线图表演示");
                ui.separator();
                
                if ui.button("添加数据").clicked() {
                    self.add_new_bar();
                }
                
                ui.checkbox(&mut self.auto_update, "自动更新");
                
                ui.separator();
                
                if ui.button("清空数据").clicked() {
                    self.chart.clear_all();
                }
                
                if ui.button("重新加载").clicked() {
                    let bars = generate_sample_bars(500);
                    self.last_bar_time = bars.last().map(|b| b.datetime).unwrap_or_else(Utc::now);
                    self.chart.update_history(bars);
                }
                
                ui.separator();
                
                ui.label(format!("数据条数: {}", self.chart.manager.get_count()));
            });
            
            ui.horizontal(|ui| {
                ui.label("操作提示:");
                ui.label("← → 移动  |  ↑ ↓ 缩放  |  鼠标拖拽平移  |  滚轮缩放  |  Home/End 跳转");
            });
        });
        
        egui::CentralPanel::default().show(ctx, |ui| {
            self.chart.show(ui, None);
        });
    }
}

/// Generate sample bar data for testing
fn generate_sample_bars(count: usize) -> Vec<BarData> {
    let mut bars = Vec::with_capacity(count);
    let mut price = 100.0_f64;
    let start_time = Utc::now() - Duration::minutes(count as i64);
    
    for i in 0..count {
        // Random walk with trend
        let trend = (i as f64 / count as f64 - 0.5) * 0.1;
        let change = (rand_f64() - 0.5) * 2.0 + trend;
        
        let open = price;
        let close = price + change;
        let high = open.max(close) + rand_f64() * 0.5;
        let low = open.min(close) - rand_f64() * 0.5;
        let volume = 1000.0 + rand_f64() * 500.0 + (change.abs() * 200.0);
        
        bars.push(BarData {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: start_time + Duration::minutes(i as i64),
            interval: Some(Interval::Minute),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            turnover: volume * close,
            open_interest: 0.0,
            gateway_name: "BINANCE".to_string(),
            extra: None,
        });
        
        price = close;
    }
    
    bars
}

/// Simple pseudo-random number generator
fn rand_f64() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    ((nanos as f64 * 1.1) % 1000.0) / 1000.0
}
