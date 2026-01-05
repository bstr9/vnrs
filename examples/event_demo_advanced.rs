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