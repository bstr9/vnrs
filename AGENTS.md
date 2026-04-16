# PROJECT KNOWLEDGE BASE

**Generated:** 2026-04-16
**Commit:** dc10d81
**Branch:** master

## OVERVIEW
High-performance algorithmic trading engine in Rust (inspired by vnpy). Features: event-driven architecture, Python bindings (PyO3), egui GUI, Binance gateways (Spot/USDT-M Futures), backtesting with realistic fill models, alpha research platform.

## STRUCTURE
```
vnrs/
├── src/
│   ├── trader/        # Core trading infrastructure (engine, objects, gateway trait)
│   ├── gateway/       # Exchange gateways (Binance Spot/USDT-M)
│   ├── backtesting/   # Strategy simulation with fill models & risk engine
│   ├── strategy/      # Strategy framework (template trait, engine)
│   ├── alpha/         # Quantitative research (ML models, factor analysis)
│   ├── chart/         # egui charting (candlestick, indicators)
│   ├── python/        # PyO3 bindings for Python strategies
│   ├── rpc/           # ZeroMQ RPC for distributed systems
│   ├── event/         # Event engine (standalone)
│   ├── lib.rs         # Library entry (re-exports all public types)
│   └── main.rs        # GUI application binary (requires "gui" feature)
└── examples/          # Mixed Rust/Python demos
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| Add new exchange | `src/gateway/` + implement `BaseGateway` trait | Follow `binance/` pattern |
| Add new strategy | `src/strategy/template.rs` or Python via PyO3 | Implement `StrategyTemplate` trait |
| Modify order types | `src/trader/constant.rs` | `OrderType`, `Status`, `Direction` enums |
| Add technical indicator | `src/trader/utility.rs` (ArrayManager) or `src/chart/indicator.rs` | Use `ta` crate or custom |
| Backtesting logic | `src/backtesting/engine.rs` | Look-ahead bias prevention via event ordering |
| Fill model simulation | `src/backtesting/fill_model.rs` | 5 models: BestPrice, Ideal, TwoTier, SizeAware, Probabilistic |
| Python integration | `src/python/bindings.rs` | PyO3 module definition |
| UI components | `src/trader/ui/` | egui immediate mode, Chinese localization |

## CONVENTIONS
- **vt_symbol**: Universal identifier `SYMBOL.EXCHANGE` (e.g., `BTCUSDT.BINANCE`)
- **Async**: Tokio runtime, `Arc<RwLock<T>>` for shared state
- **Error handling**: `Result<T, String>` (no centralized error enum)
- **Chinese UI**: Default localization with platform-specific font loading
- **Feature flags**: `gui`, `python`, `alpha`, `database` (default: gui+alpha+python)

## ANTI-PATTERNS (THIS PROJECT)
- **DON'T call `init_logger()` after tracing subscriber initialized** (`main.rs:306`)
- **Avoid `.expect()` in gateway code** - network failures expected; use proper error propagation
- **Backtesting fill model assumes "always fill at best price"** by default - not realistic for production strategies

## UNIQUE STYLES
- **vnpy-inspired architecture**: MainEngine, OmsEngine, BaseGateway, StrategyTemplate
- **Dual crate-type**: `["cdylib", "rlib"]` for Python bindings + Rust library
- **Lazy statics**: `once_cell::Lazy` for type mappings (STATUS_BINANCE2VT, etc.)
- **Event ordering**: Backtesting processes pending orders BEFORE calling strategy to prevent look-ahead bias

## COMMANDS
```bash
# Build with all features
cargo build --release --features "gui,python,alpha"

# Run GUI application
cargo run --release --bin trade_engine_app

# Run tests
cargo test

# Run examples
cargo run --example alpha_demo --features alpha
cargo run --example chart_demo --features gui

# Build Python wheel (requires maturin)
maturin develop --release --features python
```

## NOTES
- Package name mismatch: directory `vnrs` vs Cargo.toml `trade_engine`
- No `rustfmt.toml` or `clippy.toml` - uses Rust defaults
- CI runs only `cargo build` + `cargo test` (no clippy/fmt check)
- Config persists to `.rstrader/` directory (gateway configs, settings)
- Binance WebSocket API uses modern `userDataStream.subscribe.signature` method (Spot)
