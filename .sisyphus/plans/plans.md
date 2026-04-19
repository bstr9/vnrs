# vnrs 开源项目参考研究 & AI-Native 演进计划

> 生成时间: 2026-04-19
> 更新时间: 2026-04-19 (v2)
> 阶段: 开源参考研究 → AI-Native 架构演进

---

## 一、关键发现：许多"缺失"特性实际已实现

在审查代码库后发现，第一版 plans.md 中列出的大量 "P0 缺失特性" **实际已经在 vnrs 中实现**：

### 1.1 已实现特性对照表

| 特性 | 原状态 | 实际状态 | 实现文件 |
|------|--------|----------|----------|
| OCO/OTO/Bracket 条件单 | P0 缺失 | **已实现** | `src/trader/bracket_order.rs` (941行) |
| TrailingStop 订单 | P1 缺失 | **已实现** | `src/trader/stop_order.rs` (672行) |
| Level-2 订单簿 | P0 缺失 | **已实现** | `src/trader/order_book.rs` (671行) — 含 VWAP、micro-price、imbalance |
| SQLite 持久化 | P0 缺失 | **已实现** | `src/trader/sqlite_database.rs` (849行) — bars/ticks/orders/trades/positions/events |
| IOC/FOK 订单类型 | P0 缺失 | **枚举存在** | `constant.rs` 有 `OrderType::Fak` 和 `Fok`，但网关层未映射 |
| Iceberg/MIT/LIT 订单 | P1 缺失 | **已实现** | `src/trader/order_emulator.rs` (1285行) |
| MessageBus (pub/sub) | P1 缺失 | **已实现** | `src/trader/message_bus.rs` (450行) |
| Portfolio Manager | 未提及 | **已实现** | `src/trader/portfolio.rs` (639行) |
| Trading Session Manager | 未提及 | **已实现** | `src/trader/session.rs` (364行) |
| Reconciliation Engine | 未提及 | **已实现** | `src/trader/reconciliation.rs` — 仓位/订单漂移检测 |

### 1.2 真正缺失的特性

经过验证，以下特性确实尚未实现：

| 特性 | 优先级 | 说明 |
|------|--------|------|
| **Post-Only / Reduce-Only 标志** | P0 | `OrderRequest` 缺少这两个字段 |
| **GTD (Good-Till-Date) 订单类型** | P1 | `OrderType` 枚举中没有 |
| **网关层 IOC/FOK 映射** | P0 | 枚举存在但未映射到 Binance API 参数 |
| **SQLite 启动时自动加载** | P1 | SqliteDatabase 存在但未在 MainEngine 启动时自动恢复状态 |
| **延迟建模 (回测)** | P1 | 回测引擎无 feed_latency / order_latency 参数 |
| **Tick 级回测** | P1 | 当前仅 Bar 级别回测 |
| **ONNX Runtime 集成** | P2 | 无法本地推理 ML 模型 |
| **Prometheus 指标导出** | P2 | 无监控指标 |
| **结构化 JSON 日志** | P2 | 仅有 tracing 输出 |
| **自成交防范 (STP)** | P2 | 订单匹配时无用户 ID 检查 |

---

## 二、freqtrade FreqAI 架构分析

### 2.1 架构概览

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
│  │  │  [PCA] → [SVM outliers] → [DBSCAN]  │    │   │
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

### 2.2 ML 框架支持

| 家族 | 库 | 模型 |
|------|-----|------|
| **梯度提升** | LightGBM, XGBoost | Classifier/Regressor + MultiTarget + RF 变体 |
| **sklearn** | scikit-learn | RandomForestClassifier |
| **CatBoost** | catboost | sklearn 兼容接口 |
| **PyTorch** | torch | MLP, **Transformer** (Vaswani encoder 用于时序) |
| **RL** | stable-baselines3, gymnasium | PPO, A2C, DQN, TRPO, ARS, RecurrentPPO |

### 2.3 关键发现：freqtrade **没有** LLM 集成

搜索了整个 freqtrade 仓库：`LLM`, `GPT`, `Claude`, `language model`, `huggingface`, `openai`, `bert` — **零结果**。

唯一的 "Transformer" 是 `PyTorchTransformerRegressor`，这是 Vaswani "Attention Is All You Need" 的 **时序预测编码器**，不是 NLP/LLM 模型。

### 2.4 FreqAI 优点

- **清晰分离**：策略定义 features/labels，模型只负责 train/predict
- **滑动窗口重训练**：防止模型过时 (`train_period_days` → `backtest_period_days` → 滑动)
- **多交易对、多时间框架**：自动特征生成 (`include_timeframes`, `include_corr_pairlist`)
- **异常值/漂移检测**：DI (Dissimilarity Index), SVM, DBSCAN 内置于 pipeline
- **独立训练线程**：不阻塞实时交易
- **特征命名约定**：`%-{indicator}_{period}_{pair}_{timeframe}` 自动发现

### 2.5 FreqAI 缺失 (vnrs 机会)

- 无 LLM 集成 — 情绪分析、新闻、链上叙事分析
- 无 AI Agent / 自主决策层
- 无在线学习 (仅批量重训练)
- 无模型集成/投票机制
- Python only (无 Rust 性能)
- 无多资产组合优化 (每个交易对独立训练)
- 无 Feature Store / 特征版本管理

---

## 三、AI/LLM 友好的交易项目发现 (15个)

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

## 四、vnpy-Style 架构对 AI 的限制

### 4.1 同步回调陷阱

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

### 4.2 无特征管道

`ArrayManager` 内联计算 40+ 指标：
- 无特征版本管理
- 无特征共享 (两个策略使用相同特征会独立计算)
- 无训练/服务一致性保证

### 4.3 无模型生命周期管理

`alpha` 模块提供 `AlphaModel` trait，但：
- 无模型版本/注册表
- 无 train/validate/deploy 流程
- 无 shadow deployment / A/B testing
- 无 drift detection
- 无 checkpoint/rollback

### 4.4 状态管理仅限仓位

`BaseStrategy` 追踪 `positions`, `targets`, `active_orderids`，但无：
- 模型状态 (哪个版本激活，上次更新时间)
- 特征状态 (上次决策使用的特征向量)
- 推理延迟指标
- 决策审计追踪

### 4.5 事件总线不透明

`GatewayEvent` 携带类型化交易数据，但无：
- 事件元数据 (单调时间戳、schema 版本)
- 合成事件注入 (如 "情绪信号到达")
- 事件重放用于调试

---

## 五、AI-Native 架构演进方案

### 5.1 分层架构

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

### 5.2 新增组件规格

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

### 5.3 目录结构演进

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

### 5.4 Cargo.toml 新增依赖

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
