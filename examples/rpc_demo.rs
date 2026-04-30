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
//! RPC Demo - Demonstrates the RPC client-server functionality
//!
//! This example shows how to:
//! - Create and start an RPC server
//! - Register RPC functions
//! - Create and start an RPC client
//! - Make remote procedure calls
//! - Subscribe to published messages
//! - Handle heartbeats

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use trade_engine::rpc::client::{ClientConfig, RpcClient};
use trade_engine::rpc::server::{ServerConfig, RpcServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("trade_engine=debug,info")
        .init();

    println!("=== RPC Demo ===\n");

    // Create server configuration
    let server_config = ServerConfig {
        rep_address: "tcp://*:2014".to_string(),
        pub_address: "tcp://*:4102".to_string(),
        heartbeat_interval: Duration::from_secs(10),
    };

    // Create and start server
    println!("Creating RPC server...");
    let server = Arc::new(RpcServer::with_config(server_config));
    server.start().await.map_err(|e| format!("Server start failed: {:?}", e))?;
    println!("✓ Server started\n");

    // Register some RPC functions
    println!("Registering RPC functions...");
    
    // Register a simple echo function
    server.register("echo".to_string(), |args, _kwargs| {
        if let Some(arg) = args.first() {
            Ok(arg.clone())
        } else {
            Ok(serde_json::json!("No arguments provided"))
        }
    }).await;

    // Register an add function
    server.register("add".to_string(), |args, _| {
        if args.len() >= 2 {
            let a = args[0].as_i64().unwrap_or(0);
            let b = args[1].as_i64().unwrap_or(0);
            Ok(serde_json::json!(a + b))
        } else {
            Ok(serde_json::json!("Need at least 2 arguments"))
        }
    }).await;

    // Register a greet function with named parameters
    server.register("greet".to_string(), |_, kwargs| {
        let name = kwargs.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("World");
        Ok(serde_json::json!(format!("Hello, {}!", name)))
    }).await;

    // Register a get_time function
    server.register("get_time".to_string(), |_, _| {
        let now = chrono::Utc::now();
        Ok(serde_json::json!(now.to_rfc3339()))
    }).await;

    println!("✓ Registered functions: echo, add, greet, get_time\n");

    // Give server time to initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client configuration
    let client_config = ClientConfig {
        req_address: "tcp://localhost:2014".to_string(),
        sub_address: "tcp://localhost:4102".to_string(),
        timeout_ms: 30000,
        heartbeat_tolerance: Duration::from_secs(30),
        cache_size: 100,
    };

    // Create and start client
    println!("Creating RPC client...");
    let client = Arc::new(RpcClient::with_config(client_config));
    client.start().await.map_err(|e| format!("Client start failed: {:?}", e))?;
    println!("✓ Client started\n");

    // Give client time to connect
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test RPC calls
    println!("=== Testing RPC Calls ===\n");

    // Test echo
    println!("Calling 'echo' with 'Hello RPC'...");
    let result = client.call("echo".to_string(), 
        vec![serde_json::json!("Hello RPC")], 
        HashMap::new()
    ).await.map_err(|e| format!("Echo call failed: {:?}", e))?;
    println!("✓ Response: {}\n", result);

    // Test add
    println!("Calling 'add' with [5, 3]...");
    let result = client.call("add".to_string(), 
        vec![serde_json::json!(5), serde_json::json!(3)], 
        HashMap::new()
    ).await.map_err(|e| format!("Add call failed: {:?}", e))?;
    println!("✓ Response: {}\n", result);

    // Test greet with positional args
    println!("Calling 'greet' with name='Alice'...");
    let mut kwargs = HashMap::new();
    kwargs.insert("name".to_string(), serde_json::json!("Alice"));
    let result = client.call("greet".to_string(), vec![], kwargs).await.map_err(|e| format!("Greet call failed: {:?}", e))?;
    println!("✓ Response: {}\n", result);

    // Test get_time
    println!("Calling 'get_time'...");
    let result = client.call("get_time".to_string(), vec![], HashMap::new()).await.map_err(|e| format!("Get time call failed: {:?}", e))?;
    println!("✓ Response: {}\n", result);

    // Test error handling
    println!("Calling non-existent function...");
    match client.call("nonexistent".to_string(), vec![], HashMap::new()).await {
        Ok(_) => println!("✗ Should have failed\n"),
        Err(e) => println!("✓ Got expected error: {}\n", e),
    }

    // Test subscriptions
    println!("=== Testing Subscriptions ===\n");

    // Set up callback for received messages
    let callback_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let callback_count_clone = callback_count.clone();

    client.set_callback(move |topic, data| {
        let count = callback_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!("  [{}] Received message #{}: {}", topic, count + 1, data);
    }).await;

    // Subscribe to a topic
    println!("Subscribing to topic 'market_data'...");
    client.subscribe_topic("market_data".to_string()).await.map_err(|e| format!("Subscribe failed: {:?}", e))?;
    println!("✓ Subscribed\n");

    // Publish some messages from server
    println!("Server publishing messages...");
    tokio::time::sleep(Duration::from_millis(100)).await;

    server.publish("market_data".to_string(), 
        serde_json::json!({"symbol": "BTCUSDT", "price": 50000.0})
    ).await.map_err(|e| format!("Publish 1 failed: {:?}", e))?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    server.publish("market_data".to_string(), 
        serde_json::json!({"symbol": "ETHUSDT", "price": 3000.0})
    ).await.map_err(|e| format!("Publish 2 failed: {:?}", e))?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    server.publish("market_data".to_string(), 
        serde_json::json!({"symbol": "SOLUSDT", "price": 150.0})
    ).await.map_err(|e| format!("Publish 3 failed: {:?}", e))?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("✓ Published 3 messages\n");

    // Test heartbeat
    println!("=== Testing Heartbeat ===\n");
    println!("Waiting for heartbeat messages...");
    println!("Client is connected: {}\n", client.is_connected().await);

    // Wait for a few heartbeat cycles
    tokio::time::sleep(Duration::from_secs(15)).await;

    println!("Client is still connected: {}\n", client.is_connected().await);

    // Cleanup
    println!("=== Cleanup ===\n");
    println!("Stopping client...");
    client.stop().await;
    println!("✓ Client stopped\n");

    println!("Stopping server...");
    server.stop().await;
    println!("✓ Server stopped\n");

    println!("=== Demo Complete ===");
    Ok(())
}