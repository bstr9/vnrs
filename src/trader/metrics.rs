//! Prometheus metrics engine for monitoring trading events.
//!
//! This module provides optional Prometheus metrics support, gated behind the
//! `prometheus` feature flag. When enabled, it exposes a `/metrics` HTTP endpoint
//! and collects core trading metrics automatically via the `BaseEngine` trait.

use once_cell::sync::Lazy;
use prometheus::{Counter, Gauge, Registry, TextEncoder, opts};

use super::engine::BaseEngine;
use super::gateway::GatewayEvent;

// ---------------------------------------------------------------------------
// Prometheus metric definitions
// ---------------------------------------------------------------------------

/// Custom Prometheus registry (avoids polluting the default global registry).
pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

pub static ORDERS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    Counter::with_opts(opts!("vnrs_orders_total", "Total orders").namespace("vnrs")).unwrap()
});

pub static TRADES_TOTAL: Lazy<Counter> = Lazy::new(|| {
    Counter::with_opts(opts!("vnrs_trades_total", "Total trades").namespace("vnrs")).unwrap()
});

pub static TICK_COUNT: Lazy<Counter> = Lazy::new(|| {
    Counter::with_opts(opts!("vnrs_tick_count", "Total ticks received").namespace("vnrs")).unwrap()
});

pub static POSITION_VALUE: Lazy<Gauge> = Lazy::new(|| {
    Gauge::with_opts(opts!("vnrs_position_value", "Position value").namespace("vnrs")).unwrap()
});

pub static PNL_TOTAL: Lazy<Gauge> = Lazy::new(|| {
    Gauge::with_opts(opts!("vnrs_pnl_total", "Total PnL").namespace("vnrs")).unwrap()
});

pub static STRATEGY_ACTIVE: Lazy<Gauge> = Lazy::new(|| {
    Gauge::with_opts(opts!("vnrs_strategy_active", "Active strategies").namespace("vnrs")).unwrap()
});

pub static BALANCE: Lazy<Gauge> = Lazy::new(|| {
    Gauge::with_opts(opts!("vnrs_balance", "Account balance").namespace("vnrs")).unwrap()
});

// ---------------------------------------------------------------------------
// Initialization — register all metrics with the custom registry
// ---------------------------------------------------------------------------

/// Register all metrics with the custom REGISTRY.
/// Must be called once before the first scrape; safe to call multiple times
/// (subsequent calls are no-ops).
pub fn init() {
    // The Lazy<T> values are created on first access, so we touch each one
    // to force initialization, then register them if not already registered.
    macro_rules! register {
        ($metric:expr) => {
            // Ignore AlreadyReg errors — init() may be called more than once.
            let _ = REGISTRY.register(Box::new($metric.clone()));
        };
    }

    register!(ORDERS_TOTAL);
    register!(TRADES_TOTAL);
    register!(TICK_COUNT);
    register!(POSITION_VALUE);
    register!(PNL_TOTAL);
    register!(STRATEGY_ACTIVE);
    register!(BALANCE);
}

// ---------------------------------------------------------------------------
// MetricsEngine — receives GatewayEvents and updates Prometheus counters/gauges
// ---------------------------------------------------------------------------

/// A `BaseEngine` implementation that increments Prometheus metrics in response
/// to gateway events.
pub struct MetricsEngine;

impl MetricsEngine {
    pub fn new() -> Self {
        Self
    }
}

impl BaseEngine for MetricsEngine {
    fn engine_name(&self) -> &str {
        "metrics"
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Tick(_) => {
                TICK_COUNT.inc();
            }
            GatewayEvent::Order(_) => {
                ORDERS_TOTAL.inc();
            }
            GatewayEvent::Trade(_) => {
                TRADES_TOTAL.inc();
            }
            GatewayEvent::Position(p) => {
                POSITION_VALUE.set(p.frozen + p.pnl);
                PNL_TOTAL.set(p.pnl);
            }
            GatewayEvent::Account(a) => {
                BALANCE.set(a.balance);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// MetricsServer — serves /metrics via hyper
// ---------------------------------------------------------------------------

use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use tokio::net::TcpListener;

/// A lightweight HTTP server that exposes Prometheus metrics on `/metrics`.
pub struct MetricsServer {
    addr: String,
}

impl MetricsServer {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }

    /// Start the metrics HTTP server in a background tokio task.
    pub fn start(&self) {
        let addr = self.addr.clone();
        tokio::spawn(async move {
            if let Err(e) = run_server(&addr).await {
                tracing::error!("Prometheus metrics server error: {e}");
            }
        });
    }
}

async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Prometheus metrics server listening on {addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = hyper_util::rt::TokioIo::new(stream);

        tokio::spawn(async move {
            let service = service_fn(|_req: Request<hyper::body::Incoming>| async move {
                let output = metrics_output();
                Response::builder()
                    .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
                    .body::<http_body_util::Full<Bytes>>(http_body_util::Full::new(
                        Bytes::from(output),
                    ))
            });
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                tracing::debug!("Metrics HTTP connection error: {e}");
            }
        });
    }
}

/// Gather all registered metrics and encode them in the Prometheus text format.
fn metrics_output() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    encoder.encode_to_string(&metric_families).unwrap_or_default()
}
