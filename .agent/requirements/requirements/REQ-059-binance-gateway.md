---
id: REQ-059
title: "Binance Gateway（Spot + USDT-M Futures）"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P0
level: epic
cluster: Gateway
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: []
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现 Binance Spot 和 USDT-M Futures Gateway 已完整实现，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "Binance Gateway 实现 Spot 和 USDT-M Futures 的完整交易和数据订阅"
---

# Binance Gateway（Spot + USDT-M Futures）

## 描述

Binance 交易所网关，实现 Spot（现货）和 USDT-M Futures（U本位永续合约）的完整交易与数据订阅能力。两个网关均实现 `BaseGateway` trait，通过 REST API 下单/查询，通过 WebSocket 接收实时行情和订单推送。共享 REST 客户端、WebSocket 客户端、配置持久化和常量映射模块。

## 验收标准

### 架构与模块结构

- [x] BinanceSpotGateway 实现 BaseGateway trait，默认名称 BINANCE_SPOT，交易所 Exchange::Binance
- [x] BinanceUsdtGateway 实现 BaseGateway trait，默认名称 BINANCE_USDT，交易所 Exchange::BinanceUsdm
- [x] 共享 BinanceRestClient（HMAC-SHA256 签名、代理支持、速率限制重试）
- [x] 共享 BinanceWebSocketClient（消息处理、订阅管理、断线重连）
- [x] 共享 BinanceGatewayConfig 配置持久化到 .rstrader/binance/gateway_configs.json
- [x] 共享常量映射模块（状态、订单类型、方向、K线周期的双向映射）

### 连接与生命周期

- [x] Spot connect()：保存配置 → 初始化 REST 客户端 → 时间同步 → 查询账户/持仓/委托/成交/合约 → 启动用户数据流 → 启动行情 WebSocket
- [x] Futures connect()：保存配置 → 初始化 REST 客户端 → 时间同步 → 查询账户/持仓/委托/成交/合约 → 启动用户数据流 → 启动行情 WebSocket
- [x] close()：断开 market_ws 和 trade_ws，清空 event_sender 防止滞后的异步任务发送事件
- [x] 支持 REAL 和 TESTNET 两种服务器模式，各端点独立配置

### REST API 客户端

- [x] HMAC-SHA256 请求签名，参数按字母序排列后签名
- [x] 支持 GET / POST / PUT / DELETE 四种 HTTP 方法
- [x] 三种安全级别：None（公开）、Signed（签名+时间戳）、ApiKey（仅 API Key 头）
- [x] 本地与服务器时间偏移量计算与自动补偿（recvWindow=5000）
- [x] 429 速率限制自动重试，最多 3 次，遵循 retry-after 头
- [x] HTTP 代理和 SOCKS5 代理支持（初始化时根据配置重建 reqwest Client）

### WebSocket 客户端

- [x] HTTP CONNECT 隧道代理和 SOCKS5 代理连接
- [x] 直接连接（无代理）模式
- [x] 30 秒心跳 ping，60 秒无 pong 判定连接死亡
- [x] 健康监控线程：10 秒周期检查连接活性
- [x] 订阅追踪：记录所有已订阅频道，重连后自动重新订阅
- [x] subscribe() 带断线重试（5 秒内 50 次重试，确保不遗漏订阅）
- [x] 指数退避重连：1s 起步，每次翻倍，上限 60s，带 ±25% 抖动
- [x] graceful_shutdown 标志区分主动断开和异常断线，异常断线触发 on_disconnect 回调
- [x] ConnectionManager 封装自动重连逻辑，支持最大重试次数和重连成功回调

### Spot 市场数据

- [x] subscribe()：订阅 ticker（{symbol}@ticker）和 5 档深度（{symbol}@depth5@100ms）
- [x] ticker 数据解析：last_price, open, high, low, volume, turnover
- [x] depth5 数据解析：5 档买价/买量和 5 档卖价/卖量写入 TickData
- [x] TickData 推送：last_price/bid_price_1/ask_price_1 任一 > 0 时推送
- [x] DepthData 推送：从 TickData 的 5 档盘口生成 DepthData 事件
- [x] 动态订阅：合约不在预加载列表中时发出警告但允许继续

### Futures 市场数据

- [x] subscribe()：订阅 ticker 和 depth5@100ms 数据流
- [x] ticker 和 depth5 解析逻辑与 Spot 一致
- [x] TickData 和 DepthData 推送逻辑与 Spot 一致
- [x] 合约校验：合约不在已知列表中时返回错误

### Spot 用户数据流

- [x] 使用 WebSocket API 的 userDataStream.subscribe.signature 方法订阅（非传统 listenKey）
- [x] outboundAccountPosition 事件：解析账户余额推送（free + locked = total）
- [x] executionReport 事件：解析订单状态变更和成交回报
- [x] eventStreamTerminated 事件：记录警告
- [x] listenKeyExpired 事件：记录警告（传统方式遗留）
- [x] subscriptionId 追踪和订阅响应处理

### Futures 用户数据流

- [x] 使用传统 listenKey 方式：POST /fapi/v1/listenKey 创建
- [x] ACCOUNT_UPDATE 事件：解析余额推送（walletBalance）和持仓更新
- [x] ORDER_TRADE_UPDATE 事件：解析订单状态变更和成交回报
- [x] listenKeyExpired 事件：记录警告
- [x] keep_user_stream()：每 1800 次调用自动 PUT 续期 listenKey
- [x] recreate_listen_key()：断开 → 重新创建 listenKey → 重连交易流

### 订单管理

- [x] Spot send_order()：支持 Limit/Market/Stop/StopLimit/Fak(IOC)/Fok(FOK) 六种订单类型
- [x] Spot Post-Only：使用 EXPIRED_TAKER 响应类型实现 maker-only 意图（GTX 不支持现货）
- [x] Futures send_order()：支持 Limit/Market/Stop(STOP_MARKET)/StopLimit(STOP)/Fak(IOC)/Fok(FOK)
- [x] Futures Post-Only：LIMIT + timeInForce=GTX
- [x] Futures reduceOnly 参数：reduce_only=true 写入请求
- [x] Spot 不支持 reduceOnly：发出警告并忽略
- [x] Spot cancel_order()：DELETE /api/v3/order
- [x] Futures cancel_order()：DELETE /fapi/v1/order
- [x] 订单 ID 生成：connect_time（原子计数器）+ order_count（原子递增）

### 账户与持仓查询

- [x] Spot query_account()：GET /api/v3/account，解析各资产 free/locked
- [x] Spot query_position()：复用账户接口，将资产余额映射为 Long 方向持仓
- [x] Futures query_account()：GET /fapi/v2/account，解析 walletBalance 和持仓信息
- [x] Futures query_position()：GET /fapi/v2/positionRisk，解析 positionAmt 正负判断多空方向

### 历史数据

- [x] Spot query_history()：GET /api/v3/klines，分页拉取 K 线数据（limit=1000）
- [x] Futures query_history()：GET /fapi/v1/klines，分页拉取 K 线数据（limit=1500）
- [x] 支持 9 种 K 线周期：1s/1m/5m/15m/30m/1h/4h/1d/1w
- [x] Spot query_trade_impl()：按持仓委托的 symbol 分页查询 3 年历史成交（/api/v3/myTrades）
- [x] Futures query_trade_impl()：按持仓的 symbol 分页查询 3 年历史成交（/fapi/v1/userTrades）

### 合约信息

- [x] Spot query_contract()：GET /api/v3/exchangeInfo，解析 PRICE_FILTER/LOT_SIZE 过滤器
- [x] Futures query_contract()：GET /fapi/v1/exchangeInfo，解析价格步长和最小下单量
- [x] Spot 合约 product=Spot, stop_supported=false
- [x] Futures 合约 product=Futures, stop_supported=true

### 断线重连

- [x] Spot market_ws 断线：指数退避重连 + 重新订阅
- [x] Spot trade_ws 断线：断开 → 指数退避重连 → 重新发送 userDataStream.subscribe.signature → 重查账户和委托
- [x] Futures market_ws 断线：指数退避重连 + 重新订阅
- [x] Futures trade_ws 断线：断开 → 创建新 listenKey → 重连 → 重查持仓和委托

### 僵尸委托检测

- [x] 30 秒周期后台任务：检测 Submitting 状态超过 60 秒的委托
- [x] 通过 REST API 查询订单最新状态并修正
- [x] 交易所找不到的委托标记为 Cancelled
- [x] Spot 使用 /api/v3/order，Futures 使用 /fapi/v1/order

### 类型映射

- [x] 状态映射：NEW→NotTraded, PARTIALLY_FILLED→PartTraded, FILLED→AllTraded, CANCELED/EXPIRED→Cancelled, REJECTED→Rejected
- [x] 方向映射：BUY→Long, SELL→Short（双向）
- [x] Spot 订单类型映射：LIMIT/MARKET/STOP_LOSS/STOP/TAKE_PROFIT（双向）
- [x] Futures 订单类型映射（含 timeInForce）：LIMIT+GTC, LIMIT+GTX, MARKET, STOP+GTC, STOP_MARKET, TAKE_PROFIT, LIMIT+IOC, LIMIT+FOK（双向）
- [x] K 线周期映射：9 种 VT Interval → Binance interval 字符串

### 配置持久化

- [x] BinanceGatewayConfig：key/secret/server/proxy_host/proxy_port 五项配置
- [x] 自动保存到 .rstrader/binance/gateway_configs.json
- [x] 启动时自动加载已保存配置
- [x] GatewaySettings ↔ BinanceGatewayConfig 双向转换
