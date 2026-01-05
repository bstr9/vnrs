//! Event engine for the trading system.
//! This module provides an event-driven framework similar to VeighNa's event system
//! but implemented in Rust with thread-safe event handling.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::sync::mpsc as sync_mpsc;

/// Timer event type constant
pub const EVENT_TIMER: &str = "eTimer";

/// Event data structure
#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: String,
    pub data: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl Event {
    pub fn new(event_type: String, data: Option<Arc<dyn std::any::Any + Send + Sync>>) -> Self {
        Event { event_type, data }
    }
}

/// Type alias for event handler functions
pub type EventHandler = Arc<dyn Fn(&Event) + Send + Sync>;

/// A wrapper that allows us to identify handlers for removal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerId(usize);

/// Event Engine that distributes events based on their type
pub struct EventEngine {
    /// Channel sender for putting events
    sender: sync_mpsc::Sender<Event>,
    /// Channel receiver for processing events
    receiver: Arc<Mutex<Option<sync_mpsc::Receiver<Event>>>>,
    /// Map of event type to handlers
    handlers: Arc<Mutex<HashMap<String, Vec<(HandlerId, EventHandler)>>>>,
    /// General handlers that receive all events
    general_handlers: Arc<Mutex<Vec<(HandlerId, EventHandler)>>>,
    /// Counter for generating unique handler IDs
    handler_counter: Arc<Mutex<usize>>,
    /// Flag to control engine running state
    active: Arc<Mutex<bool>>,
    /// Timer thread handle
    timer_handle: Option<thread::JoinHandle<()>>,
    /// Processing thread handle
    processing_handle: Option<thread::JoinHandle<()>>,
    /// Timer interval in seconds
    interval: u64,
}

impl EventEngine {
    pub fn new(interval: u64) -> Self {
        let (sender, receiver) = sync_mpsc::channel();
        
        EventEngine {
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
            handlers: Arc::new(Mutex::new(HashMap::new())),
            general_handlers: Arc::new(Mutex::new(Vec::new())),
            handler_counter: Arc::new(Mutex::new(0)),
            active: Arc::new(Mutex::new(false)),
            timer_handle: None,
            processing_handle: None,
            interval,
        }
    }

    /// Generate a new unique handler ID
    fn generate_handler_id(&self) -> HandlerId {
        let mut counter = self.handler_counter.lock().unwrap();
        *counter += 1;
        HandlerId(*counter)
    }

    /// Start the event engine
    pub fn start(&mut self) {
        let mut active = self.active.lock().unwrap();
        *active = true;
        drop(active);

        // Start timer thread
        let active_clone = Arc::clone(&self.active);
        let sender_clone = self.sender.clone();
        let interval = self.interval;
        
        self.timer_handle = Some(thread::spawn(move || {
            while *active_clone.lock().unwrap() {
                thread::sleep(Duration::from_secs(interval));
                
                let timer_event = Event::new(EVENT_TIMER.to_string(), None);
                let _ = sender_clone.send(timer_event);
            }
        }));

        // Start event processing loop
        let active_clone = Arc::clone(&self.active);
        let handlers_clone = Arc::clone(&self.handlers);
        let general_handlers_clone = Arc::clone(&self.general_handlers);
        let receiver_opt = self.receiver.lock().unwrap().take();
        if let Some(event_receiver) = receiver_opt {
            self.processing_handle = Some(thread::spawn(move || {
                // Process events in a loop with timeout to check active status periodically
                loop {
                    // Use try_recv with timeout to periodically check if engine is still active
                    match event_receiver.recv_timeout(Duration::from_millis(10)) {
                        Ok(event) => {
                            Self::process_event(
                                &event,
                                Arc::clone(&handlers_clone),
                                Arc::clone(&general_handlers_clone),
                            );
                        }
                        Err(sync_mpsc::RecvTimeoutError::Timeout) => {
                            // Check if the engine is still active
                            if !*active_clone.lock().unwrap() {
                                break; // Exit loop if engine is not active
                            }
                            // Continue to next iteration to check again
                        }
                        Err(sync_mpsc::RecvTimeoutError::Disconnected) => {
                            // Channel closed, exit loop
                            break;
                        }
                    }
                }
            }));
        }
    }

    /// Stop the event engine
    pub fn stop(&mut self) {
        let mut active = self.active.lock().unwrap();
        *active = false;
        drop(active);

        if let Some(handle) = self.timer_handle.take() {
            let _ = handle.join();
        }
        
        if let Some(handle) = self.processing_handle.take() {
            let _ = handle.join();
        }
    }

    /// Put an event into the queue
    pub fn put(&self, event: Event) {
        let _ = self.sender.send(event);
    }
    
    /// Get a clone of the sender to allow external event posting
    pub fn sender(&self) -> sync_mpsc::Sender<Event> {
        self.sender.clone()
    }

    /// Register a handler for a specific event type
    pub fn register(&self, event_type: &str, handler: EventHandler) -> HandlerId {
        let handler_id = self.generate_handler_id();
        let mut handlers = self.handlers.lock().unwrap();
        let handler_list = handlers.entry(event_type.to_string()).or_insert_with(Vec::new);
        
        // Add handler with its ID to the list
        handler_list.push((handler_id, handler));
        handler_id
    }

    /// Unregister a handler for a specific event type
    pub fn unregister(&self, event_type: &str, handler_id: HandlerId) {
        let mut handlers = self.handlers.lock().unwrap();
        if let Some(handler_list) = handlers.get_mut(event_type) {
            // Filter out the handler based on its ID
            handler_list.retain(|(id, _)| *id != handler_id);
            
            // Remove the key if no handlers left
            if handler_list.is_empty() {
                handlers.remove(event_type);
            }
        }
    }

    /// Register a general handler that receives all events
    pub fn register_general(&self, handler: EventHandler) -> HandlerId {
        let handler_id = self.generate_handler_id();
        let mut general_handlers = self.general_handlers.lock().unwrap();
        general_handlers.push((handler_id, handler));
        handler_id
    }

    /// Unregister a general handler
    pub fn unregister_general(&self, handler_id: HandlerId) {
        let mut general_handlers = self.general_handlers.lock().unwrap();
        general_handlers.retain(|(id, _)| *id != handler_id);
    }

    /// Process an event by calling appropriate handlers
    fn process_event(
        event: &Event,
        handlers: Arc<Mutex<HashMap<String, Vec<(HandlerId, EventHandler)>>>>,
        general_handlers: Arc<Mutex<Vec<(HandlerId, EventHandler)>>>,
    ) {
        // Call type-specific handlers
        {
            let handlers_map = handlers.lock().unwrap();
            if let Some(handler_list) = handlers_map.get(&event.event_type) {
                for (_, handler) in handler_list {
                    handler(event);
                }
            }
        }

        // Call general handlers
        {
            let general_handlers_list = general_handlers.lock().unwrap();
            for (_, handler) in &*general_handlers_list {
                handler(event);
            }
        }
    }
}

impl Drop for EventEngine {
    fn drop(&mut self) {
        self.stop();
    }
}