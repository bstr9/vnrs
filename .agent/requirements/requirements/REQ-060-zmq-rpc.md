---
id: REQ-060
title: "ZeroMQ RPC 分布式通信"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P2
level: story
cluster: Infrastructure
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: [REQ-003, REQ-062]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现 ZMQ RPC 已实现服务端/客户端、事件广播、远程控制，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "ZeroMQ RPC 实现分布式事件广播和远程策略控制"
---

# ZeroMQ RPC 分布式通信

## 描述

基于 ZeroMQ 实现的远程过程调用（RPC）框架，用于分布式交易系统的进程间通信。支持远程事件广播、策略控制、订单管理等功能，兼容 vnpy Python RPC 协议。

## 验收标准

### 核心架构

- [x] ZMQ REQ/REP 请求-响应模式
- [x] ZMQ PUB/SUB 发布-订阅模式
- [x] 可配置的服务端地址（REP socket, PUB socket）
- [x] 可配置的客户端地址（REQ socket, SUB socket）
- [x] TCP keepalive 支持

### 通用类型（common.rs）

- [x] RpcRequest 请求结构（method, args, kwargs）
- [x] RpcResponse 响应结构（success, data）
- [x] RpcMessage 消息结构（topic, data）
- [x] RemoteException 远程异常类型
- [x] TimeoutError 超时错误类型
- [x] ConnectionError 连接错误类型（Disconnected, ConnectionFailed, SocketError）
- [x] 心跳常量定义（HEARTBEAT_TOPIC, HEARTBEAT_INTERVAL, HEARTBEAT_TOLERANCE）
- [x] RPC 超时常量（RPC_TIMEOUT, POLL_TIMEOUT_MS）

### 服务端（server.rs）

- [x] RpcServer 结构体
- [x] ServerConfig 配置结构
- [x] 服务启动（start）与停止（stop）
- [x] RPC 函数注册（register）
- [x] 消息发布（publish）
- [x] 心跳任务自动发送
- [x] 请求处理任务后台运行
- [x] 活跃状态检查（is_active）
- [x] 心跳检查（check_heartbeat）

### 客户端（client.rs）

- [x] RpcClient 结构体
- [x] ClientConfig 配置结构
- [x] 客户端启动（start）与停止（stop）
- [x] 远程方法调用（call）
- [x] 带超时的远程调用（call_with_timeout）
- [x] 主题订阅（subscribe_topic）
- [x] 主题取消订阅（unsubscribe_topic）
- [x] 消息回调设置（set_callback）
- [x] 连接状态检测（is_connected）
- [x] 最后心跳时间获取（last_received_ping）
- [x] RemoteMethod 代理类
- [x] MethodCache LRU 缓存
- [x] 心跳超时告警

### 交易服务（trading_service.rs）

**单条查询：**
- [x] get_tick - 获取最新 tick 行情
- [x] get_bar - 获取最新 bar 行情
- [x] get_order - 获取订单
- [x] get_trade - 获取成交
- [x] get_position - 获取持仓
- [x] get_account - 获取账户
- [x] get_contract - 获取合约
- [x] get_quote - 获取报价

**批量查询：**
- [x] get_all_ticks - 获取所有 tick
- [x] get_all_bars - 获取所有 bar
- [x] get_all_orders - 获取所有订单
- [x] get_all_trades - 获取所有成交
- [x] get_all_positions - 获取所有持仓
- [x] get_all_accounts - 获取所有账户
- [x] get_all_contracts - 获取所有合约
- [x] get_all_quotes - 获取所有报价
- [x] get_all_active_orders - 获取所有活跃订单
- [x] get_all_active_quotes - 获取所有活跃报价
- [x] get_all_logs - 获取所有日志

**网关管理：**
- [x] get_all_gateway_names - 获取所有网关名称
- [x] get_all_exchanges - 获取所有交易所

**交易操作：**
- [x] send_order - 发送订单
- [x] cancel_order - 撤销订单
- [x] subscribe - 订阅行情
- [x] connect - 连接网关
- [x] disconnect - 断开网关
- [x] query_history - 查询历史数据
- [x] write_log - 写入日志

### 错误处理

- [x] JSON 序列化/反序列化错误传播
- [x] 请求参数缺失错误提示
- [x] 函数未找到错误
- [x] 异步操作同步等待（oneshot channel）
