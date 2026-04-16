# BINANCE GATEWAY

Binance Spot and USDT-M Futures integration.

## OVERVIEW
Implements `BaseGateway` trait. Uses REST API for orders/queries, WebSocket for real-time data.

## STRUCTURE
```
binance/
├── mod.rs            # Module exports
├── spot_gateway.rs   # BinanceSpotGateway
├── usdt_gateway.rs   # BinanceUsdtGateway
├── rest_client.rs    # REST API client with signing, proxy, retry
├── websocket_client.rs # WebSocket client (HTTP/SOCKS5 proxy support)
├── config.rs         # Configuration persistence (.rstrader/binance/)
└── constants.rs      # API hosts, type mappings (Lazy<HashMap>)
```

## WHERE TO LOOK
| Task | Location |
|------|----------|
| Modify order types | `constants.rs` - ORDERTYPE_VT2BINANCE |
| Add new endpoint | `rest_client.rs` |
| WebSocket message handling | `websocket_client.rs` - WsMessageHandler |
| Config persistence | `config.rs` - BinanceConfigs |

## KEY PATTERNS
- **Order ID**: `connect_time + order_count` (atomic counter)
- **Signature**: HMAC-SHA256 for signed requests
- **Time sync**: Tracks offset between local and server time
- **Spot user stream**: WebSocket API `userDataStream.subscribe.signature`
- **Futures user stream**: Traditional listen key approach

## TYPE MAPPINGS
```
NEW → Status::NotTraded
PARTIALLY_FILLED → Status::PartTraded
FILLED → Status::AllTraded
CANCELED → Status::Cancelled
```

## CONVENTIONS
- Config saved to `.rstrader/binance/gateway_configs.json`
- Proxy support: HTTP CONNECT tunnel + SOCKS5
- Price formatting removes trailing zeros for API compliance
