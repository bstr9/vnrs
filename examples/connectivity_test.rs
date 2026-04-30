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
    let client = BinanceRestClient::new().expect("Failed to create HTTP client");
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
