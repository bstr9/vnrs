//! Event-driven framework for the trading engine.
//! Based on the VeighNa framework's event system.

mod engine;

pub use engine::{Event, EventEngine, EVENT_TIMER};