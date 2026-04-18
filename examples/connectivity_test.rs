//! Binance connectivity test — verifies REST API access using gateway config.
//!
//! Run: cargo run --example connectivity_test

use std::collections::HashMap;

use trade_engine::gateway::binance::{BinanceRestClient, SPOT_REST_HOST, Security};

#[tokio::main]
async fn main() {
    println!("=== Binance Connectivity Test ===\n");

    // Load config
    let config_path = dirs::home_dir()
        .map(|h| h.join(".rstrader").join("binance").join("gateway_configs.json"))
        .expect("Cannot find home directory");

    let config_text = std::fs::read_to_string(&config_path)
        .expect("Cannot read gateway config - run the GUI first to create it");

    let config: serde_json::Value = serde_json::from_str(&config_text)
        .expect("Invalid JSON in gateway config");

    let spot = config.get("gateways")
        .and_then(|g| g.get("BINANCE_SPOT"))
        .expect("BINANCE_SPOT config not found");

    let key = spot["key"].as_str().expect("Missing API key");
    let secret = spot["secret"].as_str().expect("Missing API secret");
    let proxy_host = spot["proxy_host"].as_str().expect("Missing proxy_host");
    let proxy_port = spot["proxy_port"].as_u64().expect("Missing proxy_port") as u16;

    println!("Config loaded:");
    println!("  API Key: {}...{}", &key[..8], &key[key.len()-4..]);
    println!("  Proxy: {}:{}", proxy_host, proxy_port);
    println!();

    // Create and init REST client
    let client = BinanceRestClient::new();
    client.init(key, secret, SPOT_REST_HOST, proxy_host, proxy_port).await;

    // Test 1: Server time (no auth needed)
    println!("[Test 1] Fetching server time...");
    let mut params = HashMap::new();
    match client.get("/api/v3/time", &params, Security::None).await {
        Ok(resp) => {
            let server_time = resp["serverTime"].as_i64().expect("Missing serverTime");
            let local_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as i64;
            let offset = (server_time - local_time).abs();
            println!("  ✅ Server time: {} (offset: {}ms)", server_time, offset);
            if offset > 5000 {
                println!("  ⚠️  Large time offset detected — sync may be needed");
            }
        }
        Err(e) => {
            println!("  ❌ Failed: {}", e);
        }
    }

    // Test 2: Account info (signed)
    println!("\n[Test 2] Fetching account info...");
    match client.get("/api/v3/account", &params, Security::Signed).await {
        Ok(resp) => {
            let balances: Vec<&serde_json::Value> = resp["balances"]
                .as_array()
                .map(|a| a.iter().filter(|b| b["free"].as_str().map(|v| v != "0.00000000").unwrap_or(false)).collect())
                .unwrap_or_default();
            println!("  ✅ Account info retrieved successfully");
            println!("  Non-zero balances: {}", balances.len());
            for b in balances.iter().take(5) {
                let asset = b["asset"].as_str().expect("missing asset");
                let free = b["free"].as_str().expect("missing free");
                println!("    {} : {} (free)", asset, free);
            }
            if balances.len() > 5 {
                println!("    ... and {} more", balances.len() - 5);
            }
        }
        Err(e) => {
            println!("  ❌ Failed: {}", e);
        }
    }

    // Test 3: Latest kline for BTCUSDT (public)
    println!("\n[Test 3] Fetching BTCUSDT latest kline...");
    params.insert("symbol".to_string(), "BTCUSDT".to_string());
    params.insert("interval".to_string(), "1m".to_string());
    params.insert("limit".to_string(), "1".to_string());
    match client.get("/api/v3/klines", &params, Security::None).await {
        Ok(resp) => {
            if let Some(arr) = resp.as_array().and_then(|a| a.first()).and_then(|k| k.as_array()) {
                let close = arr[4].as_str().expect("missing close");
                println!("  ✅ BTCUSDT last close: ${}", close);
            } else {
                println!("  ⚠️  Unexpected kline response format");
            }
        }
        Err(e) => {
            println!("  ❌ Failed: {}", e);
        }
    }

    println!("\n=== Connectivity Test Complete ===");
}
