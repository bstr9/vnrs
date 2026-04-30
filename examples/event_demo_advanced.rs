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
use trade_engine::event::{Event, EventEngine, EVENT_TIMER};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
    println!("Starting Event Engine Demo");
    
    // Create event engine with 1 second interval
    let mut engine = EventEngine::new(1);
    
    // Counter to track different event types
    let timer_counter = Arc::new(Mutex::new(0));
    let custom_counter = Arc::new(Mutex::new(0));
    
    // Register a handler for timer events
    let timer_counter_clone = Arc::clone(&timer_counter);
    let timer_handler = Arc::new(move |_event: &Event| {
        let mut count = timer_counter_clone.lock().unwrap();
        *count += 1;
        println!("Timer event received! Timer count: {}", *count);
        
        // Stop after 3 timer events
        if *count >= 3 {
            println!("Stopping event engine after {} timer events", *count);
        }
    });
    let _timer_handler_id = engine.register(EVENT_TIMER, timer_handler);
    
    // Register a handler for custom events
    let custom_counter_clone = Arc::clone(&custom_counter);
    let custom_handler = Arc::new(move |_event: &Event| {
        let mut count = custom_counter_clone.lock().unwrap();
        *count += 1;
        println!("Custom event received! Custom count: {}", *count);
    });
    let custom_handler_id = engine.register("eCustom", custom_handler);
    
    // Register a general handler that receives all events
    let general_counter = Arc::new(Mutex::new(0));
    let general_counter_clone = Arc::clone(&general_counter);
    let general_handler = Arc::new(move |_event: &Event| {
        let mut count = general_counter_clone.lock().unwrap();
        *count += 1;
        println!("General handler - Total events processed: {}", *count);
    });
    let _general_handler_id = engine.register_general(general_handler);
    
    // Clone the sender for use in the thread
    let engine_sender = engine.sender();
    
    // Start the engine
    engine.start();
    
    // Send a few custom events after some delay
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        println!("Sending first custom event");
        let _ = engine_sender.send(Event::new("eCustom".to_string(), None));
        
        thread::sleep(Duration::from_millis(1500));
        println!("Sending second custom event");
        let _ = engine_sender.send(Event::new("eCustom".to_string(), None));
        
        thread::sleep(Duration::from_millis(2500));
        println!("Sending third custom event");
        let _ = engine_sender.send(Event::new("eCustom".to_string(), None));
    });
    
    // Let it run for a while
    thread::sleep(Duration::from_secs(8));
    
    // Unregister the custom handler
    println!("Unregistering custom event handler");
    engine.unregister("eCustom", custom_handler_id);
    
    // Send another custom event after unregistering
    thread::sleep(Duration::from_millis(100));
    println!("Sending custom event after unregistering handler");
    engine.put(Event::new("eCustom".to_string(), None));
    
    // Let it run a bit more
    thread::sleep(Duration::from_secs(2));
    
    // Stop the engine
    engine.stop();
    
    println!("Event engine stopped");
    
    // Print final counts
    println!("Final timer count: {}", *timer_counter.lock().unwrap());
    println!("Final custom count: {}", *custom_counter.lock().unwrap());
    println!("Final general count: {}", *general_counter.lock().unwrap());
}