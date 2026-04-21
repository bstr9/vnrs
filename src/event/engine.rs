//! Event engine for the trading system.
//! This module provides an event-driven framework similar to VeighNa's event system
//! but implemented in Rust with thread-safe event handling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc as sync_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::journal::EventJournal;

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
#[allow(clippy::type_complexity)]
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
    /// Optional event journal for deterministic replay
    journal: Arc<Mutex<Option<EventJournal>>>,
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
            journal: Arc::new(Mutex::new(None)),
        }
    }

    /// Generate a new unique handler ID
    fn generate_handler_id(&self) -> HandlerId {
        let mut counter = self
            .handler_counter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        HandlerId(*counter)
    }

    /// Start the event engine
    pub fn start(&mut self) {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        *active = true;
        drop(active);

        // Start timer thread
        let active_clone = Arc::clone(&self.active);
        let sender_clone = self.sender.clone();
        let interval = self.interval;

        self.timer_handle = Some(thread::spawn(move || {
            while *active_clone.lock().unwrap_or_else(|e| e.into_inner()) {
                thread::sleep(Duration::from_secs(interval));

                let timer_event = Event::new(EVENT_TIMER.to_string(), None);
                let _ = sender_clone.send(timer_event);
            }
        }));

        // Start event processing loop
        let active_clone = Arc::clone(&self.active);
        let handlers_clone = Arc::clone(&self.handlers);
        let general_handlers_clone = Arc::clone(&self.general_handlers);
        let receiver_opt = self
            .receiver
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
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
                            if !*active_clone.lock().unwrap_or_else(|e| e.into_inner()) {
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
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        *active = false;
        drop(active);

        if let Some(handle) = self.timer_handle.take() {
            let _ = handle.join();
        }

        if let Some(handle) = self.processing_handle.take() {
            let _ = handle.join();
        }
    }

    /// Put an event into the queue.
    /// If journaling is enabled, the event is recorded before dispatch.
    pub fn put(&self, event: Event) {
        // Journal the event BEFORE it enters the processing pipeline
        if let Ok(mut journal_guard) = self.journal.lock() {
            if let Some(ref mut journal) = *journal_guard {
                let _ = journal.append(&event);
            }
        }

        let _ = self.sender.send(event);
    }

    /// Get a clone of the sender to allow external event posting
    pub fn sender(&self) -> sync_mpsc::Sender<Event> {
        self.sender.clone()
    }

    /// Enable event journaling to the specified path.
    ///
    /// All subsequent events passed through `put()` will be recorded
    /// to the journal file before being dispatched to handlers.
    pub fn enable_journal(&mut self, path: PathBuf) -> Result<(), String> {
        let journal = EventJournal::open(path)?;
        let mut journal_guard = self
            .journal
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *journal_guard = Some(journal);
        Ok(())
    }

    /// Disable event journaling if currently active.
    pub fn disable_journal(&mut self) {
        let mut journal_guard = self
            .journal
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *journal_guard = None;
    }

    /// Replay events from a journal file into this engine.
    ///
    /// Events are replayed in strict timestamp-then-sequence order.
    /// Returns the number of events replayed.
    ///
    /// Note: The engine should be started before calling this method
    /// so that replayed events are actually processed by handlers.
    pub fn replay_from_journal(&mut self, path: PathBuf) -> Result<usize, String> {
        let journal = EventJournal::open(path)?;
        journal.replay(self)
    }

    /// Register a handler for a specific event type
    pub fn register(&self, event_type: &str, handler: EventHandler) -> HandlerId {
        let handler_id = self.generate_handler_id();
        let mut handlers = self.handlers.lock().unwrap_or_else(|e| e.into_inner());
        let handler_list = handlers.entry(event_type.to_string()).or_default();

        // Add handler with its ID to the list
        handler_list.push((handler_id, handler));
        handler_id
    }

    /// Unregister a handler for a specific event type
    pub fn unregister(&self, event_type: &str, handler_id: HandlerId) {
        let mut handlers = self.handlers.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut general_handlers = self
            .general_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        general_handlers.push((handler_id, handler));
        handler_id
    }

    /// Unregister a general handler
    pub fn unregister_general(&self, handler_id: HandlerId) {
        let mut general_handlers = self
            .general_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        general_handlers.retain(|(id, _)| *id != handler_id);
    }

    /// Process an event by calling appropriate handlers
    #[allow(clippy::type_complexity)]
    fn process_event(
        event: &Event,
        handlers: Arc<Mutex<HashMap<String, Vec<(HandlerId, EventHandler)>>>>,
        general_handlers: Arc<Mutex<Vec<(HandlerId, EventHandler)>>>,
    ) {
        // Call type-specific handlers
        {
            let handlers_map = handlers.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(handler_list) = handlers_map.get(&event.event_type) {
                for (_, handler) in handler_list {
                    handler(event);
                }
            }
        }

        // Call general handlers
        {
            let general_handlers_list = general_handlers.lock().unwrap_or_else(|e| e.into_inner());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_event_new() {
        let event = Event::new("test_event".to_string(), None);
        assert_eq!(event.event_type, "test_event");
        assert!(event.data.is_none());
    }

    #[test]
    fn test_event_engine_new() {
        let engine = EventEngine::new(1);
        let active = engine.active.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!*active);
    }

    #[test]
    fn test_event_engine_start_stop() {
        let mut engine = EventEngine::new(1);
        engine.start();
        let active = engine.active.lock().unwrap_or_else(|e| e.into_inner());
        assert!(*active);
        drop(active);

        engine.stop();
        let active = engine.active.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!*active);
    }

    #[test]
    fn test_register_and_emit_event() {
        let mut engine = EventEngine::new(1);
        let counter = Arc::new(AtomicUsize::new(0));

        let counter_clone = Arc::clone(&counter);
        engine.register(
            "test_event",
            Arc::new(move |_event| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        engine.start();

        let event = Event::new("test_event".to_string(), None);
        engine.put(event);

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        let count = counter.load(Ordering::SeqCst);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_multiple_handlers_same_event() {
        let mut engine = EventEngine::new(1);
        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        let c1 = Arc::clone(&counter1);
        engine.register(
            "test_event",
            Arc::new(move |_event| {
                c1.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let c2 = Arc::clone(&counter2);
        engine.register(
            "test_event",
            Arc::new(move |_event| {
                c2.fetch_add(1, Ordering::SeqCst);
            }),
        );

        engine.start();

        let event = Event::new("test_event".to_string(), None);
        engine.put(event);

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_general_handler_receives_all_events() {
        let mut engine = EventEngine::new(1);
        let general_counter = Arc::new(AtomicUsize::new(0));

        let gc = Arc::clone(&general_counter);
        engine.register_general(Arc::new(move |_event| {
            gc.fetch_add(1, Ordering::SeqCst);
        }));

        engine.start();

        engine.put(Event::new("event_a".to_string(), None));
        engine.put(Event::new("event_b".to_string(), None));

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        assert!(general_counter.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn test_unregister_handler() {
        let mut engine = EventEngine::new(1);
        let counter = Arc::new(AtomicUsize::new(0));

        let c = Arc::clone(&counter);
        let handler_id = engine.register(
            "test_event",
            Arc::new(move |_event| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );

        engine.unregister("test_event", handler_id);

        engine.start();

        engine.put(Event::new("test_event".to_string(), None));

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_unregister_general_handler() {
        let mut engine = EventEngine::new(1);
        let counter = Arc::new(AtomicUsize::new(0));

        let c = Arc::clone(&counter);
        let handler_id = engine.register_general(Arc::new(move |_event| {
            c.fetch_add(1, Ordering::SeqCst);
        }));

        engine.unregister_general(handler_id);

        engine.start();

        engine.put(Event::new("test_event".to_string(), None));

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_timer_event() {
        let mut engine = EventEngine::new(1);
        let timer_counter = Arc::new(AtomicUsize::new(0));

        let tc = Arc::clone(&timer_counter);
        engine.register(
            EVENT_TIMER,
            Arc::new(move |_event| {
                tc.fetch_add(1, Ordering::SeqCst);
            }),
        );

        engine.start();

        std::thread::sleep(Duration::from_millis(2500));
        engine.stop();

        assert!(timer_counter.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn test_handler_id_uniqueness() {
        let engine = EventEngine::new(1);
        let id1 = engine.generate_handler_id();
        let id2 = engine.generate_handler_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_put_without_start() {
        let engine = EventEngine::new(1);
        let event = Event::new("test_event".to_string(), None);
        engine.put(event);
    }

    #[test]
    fn test_sender_clone() {
        let engine = EventEngine::new(1);
        let sender = engine.sender();
        let event = Event::new("test_event".to_string(), None);
        let _ = sender.send(event);
    }

    #[test]
    fn test_enable_journal() {
        let dir = std::env::temp_dir().join("vnrs_journal_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_enable_journal.bin");
        let _ = std::fs::remove_file(&path);

        let mut engine = EventEngine::new(1);
        let result = engine.enable_journal(path.clone());
        assert!(result.is_ok());

        engine.put(Event::new("journal_test".to_string(), None));

        // Verify the journal file has content
        let data = std::fs::read(&path).unwrap();
        assert!(!data.is_empty(), "Journal file should have data after put()");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_replay_from_journal() {
        let dir = std::env::temp_dir().join("vnrs_journal_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_replay_from_journal.bin");
        let _ = std::fs::remove_file(&path);

        // Phase 1: Write events to journal
        {
            let mut engine = EventEngine::new(1);
            engine
                .enable_journal(path.clone())
                .unwrap();
            engine.put(Event::new("evt_a".to_string(), None));
            engine.put(Event::new("evt_b".to_string(), None));
            engine.put(Event::new("evt_c".to_string(), None));
        }

        // Phase 2: Replay into a new engine
        let mut engine = EventEngine::new(1);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);
        engine.register(
            "evt_a",
            Arc::new(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let c2 = Arc::clone(&counter);
        engine.register(
            "evt_b",
            Arc::new(move |_| {
                c2.fetch_add(10, Ordering::SeqCst);
            }),
        );
        let c3 = Arc::clone(&counter);
        engine.register(
            "evt_c",
            Arc::new(move |_| {
                c3.fetch_add(100, Ordering::SeqCst);
            }),
        );

        engine.start();

        let count = engine.replay_from_journal(path.clone()).unwrap();
        assert_eq!(count, 3);

        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        // 1 (evt_a) + 10 (evt_b) + 100 (evt_c) = 111
        assert_eq!(counter.load(Ordering::SeqCst), 111);

        let _ = std::fs::remove_file(&path);
    }
}
