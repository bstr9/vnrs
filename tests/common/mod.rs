//! Common test infrastructure for integration tests
//!
//! This module provides reusable test helpers including:
//! - `fixtures`: Factory functions for creating test data
//! - `mock_gateway`: MockGateway implementing BaseGateway
//! - `mock_strategy`: TestStrategy implementing StrategyTemplate
//! - `assertions`: Custom test assertions for floating-point comparison

pub mod assertions;
pub mod fixtures;
pub mod mock_gateway;
pub mod mock_strategy;
