use trade_engine::event::{Event, EventEngine};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
    // Create event engine with 1 second interval
    let mut engine = EventEngine::new(1);
    
    // Counter to track events
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = Arc::clone(&counter);
    
    // Register a handler for timer events
    let timer_handler = Arc::new(move |_event: &Event| {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;
        println!("Timer event received! Count: {}", *count);
        
        // Stop after 5 events
        if *count >= 5 {
            println!("Stopping event engine after {} events", *count);
        }
    });
    
    engine.register("eTimer", timer_handler);
    
    // Start the engine
    engine.start();
    
    // Let it run for a while
    thread::sleep(Duration::from_secs(6));
    
    // Stop the engine
    engine.stop();
    
    println!("Event engine stopped");
}