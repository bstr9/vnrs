# TRADER MODULE

Core trading platform infrastructure - foundational abstractions for algorithmic trading.

## OVERVIEW
Event-driven architecture with MainEngine (orchestration) + OmsEngine (state container). Inspired by vnpy.

## STRUCTURE
```
trader/
├── engine.rs        # MainEngine (event loop), OmsEngine (order management state)
├── object.rs        # Data structs: TickData, BarData, OrderData, TradeData, PositionData
├── constant.rs      # Enums: Direction, Exchange, Status, OrderType, Interval
├── gateway.rs       # BaseGateway trait + GatewayEventSender
├── converter.rs     # OffsetConverter (position offset handling)
├── utility.rs       # BarGenerator, ArrayManager (40+ technical indicators)
├── setting.rs       # Global SETTINGS singleton
├── database.rs      # BaseDatabase trait + MemoryDatabase
├── datafeed.rs      # BaseDatafeed trait
├── optimize.rs      # Parameter optimization (GA, brute-force)
├── logger.rs        # Tracing-based logging
├── app.rs           # BaseApp trait for extensions
└── ui/              # GUI components (feature-gated)
```

## WHERE TO LOOK
| Task | Location |
|------|----------|
| Add new data type | `object.rs` - follow existing struct patterns |
| Add new exchange enum | `constant.rs` - Exchange enum |
| Implement gateway | See `src/gateway/` - implement `BaseGateway` trait |
| Position offset logic | `converter.rs` - OffsetConverter |
| Technical indicators | `utility.rs` - ArrayManager |

## KEY ABSTRACTIONS
- **MainEngine**: Gateway management, event routing, order lifecycle
- **OmsEngine**: Thread-safe state (ticks, orders, trades, positions, accounts)
- **BaseGateway**: Async trait for exchange connections
- **StrategyTemplate**: Strategy interface (in `src/strategy/`)

## CONVENTIONS
- `Arc<RwLock<T>>` for shared mutable state
- `mpsc::UnboundedChannel` for event passing
- vt_symbol format: `SYMBOL.EXCHANGE`
