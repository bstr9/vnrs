//! Remote Monitoring Panel
//!
//! GUI panel for monitoring RPC server status, event broadcast statistics,
//! and connected client information.

use egui::{Color32, Grid, RichText, Ui};
use super::style::{COLOR_TEXT_SECONDARY, COLOR_TEXT_PRIMARY, COLOR_POSITIVE, COLOR_NEGATIVE};

/// RPC monitoring panel state
pub struct RpcPanel {
    /// Configured RPC port (REP socket)
    rpc_port: u16,
    /// Whether the RPC server is running
    is_running: bool,
    /// Total events published since startup
    events_published: u64,
    /// Last error message (if any)
    last_error: Option<String>,
}

impl Default for RpcPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcPanel {
    pub fn new() -> Self {
        Self {
            rpc_port: 5555,
            is_running: false,
            events_published: 0,
            last_error: None,
        }
    }

    /// Update panel state from environment / engine
    pub fn set_rpc_port(&mut self, port: u16) {
        self.rpc_port = port;
    }

    /// Set the running state
    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }

    /// Update event broadcast counter
    pub fn set_events_published(&mut self, count: u64) {
        self.events_published = count;
    }

    /// Render the panel
    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("远程监控 (RPC)");
        ui.add_space(8.0);

        // Status indicator
        let (status_text, status_color) = if self.is_running {
            ("运行中", COLOR_POSITIVE)
        } else {
            ("已停止", COLOR_NEGATIVE)
        };

        ui.horizontal(|ui| {
            ui.label(RichText::new("服务器状态:").color(COLOR_TEXT_PRIMARY));
            // Colored dot indicator
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(10.0, 10.0),
                egui::Sense::hover(),
            );
            ui.painter().circle_filled(rect.center(), 5.0, status_color);
            ui.add_space(4.0);
            ui.label(RichText::new(status_text).color(status_color).strong());
        });

        ui.add_space(8.0);

        // Configuration display
        Grid::new("rpc_config_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label(RichText::new("REP 端口:").color(COLOR_TEXT_SECONDARY));
                ui.label(RichText::new(format!("tcp://*:{}", self.rpc_port)).color(COLOR_TEXT_PRIMARY));
                ui.end_row();

                ui.label(RichText::new("PUB 端口:").color(COLOR_TEXT_SECONDARY));
                ui.label(RichText::new(format!("tcp://*:{}", self.rpc_port + 1)).color(COLOR_TEXT_PRIMARY));
                ui.end_row();

                ui.label(RichText::new("协议:").color(COLOR_TEXT_SECONDARY));
                ui.label(RichText::new("ZMQ (REP/REQ + PUB/SUB)").color(COLOR_TEXT_PRIMARY));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Broadcast statistics
        ui.label(RichText::new("事件广播统计").strong());
        ui.add_space(4.0);

        Grid::new("rpc_stats_grid")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                ui.label(RichText::new("已发布事件:").color(COLOR_TEXT_SECONDARY));
                ui.label(RichText::new(format!("{}", self.events_published)).color(COLOR_TEXT_PRIMARY));
                ui.end_row();

                ui.label(RichText::new("服务器状态:").color(COLOR_TEXT_SECONDARY));
                ui.label(RichText::new(if self.is_running { "活跃" } else { "未启动" }).color(status_color));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Available RPC functions
        ui.label(RichText::new("可用 RPC 函数").strong());
        ui.add_space(4.0);

        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            let functions = [
                ("查询类", vec!["get_tick", "get_bar", "get_order", "get_trade", "get_position", "get_account", "get_contract", "get_quote"]),
                ("批量查询", vec!["get_all_ticks", "get_all_bars", "get_all_orders", "get_all_trades", "get_all_positions", "get_all_accounts", "get_all_contracts", "get_all_quotes", "get_all_active_orders", "get_all_active_quotes", "get_all_logs"]),
                ("交易操作", vec!["send_order", "cancel_order", "subscribe"]),
                ("网关管理", vec!["connect", "disconnect", "query_history", "get_all_gateway_names", "get_all_exchanges"]),
                ("策略控制", vec!["start_strategy", "stop_strategy"]),
                ("其他", vec!["write_log"]),
            ];

            for (category, funcs) in functions {
                ui.label(RichText::new(category).color(COLOR_TEXT_SECONDARY).size(12.0));
                for func in funcs {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new(format!("• {}", func)).color(COLOR_TEXT_PRIMARY).size(12.0));
                    });
                }
                ui.add_space(2.0);
            }
        });

        ui.add_space(8.0);

        // Show last error if any
        if let Some(ref err) = self.last_error {
            ui.colored_label(Color32::RED, format!("错误: {}", err));
        }

        ui.add_space(8.0);

        // Connection hint
        ui.horizontal(|ui| {
            ui.label(RichText::new("Python 连接示例:").color(COLOR_TEXT_SECONDARY).size(11.0));
        });
        ui.add_space(2.0);
        let code = format!(
            "from vnpy_rpc import RpcClient\nclient = RpcClient()\nclient.connect('localhost:{}', 'localhost:{}')",
            self.rpc_port, self.rpc_port + 1
        );
        ui.code_editor(&mut code.clone());
    }
}
