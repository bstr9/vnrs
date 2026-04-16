# vnrs 架构改进 TODO

**最后更新**: 2026-04-16  
**当前状态**: P0 + P1 + P2 全部实现完成，166 个测试通过，0 warnings，编译成功

---

## 已完成工作摘要

### P0 - 基础设施 (已完成)
- **P0-1**: 统一策略基类 `Strategy` — 合并 3 个重叠的 Python 策略类为 1 个
- **P0-2**: 实现 `PortfolioFacade` 对象并暴露给 Python 策略
- **P0-3**: 实现 `OrderFactory` 类型化订单创建
- **P0-4**: 添加 typed identifiers (InstrumentId, OrderId, PositionId)
- **P0 Integration**: 将 Portfolio + OrderFactory 注入到 Strategy

### P1 - 现货交易支持 (已完成)
- **P1-1**: 创建 `SpotStrategyTemplate` 现货策略基类
- **P1-2**: 暴露完整 Position 对象 (含成本、未实现盈亏)
- **P1-3**: 实现 `PyRiskManager` PyO3 封装 (风险检查暴露到 Python)

### P2 - 多品种/组合支持 (已完成)
- **P2-1**: 实现 `SynchronizedBarGenerator` 同步多品种 Bar
- **P2-2**: 添加组合级统计 `PyPortfolioStatistics`
- **P2-3**: 实现 `MessageBus` 策略间通信

### 代码质量改进 (已完成)
- **Q1**: 清理 217 个弃用警告 → 0 warnings（strategy_bindings.rs 添加 `#![allow(deprecated)]`）
- **Q2**: 添加 `#![deny(clippy::unwrap_used)]` 全局 lint，修复所有违规
- **Q3**: 单元测试从 44 增加到 166（+122 个新测试）
- **Q4**: 实现 RandomForest/GradientBoosting 真正算法替换桩实现
- **Q5**: GUI binary 编译修复（python extension-module 与 binary 分离）
- **Q6**: Chart interval_changed 事件处理实现

### 架构优化 (已完成)
- **A1**: MessageBus 自动注入到 Strategy（engine.add_strategy()）
- **A2**: SynchronizedBarGenerator PyO3 绑定（sync_bar_bindings.rs）
- **A3**: RiskManager 与 BacktestingEngine 集成（send_order 前调用 check_order）
- **A4**: Python 示例策略（spot, multi_symbol, risk_managed, message_bus）

---

## 待处理事项

### 1. Python 集成测试

**状态**: 需要 maturin develop 后在 Python 环境中运行

**步骤**:
```bash
maturin develop --release --features python
python -m pytest tests/
```

---

### 2. 多线程并发访问测试

**状态**: 待实现

**场景**: Arc<Mutex<...>> 模式在高并发下的正确性验证

---

## 新增文件清单

| 文件路径 | 说明 |
|---------|------|
| `src/trader/identifier.rs` | Typed identifiers (InstrumentId, ClientOrderId, etc.) |
| `src/python/portfolio.rs` | PortfolioFacade + PyPosition + PortfolioState |
| `src/python/order_factory.rs` | OrderFactory + PyOrder |
| `src/python/risk_manager.rs` | PyRiskManager + PyRiskConfig + PyRiskCheckResult |
| `src/python/portfolio_stats.rs` | PyPortfolioStatistics |
| `src/python/message_bus.rs` | MessageBus + PyMessage |
| `src/python/sync_bar_bindings.rs` | SynchronizedBarGenerator PyO3 bindings |
| `src/python/cta_strategy.py` | CtaStrategy 向后兼容 shim |
| `src/python/spot_strategy.py` | SpotStrategy 基类 |
| `src/chart/sync_bar_generator.rs` | SynchronizedBarGenerator |
| `examples/spot_strategy_example.py` | 现货策略示例 |
| `examples/multi_symbol_strategy.py` | 多品种同步示例 |
| `examples/risk_managed_strategy.py` | 风控策略示例 |
| `examples/message_bus_example.py` | 策略间通信示例 |

---

## 构建与测试命令

```bash
# 编译检查
cargo check --features python

# 运行测试
cargo test --no-default-features --features alpha --lib

# Clippy lint (含 unwrap_used 检查)
cargo clippy --features "alpha,python" -- -D clippy::unwrap_used

# 构建 Python wheel
maturin develop --release --features python

# 运行 GUI
cargo run --release --bin trade_engine_app --features "gui,alpha"
```
