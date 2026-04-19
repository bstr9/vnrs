# vnrs 开源项目参考研究 & AI-Native 演进计划

> 生成时间: 2026-04-19
> 更新时间: 2026-04-19 (v3 — 合并 v1 原始内容 + v2 AI 演进 + 精确校正)
> 阶段: 开源参考研究 → AI-Native 架构演进

---

## 一、开源项目全景扫描

通过 GitHub 搜索，共发现 **15+** 个与 vnrs 高度相关的开源项目。按语言和技术栈分类如下：

### 1.1 Rust 原生交易框架（最直接参考）

| 项目 | Stars | 语言 | 核心定位 | 关键特性 |
|------|-------|------|----------|----------|
| **nautilus_trader** | 22,088 | Rust + Python | 生产级多资产多交易所交易引擎 | 确定性事件驱动、contingency orders、128-bit 高精度、Redis 持久化、15+ 交易所适配器 |
| **hftbacktest** | 3,955 | Rust + Python | HFT 回测与做市策略 | 队列位置模拟、订单/行情延迟建模、Level-2/Level-3 订单簿重建、Binance/Bybit 实盘 |
| **RustQuant** | 1,709 | Rust | 量化金融数学库 | AAD 自动微分、期权定价、随机过程、组合优化、ISO 标准码、基础 LOB |
| **OrderBook-rs** | 441 | Rust | 高性能无锁订单簿 | 无锁架构、Iceberg/FOK/IOC/GTD/TrailingStop/Pegged 订单、STP 自成交防范、费用模型、NATS 集成、日志追加恢复 |
| **tesser** | 151 | Rust | 模块化量化交易框架 | Cargo workspace 架构、ONNX AI 推理、tokio::broadcast EventBus、Decimal 原生指标、SQLite/LMDB 状态持久化、执行算法 (PeggedBest/Sniper/TrailingStop) |
| **extrema_infra** | 164 | Rust | 量化交易基础设施 | HList 静态分发策略注册、ZeroMQ + ONNX ML 集成、AltTensor 张量载体、广播式数据分发、无锁并发 |
| **ibx** | 111 | Rust + Python | Interactive Brokers 直连引擎 | 无 Java Gateway 直连 IB、SeqLock 无锁行情读取、340ns tick 延迟、ibapi 兼容 API |
| **binance-rs** | 846 | Rust | Binance API 客户端 | Spot + Futures REST/WebSocket |
| **binance-connector-rust** | 305 | Rust | Binance 官方连接器 | 官方维护、Spot REST/WebSocket |

### 1.2 Python/Go 交易框架（架构模式参考）

| 项目 | Stars | 语言 | 核心定位 | 关键特性 |
|------|-------|------|----------|----------|
| **freqtrade** | 48,942 | Python | 加密货币交易机器人 | FreqAI 机器学习策略优化、SQLite 持久化、Telegram 控制、Hyperopt 参数优化、WebUI |
| **vnpy** | 39,550 | Python | 全功能量化交易平台 | CTA/价差/期权/组合策略、CTP 等 20+ 网关、vnpy.alpha ML 模块、数据管理、风控模块 |
| **hummingbot** | 18,218 | Python + Cython | 做市与套利机器人 | CLOB CEX/DEX + AMM DEX 支持、40+ 交易所连接器、Gateway DEX 中间件 |
| **jesse** | 7,682 | Python | 加密策略研究框架 | 精致的研究→回测→实盘流程、完整的策略生命周期 |
| **bbgo** | 1,630 | Go | 加密货币交易机器人框架 | 8+ 交易所、30+ 内置指标、Grid/做市/套利策略、MySQL/SQLite/Redis 持久化、Web Dashboard、K8s 部署 |
| **pybroker** | 3,263 | Python | ML 驱动的算法交易 | 机器学习策略开发、Walkforward 分析 |

---

## 二、逐项特性对比分析

### 2.1 vnrs 当前已完成特性

| 特性 | 状态 | 实现 |
|------|------|------|
| 事件驱动架构 | ✅ | EventEngine + tokio channels |
| OMS 订单管理 | ✅ | OmsEngine |
| Binance Spot/USDT-M 网关 | ✅ | REST + WebSocket |
| 策略框架 (StrategyTemplate) | ✅ | CTA 风格策略 |
| 回测引擎 | ✅ | 5 种 Fill Model |
| PyO3 Python 绑定 | ✅ | CtaTemplate Python 接口 |
| 风控引擎 | ✅ | 余额检查、下单限制 |
| 告警/通知系统 | ✅ | LogAlertChannel + WebhookAlertChannel |
| 算法执行 (TWAP/VWAP) | ✅ | AlgoEngine |
| 数据记录器 | ✅ | DataRecorder (BaseEngine) |
| 策略 warmup | ✅ | load_bars + 缓存 |
| 实时 P&L 监控 | ✅ | 自动 tick 更新 |
| 多周期 K 线合成 | ✅ | BarSynthesizer |
| WebSocket 自动重连 | ✅ | 指数退避 + 事件清理 |
| 仓位对账 | ✅ | 启动/重连时对账 |
| 订单超时处理 | ✅ | 孤儿订单检测 |
| 多策略仓位隔离 | ✅ | 策略级仓位跟踪 |
| 速率限制 | ✅ | 请求频率控制 |
| 事件日志 | ✅ | 事件持久化 |
| 崩溃恢复 | ✅ | 状态恢复 |
| 优雅关闭 | ✅ | MCP mode 支持 |
| 交易去重 | ✅ | LRU Trade Dedup |
| Alpha 研究平台 | ✅ | 因子分析 + ML |
| egui GUI | ✅ | 实时图表 |
| ZeroMQ RPC | ✅ | 分布式通信 |
| OCO/OTO/Bracket 条件单 | ✅ | `bracket_order.rs` (941行) — ContingencyType: Oco/Oto/Bracket |
| TrailingStop 订单 | ✅ | `stop_order.rs` (672行) — TrailingStopPct/TrailingStopAbs/StopMarket/StopLimit/TakeProfit |
| Iceberg/MIT/LIT 订单 | ✅ | `order_emulator.rs` (1285行) — 交易所不原生支持的订单类型本地模拟 |
| Level-2 订单簿 | ✅ | `order_book.rs` (671行) — 含 VWAP、micro-price、imbalance、book_pressure |
| SQLite 持久化 | ⚠️ | `sqlite_database.rs` (849行) — 已实现但**未启动时自动加载**，save_order/trade/position 为 stub |
| MessageBus (pub/sub) | ✅ | `message_bus.rs` (450行) — subscribe/publish 模式 |
| Portfolio Manager | ✅ | `portfolio.rs` (639行) — PnL 跟踪、exposure、win rate、profit factor |
| Trading Session Manager | ✅ | `session.rs` (364行) — Binance 24/7、中国期货日夜盘、上交所深交所 |
| Reconciliation Engine | ✅ | `reconciliation.rs` (820行) — 仓位/订单漂移检测，重连自动对账 |

### 2.2 关键特性差距（与顶级项目对比 — 修正版）

> **⚠ 修正说明**: v1 中大量特性被标记为 ❌，经代码审查发现已实现。以下表格为修正后的准确状态。

#### A. 订单类型与执行能力

| 特性 | vnrs | nautilus_trader | hftbacktest | OrderBook-rs | tesser | 优先级 |
|------|------|-----------------|-------------|--------------|--------|--------|
| IOC (Immediate-Or-Cancel) | ⚠️ 枚举存在 (Fak) | ✅ | ✅ | ✅ | — | **P0** |
| FOK (Fill-Or-Kill) | ⚠️ 枚举存在 (Fok) | ✅ | ✅ | ✅ | — | **P0** |
| GTD (Good-Till-Date) | ❌ | ✅ | — | ✅ | — | P1 |
| Post-Only 标志 | ❌ | ✅ | — | ✅ | — | **P0** |
| Reduce-Only 标志 | ❌ | ✅ | — | — | — | **P0** |
| OCO (One-Cancels-Other) | ✅ `bracket_order.rs` | ✅ | — | — | — | — |
| OTO (One-Triggers-Other) | ✅ `bracket_order.rs` | ✅ | — | — | — | — |
| Iceberg 订单 | ✅ `order_emulator.rs` | ✅ | — | ✅ | — | — |
| Trailing Stop 订单 | ✅ `stop_order.rs` + `order_emulator.rs` | ✅ | — | ✅ | ✅ | — |
| Pegged 订单 | ❌ | — | — | ✅ | ✅ | P2 |
| 自成交防范 (STP) | ❌ | — | — | ✅ | — | P2 |

**IOC/FOK 详情**: `constant.rs` 有 `OrderType::Fak` 和 `OrderType::Fok` 枚举。Binance **Futures** 网关已正确映射 (`Fak → LIMIT+IOC`, `Fok → LIMIT+FOK`)。但 **Spot** 网关映射为 `LIMIT` 却**未设置 timeInForce**，导致 FAK/FOK Spot 订单会以 GTC 限价单执行（潜在 bug）。

**真正缺失**: Post-Only / Reduce-Only 标志（`OrderRequest` 中无此字段）、GTD 订单类型、Spot 网关 IOC/FOK timeInForce 修复。

#### B. 订单簿与行情深度

| 特性 | vnrs | hftbacktest | OrderBook-rs | nautilus_trader | extrema_infra | 优先级 |
|------|------|-------------|--------------|-----------------|---------------|--------|
| Level-2 订单簿 (Market-By-Price) | ✅ `order_book.rs` | ✅ | ✅ | ✅ | ✅ (on_lob) | — |
| Level-3 订单簿 (Market-By-Order) | ❌ | ✅ | — | — | — | P2 |
| 订单簿不平衡指标 | ✅ `volume_imbalance()` | ✅ (alpha) | ✅ | — | — | — |
| VWAP 计算 | ✅ `vwap(is_buy, qty)` | — | ✅ | — | — | — |
| 微价格 (Micro Price) | ✅ `micro_price()` | — | ✅ | — | — | — |
| 市场冲击模拟 | ❌ | — | ✅ | — | — | P2 |
| 书本压力 | ✅ `book_pressure(levels)` | — | — | — | — | — |

**说明**: v1 中标记为 ❌ 的 L2 订单簿及相关指标实际均已实现。`order_book.rs` 使用 BTreeMap 维护 bid/ask，支持增量更新 (`apply_update`)，提供 `OrderBookSnapshot` 无锁读取。

#### C. 延迟建模与回测精度

| 特性 | vnrs | hftbacktest | nautilus_trader | tesser | 优先级 |
|------|------|-------------|-----------------|--------|--------|
| 行情延迟建模 | ❌ | ✅ | — | — | P1 |
| 订单延迟建模 | ❌ | ✅ | — | ✅ (--latency-ms) | P1 |
| 队列位置模拟 | ❌ | ✅ | — | — | P2 |
| Tick 级回测 | ❌ | ✅ | ✅ | ✅ (tick mode) | P1 |

**影响**: 当前回测过于理想化。延迟建模是回测结果可信度的关键。

#### D. 状态持久化与恢复

| 特性 | vnrs | nautilus_trader | tesser | bbgo | freqtrade | 优先级 |
|------|------|-----------------|--------|------|-----------|--------|
| SQLite 持久化 | ⚠️ 已实现，未自加载 | ✅ (Cache) | ✅ | ✅ | ✅ | **P0** |
| LMDB 持久化 | ❌ | — | ✅ | — | — | P2 |
| Redis 持久化 | ❌ | ✅ | — | ✅ | — | P2 |
| 状态快照/恢复 | ⚠️ `restore_from_database()` 存在但未自动调用 | ✅ | ✅ | ✅ | ✅ | **P0** |
| 确定性重放 | ❌ | ✅ | — | — | — | P2 |

**说明**: `SqliteDatabase` (849行) 实现了 `BaseDatabase` trait，支持 bars/ticks/orders/trades/positions/events 六张表。但存在两个问题：(1) 启动时不会自动创建并加载（需 `MainEngine::new_with_database(db)` 显式传入）；(2) `save_order_data`/`save_trade_data`/`save_position_data` 为 stub，返回 `Ok(true)` 但未实际持久化。

#### E. ML/AI 集成

| 特性 | vnrs | extrema_infra | tesser | vnpy | freqtrade | 优先级 |
|------|------|---------------|--------|------|-----------|--------|
| ONNX Runtime 推理 | ❌ | ✅ | ✅ (tesser-cortex) | — | — | P1 |
| ZeroMQ ML 管道 | ✅ (rpc) | ✅ | — | — | — | — |
| ML 特征管道 | ❌ (alpha模块部分) | ✅ (AltTensor) | ✅ | ✅ (vnpy.alpha) | ✅ (FreqAI) | P1 |
| 模型训练框架 | ❌ | — | — | ✅ (vnpy.alpha) | ✅ (FreqAI) | P2 |

**影响**: 已有 Alpha 模块和 ZMQ RPC，但缺少 ONNX 直接推理能力。

#### F. 量化金融数学

| 特性 | vnrs | RustQuant | 优先级 |
|------|------|-----------|--------|
| 期权定价 | ❌ | ✅ (Black-Scholes, 二叉树等) | P2 |
| 随机过程 | ❌ | ✅ (Brownian Motion, CIR, OU, Vasicek) | P2 |
| AAD 自动微分 | ❌ | ✅ | P2 |
| ISO 标准码 | ❌ | ✅ (ISO-4217, ISO-3166, ISO-10383) | P2 |

**影响**: 这些对期权交易和高级量化研究有价值，但对当前现货/期货优先级较低。

#### G. 架构与工程

| 特性 | vnrs | nautilus_trader | tesser | extrema_infra | 优先级 |
|------|------|-----------------|--------|---------------|--------|
| MessageBus (pub/sub) | ✅ `message_bus.rs` | — | ✅ | ✅ | — |
| tokio::broadcast EventBus | ❌ (mpsc + MessageBus) | — | ✅ | ✅ | P2 |
| HList 静态分发 | ❌ (HashMap<dyn>) | — | — | ✅ | P2 |
| 纳秒时间戳 | ❌ (微秒) | ✅ | — | ✅ (µs) | P2 |
| 128-bit 高精度 | ❌ (rust_decimal 64-bit) | ✅ (可选) | — | — | P2 |
| Prometheus 指标 | ❌ | — | ✅ | — | P1 |
| 结构化 JSON 日志 | ❌ | — | ✅ | — | P1 |
| 多进程策略部署 | ❌ | — | ✅ (Docker Compose) | — | P2 |
| Web Dashboard | ❌ (egui only) | — | — | ✅ (bbgo: React) | P2 |
| Telegram/Slack 控制 | ❌ | — | — | ✅ (bbgo/freqtrade) | P2 |

**说明**: `message_bus.rs` (450行) 实现了完整的 topic-based pub/sub 模式 (`subscribe`/`publish`/`get_messages_for`)，v1 中标记为 ❌ 实际已实现。

---

## 三、项目特色发现（值得借鉴的独到实现）

### 3.1 hftbacktest — 队列位置模拟

hftbacktest 的核心竞争力在于**订单队列位置模拟**。它不仅模拟价格匹配，还跟踪订单在价格队列中的位置，基于队列前方的委托量来估算成交概率。这对 HFT 做市策略的回测精度至关重要。

**关键数据结构**:
- 每个 price level 维护队列中的订单量
- 新到订单加入队列尾部
- 成交从队列头部消耗
- 支持自定义队列消耗模型

### 3.2 OrderBook-rs — 无锁架构 + STP + 费用模型

OrderBook-rs 的亮点:
- **无锁并发**: 基于 `dashmap::DashMap` + `crossbeam::SegQueue` 混合，200K+ ops/s
- **自成交防范 (STP)**: CancelTaker/CancelMaker/CancelBoth 三种模式
- **费用模型**: Maker/Taker 不同费率，TradeResult 包含费用字段
- **日志追加**: FileJournal + CRC32 校验 + 段轮换，支持确定性重放
- **NATS JetStream**: 交易事件发布，支持批量和节流

### 3.3 extrema_infra — HList 静态分发 + ONNX

extrema_infra 的亮点:
- **HList 策略注册**: 编译时保证类型安全，零运行时开销（vs `Vec<Box<dyn Strategy>>`）
- **ONNX 本地推理**: AltTensor 统一张量载体，支持多输出模型
- **分离关注点**: 延迟敏感任务（下单/取消）与支持任务（特征/风控/仓位）独立运行
- **广播式分发**: 同一行情源广播到多个策略

### 3.4 tesser — ONNX + 执行算法 + 状态持久化

tesser 的亮点:
- **tesser-cortex**: ONNX Runtime 包装器，零拷贝数据流
- **执行算法**: PeggedBest (追踪最优价)、Sniper (等待目标价)、TrailingStop
- **SQLite/LMDB 持久化**: 仓位/订单/价格状态，启动时自动加载
- **对账**: 定期 REST API 对比本地状态，差异告警
- **Prometheus 指标**: tick/candle 吞吐、equity、order error

### 3.5 ibx — SeqLock 无锁行情

ibx 的亮点:
- **SeqLock**: 无锁行情读取，任何线程可读，零 GIL 争用
- **单核绑定**: 整个引擎跑在一个 pinned core 上，零分配
- **极低延迟**: tick 340ns, 下单 459ns (vs Java Gateway 2ms/83µs)

### 3.6 RustQuant — 量化金融数学库

RustQuant 的亮点:
- **autodiff**: AAD 自动微分，高效计算梯度
- **instruments**: 债券、期权定价
- **stochastics**: Brownian Motion, CIR, OU, Vasicek, Hull-White
- **portfolio**: 组合类型，HashMap of Positions
- **data**: Yahoo! Finance 数据下载

---

## 四、freqtrade FreqAI 架构深入分析

### 4.1 架构概览

```
┌─────────────────────────────────────────────────────┐
│                   IStrategy                          │
│  ┌──────────────────────────────────────────────┐   │
│  │ feature_engineering_expand_all()             │   │
│  │ feature_engineering_expand_basic()           │   │
│  │ feature_engineering_standard()               │   │
│  │ set_freqai_targets()   ← 定义标签 (& 前缀)   │   │
│  └──────────────┬───────────────────────────────┘   │
│                 │ DataFrame with %features & &labels │
│                 ▼                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │         IFreqaiModel.start()                 │   │
│  │  ┌──────────────┐  ┌─────────────────────┐  │   │
│  │  │ DataKitchen  │  │   DataDrawer        │  │   │
│  │  │ - features   │  │   - model persist   │  │   │
│  │  │ - labels     │  │   - historic preds  │  │   │
│  │  │ - pipeline   │  │   - model dict      │  │   │
│  │  └──────┬───────┘  └─────────────────────┘  │   │
│  │         │                                     │   │
│  │  ┌──────▼───────────────────────────────┐    │   │
│  │  │  Data Pipeline (datasieve)           │    │   │
│  │  │  VarianceThreshold → MinMaxScaler →  │    │   │
│  │  │  [PCA] → [SVM outliers] → [DI] →    │    │   │
│  │  │  [DBSCAN] → [Noise removal]          │    │   │
│  │  └──────┬───────────────────────────────┘    │   │
│  │         │                                     │   │
│  │  ┌──────▼───────────────────────────────┐    │   │
│  │  │    Model (三大家族)                  │    │   │
│  │  │  1. Sklearn (LightGBM/XGBoost/RF)   │    │   │
│  │  │  2. PyTorch (MLP/Transformer)       │    │   │
│  │  │  3. RL (PPO/A2C/DQN via SB3)        │    │   │
│  │  └──────┬───────────────────────────────┘    │   │
│  │         │ predictions (&s prefixed cols)      │   │
│  └─────────┼─────────────────────────────────────┘   │
│            ▼                                         │
│  Strategy 使用: dataframe['&-prediction']            │
└─────────────────────────────────────────────────────┘
```

### 4.2 ML 框架支持

| 家族 | 库 | 模型 |
|------|-----|------|
| **梯度提升** | LightGBM, XGBoost | Classifier/Regressor + MultiTarget + RF 变体 |
| **sklearn** | scikit-learn | RandomForestClassifier |
| **CatBoost** | catboost | sklearn 兼容接口 |
| **PyTorch** | torch | MLP, **Transformer** (Vaswani encoder 用于时序) |
| **RL** | stable-baselines3, gymnasium | PPO, A2C, DQN, TRPO, ARS, RecurrentPPO |

### 4.3 关键发现：freqtrade **没有** LLM 集成

搜索了整个 freqtrade 仓库：`LLM`, `GPT`, `Claude`, `language model`, `huggingface`, `openai`, `bert` — **零结果**。

唯一的 "Transformer" 是 `PyTorchTransformerRegressor`，这是 Vaswani "Attention Is All You Need" 的 **时序预测编码器**，不是 NLP/LLM 模型。

### 4.4 训练生命周期

FreqAI 使用**滑动窗口重训练**模式：

```
|←── train_period_days ──→|← backtest_period_days →|
        (训练)                    (预测)
┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
                              |←── train_period ──→|← backtest →|
                                   (滑窗前进)
```

- 每次窗口滑到新位置，重新训练模型
- 训练在**独立线程**中执行，不阻塞实时交易
- `expired_hours` 控制模型过期，自动触发重训练
- 支持 `live_retrain_hours` 定时重训练

### 4.5 datasieve 数据管道

```python
# datasieve pipeline 完整流程
VarianceThreshold   # 移除低方差特征
    → MinMaxScaler  # 归一化到 [0,1]
    → [PCA]         # 可选降维
    → [SVM]         # 可选异常值检测 (OneClassSVM)
    → [DI]          # Dissimilarity Index 漂移检测
    → [DBSCAN]      # 可选聚类去噪
    → [Noise]       # 可选噪声移除
```

管道中的每一步都是**可选的**，通过配置文件控制开关。

### 4.6 FreqAI 优点

- **清晰分离**：策略定义 features/labels，模型只负责 train/predict
- **滑动窗口重训练**：防止模型过时 (`train_period_days` → `backtest_period_days` → 滑动)
- **多交易对、多时间框架**：自动特征生成 (`include_timeframes`, `include_corr_pairlist`)
- **异常值/漂移检测**：DI (Dissimilarity Index), SVM, DBSCAN 内置于 pipeline
- **独立训练线程**：不阻塞实时交易
- **特征命名约定**：`%-{indicator}_{period}_{pair}_{timeframe}` 自动发现

### 4.7 FreqAI 缺失 (vnrs 机会)

- **无 LLM 集成** — 情绪分析、新闻、链上叙事分析
- **无 AI Agent / 自主决策层**
- **无在线学习** (仅批量重训练)
- **无模型集成/投票机制**
- **Python only** (无 Rust 性能)
- **无多资产组合优化** (每个交易对独立训练)
- **无 Feature Store / 特征版本管理**

---

## 五、AI/LLM 友好的交易项目发现 (15个)

### Tier 1: LLM-Native 交易框架

| 项目 | Stars | 语言 | 核心创新 | AI 模式 |
|------|-------|------|----------|---------|
| **TradingAgents** | 51,484 | Python | 多智能体辩论框架 — 分析师→研究员→交易员→风控 | LangGraph 编排，多 LLM 提供者，可配置辩论轮数 |
| **AI-Trader** | 13,519 | Python | Agent-Native 平台 — AI agent 通过 SKILL.md 文件接入 | MCP 风格 skill 文件，agent-as-first-class-citizen |
| **OpenAlice** | 3,634 | TypeScript | Trading-as-Git — stage/commit/push，guard pipeline | 四层架构，append-only event log，MCP server |

### Tier 2: LLM 增强研究平台

| 项目 | Stars | 语言 | 核心创新 | AI 模式 |
|------|-------|------|----------|---------|
| **FinGPT** | 19,598 | Python | 五层 FinLLM 框架，LoRA 轻量微调 ($300 vs $3M) | RAG 情绪分析，指令微调基准 |
| **LangAlpha** | 843 | Python | PTC (Programmatic Tool Calling) — agent 写代码而非 dump 数据 | LangGraph ReAct，隔离沙箱，实时干预 |
| **ATLAS** | 1,349 | Python | 自研究循环 — prompts 是权重，Sharpe 是损失函数 | 达尔文式权重进化，regime-specific 训练 |

### Tier 3: RL-Native 交易框架

| 项目 | Stars | 语言 | 核心创新 | AI 模式 |
|------|-------|------|----------|---------|
| **TensorTrade** | 6,169 | Python | 可组合 RL 环境 — Observer/Agent/Action/Portfolio/Reward | OpenAI Gym，Ray RLlib 分布式训练 |
| **TorchTrade** | 350 | Python | TorchRL 构建，多时间框架观察，LLM-as-Actor | LLM actor 与 RL policy 可互换 |
| **TradeMaster** | 2,568 | Python | 全流程 RL 平台，13+ 算法，PRUDEX-Compass 评估 | FinAgent 多模态，MacroHFT 分层 RL |
| **JAX-LOB** | 140 | Python/JAX | GPU 加速 LOB 模拟器，大规模 RL 训练 | JAX vmap/jit 批量环境并行 |

### Tier 4: AI Agent 基础设施

| 项目 | Stars | 语言 | 核心创新 | AI 模式 |
|------|-------|------|----------|---------|
| **AutoHedge** | 1,341 | Python | 多 agent pipeline：Director→Quant→Risk→Execution | Swarms 框架，结构化输出 |
| **Lumibot** | 1,348 | Python | DuckDB 时间序列分析，MCP server 挂载 | SQL 查询替代 raw bars，backtest 缓存 LLM 调用 |
| **QuantDinger** | 1,218 | Python | 自托管 AI quant OS — 自然语言→Python 策略代码 | LLM 策略生成，置信度校准，反思 worker |
| **TradeMemory** | 619 | Python | AI agent 持久记忆层，Outcome-Weighted Memory | 5 层记忆，SHA-256 审计，MiFID II 合规 |
| **OKX Agent Skills** | 78 | TypeScript | 即插即用 MCP skills — 任何 LLM agent 可交易 | MCP server，声明式 skill 文件 |

---

## 六、vnpy-Style 架构对 AI 的限制

### 6.1 同步回调陷阱

```rust
// 当前：StrategyTemplate 是同步回调
pub trait StrategyTemplate: Send + Sync {
    fn on_tick(&mut self, tick: &TickData, context: &StrategyContext);
    fn on_bar(&mut self, bar: &BarData, context: &StrategyContext);
}
// 问题：on_bar/on_tick 是同步 &mut self 回调
// ML 推理 (ONNX, gRPC, LLM API) 本质上是 async，可能耗时 10-500ms
// 策略会阻塞整个事件循环
```

**证据** (`strategy/engine.rs:178-191`)：
```rust
fn process_tick_event(&self, tick: &TickData) {
    let mut strategies = self.strategies.blocking_write();
    if let Some(strategy) = strategies.get_mut(strategy_name) {
        strategy.on_tick(tick, context);  // 阻塞在这里
    }
}
```

### 6.2 无特征管道

`ArrayManager` 内联计算 40+ 指标：
- 无特征版本管理
- 无特征共享 (两个策略使用相同特征会独立计算)
- 无训练/服务一致性保证

### 6.3 无模型生命周期管理

`alpha` 模块提供 `AlphaModel` trait，但：
- 无模型版本/注册表
- 无 train/validate/deploy 流程
- 无 shadow deployment / A/B testing
- 无 drift detection
- 无 checkpoint/rollback

### 6.4 状态管理仅限仓位

`BaseStrategy` 追踪 `positions`, `targets`, `active_orderids`，但无：
- 模型状态 (哪个版本激活，上次更新时间)
- 特征状态 (上次决策使用的特征向量)
- 推理延迟指标
- 决策审计追踪

### 6.5 事件总线不透明

`GatewayEvent` 携带类型化交易数据，但无：
- 事件元数据 (单调时间戳、schema 版本)
- 合成事件注入 (如 "情绪信号到达")
- 事件重放用于调试

---

## 七、AI-Native 架构演进方案

### 7.1 分层架构

```
┌─────────────────────────────────────────────────────────┐
│                    AI Agent Layer                        │
│  LLM Sentiment │ RL Trader │ Risk Assessor │ Researcher  │
├─────────────────────────────────────────────────────────┤
│                  Model Serving Layer                     │
│  ONNX Runtime │ Candle (local) │ gRPC (Triton) │ ZMQ    │
├─────────────────────────────────────────────────────────┤
│                  Feature Store Layer                     │
│  Online Store (in-memory) │ Offline Store (Parquet)     │
│  Feature Registry │ Time-Travel Snapshots │ Lineage     │
├─────────────────────────────────────────────────────────┤
│                  Strategy Layer (演进)                   │
│  AsyncStrategy trait │ Weight-centric interface          │
│  Signal Bus │ Decision Audit Trail                       │
├─────────────────────────────────────────────────────────┤
│                  Core Engine (现有)                      │
│  MainEngine │ OmsEngine │ BaseGateway │ EventEngine      │
└─────────────────────────────────────────────────────────┘
```

### 7.2 新增组件规格

#### 组件 1: `AsyncStrategy` Trait

```rust
#[async_trait]
pub trait AsyncStrategy: Send + Sync {
    fn strategy_name(&self) -> &str;
    fn vt_symbols(&self) -> &[String];

    async fn on_init(&mut self, context: &StrategyContext) -> Result<(), StrategyError>;
    async fn on_bar(&mut self, bar: &BarData, context: &StrategyContext)
        -> Vec<OrderRequest>;
    async fn on_tick(&mut self, tick: &TickData, context: &StrategyContext)
        -> Vec<OrderRequest>;

    /// Weight-centric interface (FinRL-X pattern)
    fn target_weights(&self) -> HashMap<String, f64>;
    fn drain_decisions(&mut self) -> Vec<DecisionRecord>;
}

pub struct DecisionRecord {
    pub timestamp: DateTime<Utc>,
    pub strategy: String,
    pub signal: SignalType,
    pub confidence: f64,
    pub features_used: Vec<String>,
    pub model_version: String,
    pub inference_latency_us: u64,
    pub orders_generated: Vec<String>,
}
```

#### 组件 2: Feature Store (`src/feature/`)

```rust
pub struct FeatureStore {
    online: Arc<DashMap<String, FeatureVector>>,    // <1us 读
    offline: Arc<ParquetFeatureStore>,              // 回测用
    registry: Arc<FeatureRegistry>,                 // 定义、版本、血缘
}

pub struct FeatureDefinition {
    pub id: FeatureId,
    pub expression: String,         // Polars 表达式或 Rust fn
    pub version: u32,
    pub dependencies: Vec<FeatureId>, // 血缘追踪
    pub dtype: FeatureType,
}

impl FeatureStore {
    pub fn get_online(&self, entity: &str) -> Option<FeatureVector>;
    pub async fn get_at(&self, entity: &str, ts: i64) -> Result<FeatureVector>; // 时间旅行
    pub async fn materialize(&self, data: &BarData) -> Result<()>;
    pub async fn snapshot(&self, tag: &str) -> Result<SnapshotId>;
}
```

#### 组件 3: Model Registry & Serving (`src/model/`)

```rust
pub struct ModelEntry {
    pub model_id: String,
    pub version: SemVer,
    pub stage: ModelStage,  // Development -> Staging -> Shadow -> Canary -> Production
    pub artifact_path: PathBuf,
    pub metrics: ModelMetrics,
    pub feature_ids: Vec<FeatureId>,
}

#[async_trait]
pub trait ModelServer: Send + Sync {
    async fn predict(&self, features: &FeatureVector) -> Result<Prediction>;
    fn model_info(&self) -> &ModelEntry;
    async fn health(&self) -> Result<HealthStatus>;
}
// 实现：OnnxModelServer, CandleModelServer, GrpcModelServer, ZmqModelServer
```

**Rust ML 栈决策矩阵**:

| 方式 | 延迟 | 依赖 | 用途 |
|------|------|------|------|
| `ort` (ONNX Runtime) | 0.5-15ms | C++ shared lib | 生产模型 (XGBoost, sklearn, PyTorch 导出) |
| `candle` | 10-20ms CPU | 纯 Rust | 本地 LLM/embedding，GGUF 量化 |
| `tract` | 1-5ms | 纯 Rust | 小模型，嵌入式部署 |
| gRPC to Triton | 5-50ms | 外部服务器 | GPU 推理，大型 transformer |
| ZMQ to Python | 2-20ms | Python 进程 | 研究迭代，利用 Python ML 生态 |

**推荐**: 默认 `ort` 用于结构化模型，`candle` 用于文本/情绪，ZMQ 桥接 Python 研究。

#### 组件 4: AI Agent Layer (`src/agent/`)

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn agent_name(&self) -> &str;
    fn agent_type(&self) -> AgentType;

    async fn observe(&mut self, event: &AgentEvent) -> Result<()>;
    async fn decide(&mut self) -> Result<Option<AgentAction>>;
    async fn feedback(&mut self, outcome: &DecisionOutcome) -> Result<()>;
}

pub enum AgentType {
    SentimentAnalyst,
    TechnicalAnalyst,
    RiskAssessor,
    RLTrader,
    DebateParticipant,
}

// MCP bridge for LLM tool-use
pub struct McpBridge {
    tools: Vec<ToolDefinition>,
    mcp_server: mcp::Server,
}
```

**LLM 集成模式 (按实用性排序)**:

| 模式 | 实现方式 | 延迟 | 适用性 |
|------|----------|------|--------|
| **Sentiment -> Feature** | LLM 异步处理新闻，写入 Feature Store | 秒-分钟 | 最佳起点 |
| **LLM as Code Generator** | LLM 生成策略代码 -> WASM/Python | 分钟 | 研究用 |
| **LLM as Risk Assessor** | 定期组合审查，自然语言风险报告 | 分钟 | 低频 |
| **LLM as Decision Maker** | LLM 通过 MCP 调用工具，实时交易 | 秒 | 高延迟、高成本 |
| **LLM as Debate Agent** | 多 LLM 辩论 (TradingAgents 模式) | 30-60s | 研究/教育 |

#### 组件 5: Signal Bus (`src/signal/`)

```rust
pub struct Signal {
    pub signal_id: String,
    pub source: String,              // "sentiment_agent" or "rl_policy_v3"
    pub symbol: String,
    pub direction: SignalDirection,
    pub strength: f64,
    pub confidence: f64,
    pub features: Vec<(String, f64)>,
    pub model_version: String,
    pub timestamp: i64,
}

pub struct SignalBus {
    subscribers: Arc<DashMap<String, Vec<mpsc::Sender<Signal>>>>,
}
// 策略可以订阅 AI 信号并与传统指标结合
```

#### 组件 6: RL Environment (`src/rl/`)

```rust
pub struct TradingEnv {
    engine: Arc<MainEngine>,
    feature_store: Arc<FeatureStore>,
    reward_fn: Box<dyn RewardFunction>,
    action_mapper: Box<dyn ActionMapper>,
}

pub trait ActionMapper: Send + Sync {
    fn map_action(&self, action: Action) -> Vec<OrderRequest>;
}

pub trait RewardFunction: Send + Sync {
    fn compute(&self, prev: &Observation, curr: &Observation, action: &Action) -> f64;
}
// 标准 reward: SharpeReward, PnlReward, RiskAdjustedReward
// 通过 PyO3 导出，兼容 stable-baselines3, ray[rllib]
```

### 7.3 目录结构演进

```
vnrs/
├── src/
│   ├── trader/          # 现有: 核心交易基础设施 (不变)
│   ├── gateway/         # 现有: 交易所网关 (不变)
│   ├── backtesting/     # 现有: 策略模拟 (不变)
│   ├── strategy/        # 演进: 添加 AsyncStrategy, SignalBus 集成
│   ├── alpha/           # 演进: 使用 FeatureStore, ModelRegistry
│   ├── feature/         # 新增: Feature Store (online/offline/registry/snapshot)
│   ├── model/           # 新增: Model Registry & Serving (onnx/candle/grpc/zmq)
│   ├── agent/           # 新增: AI Agent Layer (sentiment/rl/risk/mcp)
│   ├── signal/          # 新增: Signal Bus (类型化信号, pub/sub)
│   ├── rl/              # 新增: RL Environment (gym 接口, action/reward)
│   ├── chart/           # 现有: egui 图表 (不变)
│   ├── python/          # 演进: 添加 TradingEnv, FeatureStore 绑定
│   ├── rpc/             # 现有: ZeroMQ RPC (不变)
│   ├── event/           # 现有: 事件引擎 (不变)
│   ├── lib.rs
│   └── main.rs
```

### 7.4 Cargo.toml 新增依赖

```toml
[dependencies]
# 现有...

# 新增: ML 推理 (全部可选 feature)
ort = { version = "2.0", features = ["load-dynamic"], optional = true }
candle-core = { version = "0.9", optional = true }
candle-nn = { version = "0.9", optional = true }
candle-transformers = { version = "0.9", optional = true }
tract-onnx = { version = "0.22", optional = true }

# 新增: Feature Store
dashmap = "6"
# polars, arrow, parquet 已在 alpha 模块使用

# 新增: Model Serving (可选)
tonic = { version = "0.12", optional = true }
prost = { version = "0.13", optional = true }

[features]
default = ["gui", "alpha", "python"]
ml-inference = ["ort"]
ml-local = ["candle-core", "candle-nn", "candle-transformers"]
ml-tract = ["tract-onnx"]
ml-grpc = ["tonic", "prost"]
ml-full = ["ml-inference", "ml-local", "ml-grpc"]
agent = []
rl = []
ai-native = ["ml-full", "agent", "rl"]
```

---

## 八、实施路线图 (修正版)

### Phase 0: 关键缺陷修复 (P0) — 预计 2-3 天

| # | 特性 | 实现要点 | 无新依赖 |
|---|------|----------|----------|
| P0-1 | **Post-Only / Reduce-Only 标志** | 在 `OrderRequest` (object.rs) 添加 `post_only: bool` 和 `reduce_only: bool`；网关层映射到 Binance 参数 | ✅ |
| P0-2 | **IOC/FOK Spot 网关修复** | 修复 `spot_gateway.rs` 中 FAK/FOK 的 timeInForce 映射 | ✅ |
| P0-3 | **SQLite 启动时自动加载** | `MainEngine::new()` 默认创建 SqliteDatabase 并调用 `restore_from_database()` | ✅ |

### Phase 1: AI 基础设施 (P1) — 预计 5-7 天

| # | 特性 | 实现要点 |
|---|------|----------|
| P1-1 | **FeatureStore (online/offline)** | 新建 `src/feature/`；DashMap online store + Parquet offline store |
| P1-2 | **AsyncStrategy trait** | 修改 `strategy/engine.rs` 支持 async 回调；DecisionRecord 追踪 |
| P1-3 | **StrategyEngine 重构** | 分离同步/异步策略路径；async 策略使用 tokio::spawn |
| P1-4 | **ONNX Runtime 集成** | 新建 `src/model/onnx.rs`；使用 `ort` crate (需讨论新依赖) |

### Phase 2: 模型生命周期 (P1) — 预计 5-7 天

| # | 特性 | 实现要点 |
|---|------|----------|
| P2-1 | **ModelRegistry** | SQLite 后端存储模型元数据；ModelStage 状态机 |
| P2-2 | **SignalBus** | 新建 `src/signal/`；类型化 Signal + pub/sub |
| P2-3 | **DecisionAudit** | 所有 AI 决策写入 SQLite；包含 features/confidence/model_version |
| P2-4 | **Shadow Deployment** | 新模型 shadow 模式：记录预测但不交易 |

### Phase 3: AI Agent 层 (P2) — 预计 5-7 天

| # | 特性 | 实现要点 |
|---|------|----------|
| P3-1 | **Agent trait** | 新建 `src/agent/`；observe/decide/feedback 接口 |
| P3-2 | **McpBridge** | 集成 MCP protocol；LLM 工具调用 |
| P3-3 | **SentimentAgent** | 示例 agent：异步拉取新闻，调用 LLM，写入 Feature Store |
| P3-4 | **RiskAssessorAgent** | 定期组合风险分析，生成风险报告 |
| P3-5 | **candle 集成** | 本地 LLM embedding (可选) |
| P3-6 | **ZMQ Python 桥接** | 现有 RPC 扩展：Python ML 服务调用 |

### Phase 4: RL 环境 (P2) — 预计 3-5 天

| # | 特性 | 实现要点 |
|---|------|----------|
| P4-1 | **TradingEnv** | 新建 `src/rl/`；gym 兼容接口 (step/reset/observation) |
| P4-2 | **ActionMapper** | 离散/连续 action → OrderRequest 映射 |
| P4-3 | **RewardFunction** | SharpeReward, PnlReward, RiskAdjustedReward |
| P4-4 | **PyO3 导出** | Python 可直接使用 TradingEnv |

---

## 九、架构模式借鉴总结

| 模式 | 来源项目 | vnrs 适用性 | 优先级 |
|------|----------|-------------|--------|
| **Trading-as-Git** | OpenAlice | 策略版本管理、审计追踪 | P0 |
| **Multi-agent Debate** | TradingAgents | 多视角决策、风险评估 | P0 |
| **LLM-as-Actor** | TorchTrade | LLM 与 RL policy 互换 | P1 |
| **PTC (Programmatic Tool Calling)** | LangAlpha | Agent 写代码而非 dump 数据 | P1 |
| **DuckDB 时间序列** | Lumibot | SQL 查询替代 raw bars | P1 |
| **Agent Memory** | TradeMemory | 持久记忆、Outcome-Weighted | P1 |
| **Autoresearch Loop** | ATLAS | prompts 权重进化 | P2 |
| **GPU RL Acceleration** | JAX-LOB | 大规模 RL 训练 | P2 |
| **MCP Server** | OKX/OpenAlice | LLM 工具调用标准化 | P2 |
| **SKILL.md Onboarding** | AI-Trader | Agent 文档驱动接入 | P3 |
| **PRISM Regime Detection** | ATLAS | 市场状态识别 | P3 |

---

## 十、核心洞察

### 10.1 Architecture > Model

在 TradingAgents 研究中发现：**改变 agent 架构对收益的影响 (20-40%) 远大于改变 LLM 骨干 (<5%)**。

- 多 agent 辩论比单 agent 效果提升显著
- LLM backbone (GPT-4 vs Claude vs Llama) 差异较小
- 架构设计（信息流、决策链、反馈机制）是关键

**启示**: vnrs 应优先投入 AI 架构演进，而非追求最新 LLM。

### 10.2 90/10 Gap

分析 LLM 交易相关论文发现：

| 研究方向 | 论文比例 | vnrs 定位 |
|----------|----------|-----------|
| Alpha/信号生成 | 90.9% | ❌ 不是重点 |
| 部署基础设施 | 9.1% | ✅ **这是 vnrs 的机会** |

**启示**: vnrs 应成为 AI 交易策略的**生产级部署基础设施**，而非又一个 alpha 研究框架。

### 10.3 Weight-centric Interface (FinRL-X)

FinRL-X 提出：策略-执行合约应为**组合权重向量**，而非离散动作。

```rust
// 传统: 离散动作
enum Action { Buy, Sell, Hold }

// Weight-centric: 组合权重
fn target_weights(&self) -> HashMap<String, f64> {
    // {"BTCUSDT": 0.3, "ETHUSDT": 0.2, "USDT": 0.5}
}
```

**优势**:
- 统一 RL/LLM/传统策略接口
- 天然支持组合优化
- 执行引擎处理权重→订单映射

### 10.4 Rust ML 生态就绪 (2026)

| 组件 | 成熟度 | 生产可用 |
|------|--------|----------|
| `ort` (ONNX Runtime) | ✅ 稳定 | ✅ 是 |
| `candle` (HuggingFace) | ✅ 活跃开发 | ⚠️ embeddings 可用，LLM CPU 较慢 |
| `tract` (纯 Rust) | ✅ 稳定 | ✅ 小模型 |
| `burn` (深度学习框架) | ⚠️ 发展中 | ❌ |

**结论**: Rust ML 生态已可支撑交易场景的模型推理需求。

---

## 十一、约束与注意事项

### 11.1 已有约束（必须遵守）

- **不引入新 crate 依赖**（用户明确要求）→ Phase 1 ONNX (`ort`) 需要特别讨论
- **不删除现有功能**
- **每次迭代编译通过**: `cargo check --features "gui,alpha,python"`
- **测试通过**: `cargo test --lib --features "gui,alpha,python" -- --test-threads=4`
- **#![deny(clippy::unwrap_used)]** — 不得使用 `.unwrap()`
- 现货类型优先（"除了期货，你也得考虑现货啊！"）

### 11.2 Phase 0 无新依赖

Phase 0 的三项任务（Post-Only/Reduce-Only 标志、IOC/FOK Spot 修复、SQLite 自动加载）**均无需引入新依赖**，可直接实施。

### 11.3 新依赖风险评估

| 特性 | 需要的新依赖 | 是否可避免 | 替代方案 |
|------|-------------|-----------|----------|
| ONNX Runtime | `ort` crate | 不可避免 | 通过 ZMQ 调用外部 Python 进程（已有 RPC） |
| Candle LLM | `candle-*` crates | 可避免 | 仅用于本地 embedding，可选 |
| Prometheus | `prometheus` crate | 可避免 | 手写简单 HTTP `/metrics` 端点 |
| Redis | `redis` crate | 可避免 | 先实现 SQLite 即可 |
| HList | `frunk` crate | 可避免 | 宏实现或继续用 HashMap |

### 11.4 实施建议

1. **Phase 0** 是最高优先级，修复实盘关键缺陷，无新依赖风险
2. **Phase 1-2** 可并行推进：FeatureStore/AsyncStrategy 不依赖 ONNX
3. **ONNX 集成**如果用户坚持不引入新依赖，可通过现有 ZMQ RPC 调用外部 Python 推理服务
4. 建议优先完成 Phase 0-2，这是从"研究工具"到"生产系统"的关键跨越

---

## 十二、项目间架构模式对比

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          架构模式对比                                        │
├──────────┬──────────────────┬──────────────────┬───────────────────────────┤
│ 维度      │ vnrs (当前)      │ nautilus_trader   │ tesser / extrema_infra   │
├──────────┼──────────────────┼──────────────────┼───────────────────────────┤
│ 事件分发  │ tokio::mpsc      │ 确定性事件循环     │ tokio::broadcast          │
│ 策略注册  │ HashMap<String,  │ Strategy trait    │ HList (编译时) / trait    │
│          │ Box<dyn Strategy>│                   │                           │
│ 网关抽象  │ BaseGateway trait│ Adapter pattern   │ ExecutionClient trait     │
│ 状态管理  │ 纯内存           │ Cache (内存/Redis) │ SQLite/LMDB              │
│ 精度      │ rust_decimal     │ 64/128-bit 可选   │ rust_decimal / f64        │
│ 回测模型  │ Bar级 + Fill模型  │ Bar/Tick/Book    │ Candle + Tick + Matching  │
│ ML集成    │ Alpha模块 + ZMQ  │ —                 │ ONNX (tesser-cortex)     │
│ 持久化    │ ⚠️ SQLite存在未自用│ ✅                │ ✅                        │
│ 监控      │ tracing log      │ —                 │ Prometheus + JSON log     │
│ 部署      │ 单进程           │ 单节点            │ 多进程 (Docker Compose)   │
│ OCO/OTO  │ ✅ bracket_order  │ ✅ contingency    │ —                         │
│ L2 Order │ ✅ order_book    │ ✅                │ ✅                        │
│ AI-Native│ ❌ 计划中         │ ❌                │ ⚠️ ONNX                   │
└──────────┴──────────────────┴──────────────────┴───────────────────────────┘
```

---

## 十三、总结

### 核心发现

1. **vnrs 的 vn.py 血统最为纯正** — 架构设计（MainEngine/OmsEngine/BaseGateway/StrategyTemplate）直接传承自 vn.py，但在 Rust 实现的完整度和性能上有独特优势。

2. **nautilus_trader 是最成熟的 Rust 交易引擎** — 22K stars，确定性架构，多交易所，contingency orders，但 LGPL 许可证和复杂度可能不适合轻量级场景。

3. **hftbacktest 在回测精度上独树一帜** — 队列位置模拟和延迟建模是我们最需要学习的，但它的 Python+Numba 技术栈与 Rust 有差异。

4. **tesser 和 extrema_infra 是最接近的 Rust 同类** — 模块化架构、ONNX 集成、状态持久化都是我们可以直接参考的。

5. **OrderBook-rs 的无锁订单簿实现值得参考** — 如果未来要做交易所级别的撮合引擎，这是最佳参考。

6. **freqtrade 没有任何 LLM 集成** — 这意味着 vnrs 在 AI-Native 方向没有直接竞争者。

### 最关键的 3 个差距 (修正版)

1. **Post-Only / Reduce-Only 标志** — `OrderRequest` 缺少这两个字段，影响期货交易质量
2. **SQLite 启动时自动加载** — 存在但未自动调用，无法实现崩溃恢复
3. **延迟建模 (回测)** — 当前回测过于理想化，需添加 feed_latency / order_latency

### 已修正的 v1 错误标记

v1 中以下特性被错误标记为 ❌，实际已实现：
- OCO/OTO/Bracket 条件单 → ✅ `bracket_order.rs` (941行)
- TrailingStop 订单 → ✅ `stop_order.rs` (672行)
- Iceberg/MIT/LIT 订单 → ✅ `order_emulator.rs` (1285行)
- Level-2 订单簿 → ✅ `order_book.rs` (671行)，含 VWAP/micro-price/imbalance
- MessageBus (pub/sub) → ✅ `message_bus.rs` (450行)

### 最值得参考的代码片段（建议保存）

| 来源 | 内容 | 用途 |
|------|------|------|
| tesser | SQLite 状态持久化 | Phase 0 P0-3 实现 |
| nautilus_trader | OCO/OTO 订单组管理 | 已实现，可参考优化 |
| hftbacktest | 队列位置模拟核心 | P2 回测增强 |
| OrderBook-rs | 无锁订单簿 + STP | 已实现，可参考 STP |
| extrema_infra | ONNX 推理 + AltTensor | Phase 1 P1-4 实现 |
| TradingAgents | 多 agent 辩论框架 | Phase 3 AI Agent 层 |

### vnpy-Style vs AI-Native 定位

| 维度 | vnpy-Style (当前) | AI-Native (演进目标) |
|------|-------------------|----------------------|
| 策略接口 | 同步回调 | AsyncStrategy + Weight-centric |
| 特征管理 | ArrayManager 内联 | FeatureStore + 版本管理 |
| 模型管理 | 无 | ModelRegistry + Shadow/Canary |
| 决策追踪 | 无 | DecisionRecord + 审计 |
| LLM 集成 | 无 | Agent Layer + MCP Bridge |
| RL 环境 | 无 | TradingEnv (gym 兼容) |
| 部署模式 | 单进程 | 可选多进程 + Python 桥接 |

---

> 文档版本: v3 (2026-04-19)
> 作者: OpenCode Agent
> 审核: 基于 git b46a4be (v1) + 代码审查修正
