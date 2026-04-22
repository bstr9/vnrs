//! Trading RPC service — bridges MainEngine with RpcServer
//!
//! Registers trading-related RPC functions on an RpcServer instance so that
//! remote clients can query state and submit/cancel orders over ZMQ.
//! Matches the vnpy `MainEngineRpc` pattern.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tracing::info;

use crate::rpc::server::RpcServer;
use crate::trader::engine::MainEngine;
use crate::trader::gateway::GatewaySettingValue;

/// Helper: extract a required string argument by position
fn arg_str(args: &[serde_json::Value], idx: usize, name: &str) -> Result<String, String> {
    args.get(idx)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("参数 {} (位置 {}) 缺失或类型错误", name, idx))
}

/// Helper: extract an optional string argument by position
fn arg_str_opt(args: &[serde_json::Value], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Register all trading RPC functions on the given server.
///
/// This is the main entry point — call it once after creating both
/// the `MainEngine` and `RpcServer`.
pub async fn register_trading_functions(engine: Arc<MainEngine>, server: &RpcServer) {
    // ========================================================================
    // Read-only queries (synchronous — read directly from OmsEngine)
    // ========================================================================

    let e = engine.clone();
    server
        .register("get_tick".to_string(), move |args, _kwargs| {
            let vt_symbol = arg_str(&args, 0, "vt_symbol")?;
            match e.get_tick(&vt_symbol) {
                Some(tick) => serde_json::to_value(tick).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_bar".to_string(), move |args, _kwargs| {
            let vt_symbol = arg_str(&args, 0, "vt_symbol")?;
            match e.get_bar(&vt_symbol) {
                Some(bar) => serde_json::to_value(bar).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_order".to_string(), move |args, _kwargs| {
            let vt_orderid = arg_str(&args, 0, "vt_orderid")?;
            match e.get_order(&vt_orderid) {
                Some(order) => serde_json::to_value(order).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_trade".to_string(), move |args, _kwargs| {
            let vt_tradeid = arg_str(&args, 0, "vt_tradeid")?;
            match e.get_trade(&vt_tradeid) {
                Some(trade) => serde_json::to_value(trade).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_position".to_string(), move |args, _kwargs| {
            let vt_positionid = arg_str(&args, 0, "vt_positionid")?;
            match e.get_position(&vt_positionid) {
                Some(pos) => serde_json::to_value(pos).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_account".to_string(), move |args, _kwargs| {
            let vt_accountid = arg_str(&args, 0, "vt_accountid")?;
            match e.get_account(&vt_accountid) {
                Some(acc) => serde_json::to_value(acc).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_contract".to_string(), move |args, _kwargs| {
            let vt_symbol = arg_str(&args, 0, "vt_symbol")?;
            match e.get_contract(&vt_symbol) {
                Some(c) => serde_json::to_value(c).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("get_quote".to_string(), move |args, _kwargs| {
            let vt_quoteid = arg_str(&args, 0, "vt_quoteid")?;
            match e.get_quote(&vt_quoteid) {
                Some(q) => serde_json::to_value(q).map_err(|err| err.to_string()),
                None => Ok(json!(null)),
            }
        })
        .await;

    // ========================================================================
    // Bulk read queries
    // ========================================================================

    let e = engine.clone();
    server
        .register("get_all_ticks".to_string(), move |_args, _kwargs| {
            let ticks = e.get_all_ticks();
            serde_json::to_value(ticks).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_bars".to_string(), move |_args, _kwargs| {
            let bars = e.get_all_bars();
            serde_json::to_value(bars).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_orders".to_string(), move |_args, _kwargs| {
            let orders = e.get_all_orders();
            serde_json::to_value(orders).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_trades".to_string(), move |_args, _kwargs| {
            let trades = e.get_all_trades();
            serde_json::to_value(trades).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_positions".to_string(), move |_args, _kwargs| {
            let positions = e.get_all_positions();
            serde_json::to_value(positions).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_accounts".to_string(), move |_args, _kwargs| {
            let accounts = e.get_all_accounts();
            serde_json::to_value(accounts).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_contracts".to_string(), move |_args, _kwargs| {
            let contracts = e.get_all_contracts();
            serde_json::to_value(contracts).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_quotes".to_string(), move |_args, _kwargs| {
            let quotes = e.get_all_quotes();
            serde_json::to_value(quotes).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_active_orders".to_string(), move |_args, _kwargs| {
            let orders = e.get_all_active_orders();
            serde_json::to_value(orders).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_active_quotes".to_string(), move |_args, _kwargs| {
            let quotes = e.get_all_active_quotes();
            serde_json::to_value(quotes).map_err(|err| err.to_string())
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_logs".to_string(), move |_args, _kwargs| {
            let logs = e.get_all_logs();
            serde_json::to_value(logs).map_err(|err| err.to_string())
        })
        .await;

    // ========================================================================
    // Gateway management
    // ========================================================================

    let e = engine.clone();
    server
        .register("get_all_gateway_names".to_string(), move |_args, _kwargs| {
            let names = e.get_all_gateway_names();
            Ok(json!(names))
        })
        .await;

    let e = engine.clone();
    server
        .register("get_all_exchanges".to_string(), move |_args, _kwargs| {
            let exchanges: Vec<String> = e
                .get_all_exchanges()
                .iter()
                .map(|ex| ex.value().to_string())
                .collect();
            Ok(json!(exchanges))
        })
        .await;

    // ========================================================================
    // Async trading operations
    // These require spawning a tokio task since RpcFunction is sync.
    // We use a oneshot channel to wait for the result synchronously.
    // ========================================================================

    let e = engine.clone();
    server
        .register("send_order".to_string(), move |args, kwargs| {
            // Try to parse OrderRequest from kwargs first, then from args[0]
            let req: crate::trader::object::OrderRequest = if !kwargs.is_empty() {
                serde_json::from_value(serde_json::to_value(&kwargs).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("OrderRequest 解析失败: {}", e))?
            } else if !args.is_empty() {
                serde_json::from_value(args[0].clone())
                    .map_err(|e| format!("OrderRequest 解析失败: {}", e))?
            } else {
                return Err("send_order 需要 OrderRequest 参数".to_string());
            };

            let gateway_name = if !args.is_empty() && args.len() > 1 {
                arg_str(&args, 1, "gateway_name")?
            } else if let Some(gw) = kwargs.get("gateway_name").and_then(|v| v.as_str()) {
                gw.to_string()
            } else {
                return Err("send_order 需要 gateway_name 参数".to_string());
            };

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
            tokio::spawn(async move {
                let result = e_clone.send_order(req, &gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(vt_orderid)) => Ok(json!(vt_orderid)),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("send_order 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("cancel_order".to_string(), move |args, kwargs| {
            let req: crate::trader::object::CancelRequest = if !kwargs.is_empty() {
                serde_json::from_value(serde_json::to_value(&kwargs).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("CancelRequest 解析失败: {}", e))?
            } else if !args.is_empty() {
                serde_json::from_value(args[0].clone())
                    .map_err(|e| format!("CancelRequest 解析失败: {}", e))?
            } else {
                return Err("cancel_order 需要 CancelRequest 参数".to_string());
            };

            let gateway_name = if !args.is_empty() && args.len() > 1 {
                arg_str(&args, 1, "gateway_name")?
            } else if let Some(gw) = kwargs.get("gateway_name").and_then(|v| v.as_str()) {
                gw.to_string()
            } else {
                return Err("cancel_order 需要 gateway_name 参数".to_string());
            };

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.cancel_order(req, &gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(())) => Ok(json!("cancelled")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("cancel_order 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("subscribe".to_string(), move |args, kwargs| {
            let req: crate::trader::object::SubscribeRequest = if !kwargs.is_empty() {
                serde_json::from_value(serde_json::to_value(&kwargs).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("SubscribeRequest 解析失败: {}", e))?
            } else if !args.is_empty() {
                serde_json::from_value(args[0].clone())
                    .map_err(|e| format!("SubscribeRequest 解析失败: {}", e))?
            } else {
                return Err("subscribe 需要 SubscribeRequest 参数".to_string());
            };

            let gateway_name = if !args.is_empty() && args.len() > 1 {
                arg_str(&args, 1, "gateway_name")?
            } else if let Some(gw) = kwargs.get("gateway_name").and_then(|v| v.as_str()) {
                gw.to_string()
            } else {
                return Err("subscribe 需要 gateway_name 参数".to_string());
            };

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.subscribe(req, &gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(())) => Ok(json!("subscribed")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("subscribe 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("connect".to_string(), move |args, _kwargs| {
            let gateway_name = arg_str(&args, 0, "gateway_name")?;

            // Build GatewaySettings from kwargs-like map passed as args[1] or remaining args
            // For simplicity, accept a JSON object as args[1] for settings
            let settings: HashMap<String, GatewaySettingValue> = if args.len() > 1 {
                let settings_json = &args[1];
                parse_gateway_settings(settings_json)?
            } else {
                HashMap::new()
            };

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.connect(settings, &gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(())) => Ok(json!("connected")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("connect 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("disconnect".to_string(), move |args, _kwargs| {
            let gateway_name = arg_str(&args, 0, "gateway_name")?;

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.disconnect(&gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(())) => Ok(json!("disconnected")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("disconnect 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("query_history".to_string(), move |args, kwargs| {
            let req: crate::trader::object::HistoryRequest = if !kwargs.is_empty() {
                serde_json::from_value(serde_json::to_value(&kwargs).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("HistoryRequest 解析失败: {}", e))?
            } else if !args.is_empty() {
                serde_json::from_value(args[0].clone())
                    .map_err(|e| format!("HistoryRequest 解析失败: {}", e))?
            } else {
                return Err("query_history 需要 HistoryRequest 参数".to_string());
            };

            let gateway_name = if !args.is_empty() && args.len() > 1 {
                arg_str(&args, 1, "gateway_name")?
            } else if let Some(gw) = kwargs.get("gateway_name").and_then(|v| v.as_str()) {
                gw.to_string()
            } else {
                return Err("query_history 需要 gateway_name 参数".to_string());
            };

            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<crate::trader::object::BarData>, String>>();
            tokio::spawn(async move {
                let result = e_clone.query_history(req, &gateway_name).await;
                let _ = tx.send(result);
            });

            match rx.recv() {
                Ok(Ok(bars)) => serde_json::to_value(bars).map_err(|e| e.to_string()),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("query_history 内部通信失败".to_string()),
            }
        })
        .await;

    // ========================================================================
    // Write log
    // ========================================================================

    let e = engine.clone();
    server
        .register("write_log".to_string(), move |args, _kwargs| {
            let msg = arg_str(&args, 0, "msg")?;
            let source = arg_str_opt(&args, 1).unwrap_or_else(|| "RPC".to_string());
            e.write_log(msg, &source);
            Ok(json!("logged"))
        })
        .await;

    // ========================================================================
    // Strategy control
    // ========================================================================

    let e = engine.clone();
    server
        .register("start_strategy".to_string(), move |args, _kwargs| {
            let strategy_name = arg_str(&args, 0, "strategy_name")?;
            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.start_strategy(&strategy_name).await;
                let _ = tx.send(result);
            });
            match rx.recv() {
                Ok(Ok(())) => Ok(json!("started")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("start_strategy 内部通信失败".to_string()),
            }
        })
        .await;

    let e = engine.clone();
    server
        .register("stop_strategy".to_string(), move |args, _kwargs| {
            let strategy_name = arg_str(&args, 0, "strategy_name")?;
            let e_clone = e.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            tokio::spawn(async move {
                let result = e_clone.stop_strategy(&strategy_name).await;
                let _ = tx.send(result);
            });
            match rx.recv() {
                Ok(Ok(())) => Ok(json!("stopped")),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("stop_strategy 内部通信失败".to_string()),
            }
        })
        .await;

    info!("已注册所有交易RPC函数");
}

/// Parse a JSON object into GatewaySettings
///
/// Accepts a map of string keys to values that can be strings, integers,
/// floats, or booleans. Converts each to the appropriate `GatewaySettingValue`.
fn parse_gateway_settings(
    value: &serde_json::Value,
) -> Result<HashMap<String, GatewaySettingValue>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| "Gateway settings 必须是 JSON 对象".to_string())?;

    let mut settings = HashMap::new();
    for (key, val) in map {
        let gsv = match val {
            serde_json::Value::String(s) => GatewaySettingValue::String(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    GatewaySettingValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    GatewaySettingValue::Float(f)
                } else {
                    GatewaySettingValue::String(n.to_string())
                }
            }
            serde_json::Value::Bool(b) => GatewaySettingValue::Bool(*b),
            other => GatewaySettingValue::String(other.to_string()),
        };
        settings.insert(key.clone(), gsv);
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gateway_settings_string() {
        let json = json!({"key": "value", "num": 42, "float": 3.14, "flag": true});
        let settings = parse_gateway_settings(&json).expect("解析失败");

        assert!(matches!(
            settings.get("key"),
            Some(GatewaySettingValue::String(s)) if s == "value"
        ));
        assert!(matches!(
            settings.get("num"),
            Some(GatewaySettingValue::Int(42))
        ));
        assert!(matches!(
            settings.get("float"),
            Some(GatewaySettingValue::Float(f)) if (*f - 3.14).abs() < 0.001
        ));
        assert!(matches!(
            settings.get("flag"),
            Some(GatewaySettingValue::Bool(true))
        ));
    }

    #[test]
    fn test_parse_gateway_settings_invalid() {
        let json = json!("not an object");
        let result = parse_gateway_settings(&json);
        assert!(result.is_err());
    }

    #[test]
    fn test_arg_helpers() {
        let args = vec![json!("hello"), json!("world")];

        assert_eq!(arg_str(&args, 0, "a").unwrap(), "hello");
        assert_eq!(arg_str(&args, 1, "b").unwrap(), "world");
        assert!(arg_str(&args, 3, "c").is_err());
        assert_eq!(arg_str_opt(&args, 1), Some("world".to_string()));
        assert_eq!(arg_str_opt(&args, 5), None);
    }
}
