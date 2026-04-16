//! MCP Resources 实现
//!
//! 实现 ServerHandler 的 list_resources 和 read_resource 方法，
//! 提供交易数据（ticks / orders / positions / accounts / trades / contracts）
//! 和 UI 状态（current_symbol / chart_indicators）作为 MCP Resource。

use rmcp::{model::*, ErrorData as McpError};
use serde_json::json;
use std::sync::{Arc, RwLock};

use super::types::UIState;
use crate::trader::MainEngine;

/// 列出所有可用 MCP Resources
pub fn list_resources(ui_state: &Arc<RwLock<UIState>>) -> Vec<Resource> {
    let mut resources = vec![
        make_resource("trading://ticks", "Real-time tick data"),
        make_resource("trading://orders", "All orders"),
        make_resource("trading://active_orders", "Active orders"),
        make_resource("trading://positions", "All positions"),
        make_resource("trading://accounts", "All accounts"),
        make_resource("trading://trades", "All trades"),
        make_resource("trading://contracts", "All contracts"),
    ];

    // UI 状态资源
    let state = ui_state.read().map(|s| s.clone()).unwrap_or_default();
    if state.current_symbol.is_some() {
        resources.push(make_resource("ui://current_symbol", "Current symbol"));
    }
    resources.push(make_resource("ui://chart_indicators", "Chart indicators"));
    resources.push(make_resource("ui://active_tab", "Active tab"));

    resources
}

/// 读取指定 URI 的 MCP Resource 数据
pub fn read_resource(
    uri: &str,
    engine: &Arc<MainEngine>,
    ui_state: &Arc<RwLock<UIState>>,
) -> Result<ReadResourceResult, McpError> {
    match uri {
        "trading://ticks" => {
            let ticks = engine.get_all_ticks();
            let text = serde_json::to_string_pretty(&ticks)
                .unwrap_or_else(|_| "Failed to serialize ticks".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://orders" => {
            let orders = engine.get_all_orders();
            let text = serde_json::to_string_pretty(&orders)
                .unwrap_or_else(|_| "Failed to serialize orders".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://active_orders" => {
            let orders = engine.get_all_active_orders();
            let text = serde_json::to_string_pretty(&orders)
                .unwrap_or_else(|_| "Failed to serialize active orders".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://positions" => {
            let positions = engine.get_all_positions();
            let text = serde_json::to_string_pretty(&positions)
                .unwrap_or_else(|_| "Failed to serialize positions".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://accounts" => {
            let accounts = engine.get_all_accounts();
            let text = serde_json::to_string_pretty(&accounts)
                .unwrap_or_else(|_| "Failed to serialize accounts".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://trades" => {
            let trades = engine.get_all_trades();
            let text = serde_json::to_string_pretty(&trades)
                .unwrap_or_else(|_| "Failed to serialize trades".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "trading://contracts" => {
            let contracts = engine.get_all_contracts();
            let summary: Vec<serde_json::Value> = contracts
                .iter()
                .map(|c| {
                    json!({
                        "vt_symbol": c.vt_symbol(),
                        "name": c.name,
                        "exchange": format!("{}", c.exchange),
                    })
                })
                .collect();
            let text = serde_json::to_string_pretty(&summary)
                .unwrap_or_else(|_| "Failed to serialize contracts".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "ui://current_symbol" => {
            let state = ui_state.read().map(|s| s.clone()).unwrap_or_default();
            let text = state.current_symbol.unwrap_or_else(|| "None".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "ui://chart_indicators" => {
            let state = ui_state.read().map(|s| s.clone()).unwrap_or_default();
            let text = serde_json::to_string_pretty(&state.chart_indicators)
                .unwrap_or_else(|_| "[]".to_string());
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                uri.to_string(),
            )]))
        }
        "ui://active_tab" => {
            let state = ui_state.read().map(|s| s.clone()).unwrap_or_default();
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                state.active_tab,
                uri.to_string(),
            )]))
        }
        _ => Err(McpError::resource_not_found(
            "resource_not_found",
            Some(json!({ "uri": uri })),
        )),
    }
}

/// 创建一个简单 Resource 对象
fn make_resource(uri: &str, name: &str) -> Resource {
    RawResource::new(uri, name.to_string()).no_annotation()
}
