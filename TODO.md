# vnrs 架构改进 TODO

**最后更新**: 2026-04-16  
**当前状态**: P0 + P1 + P2 全部实现完成，179 个测试通过，编译成功

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

---

## 待处理事项

### 1. 清理弃用警告 (217 个警告，全部来自 strategy_bindings.rs)

**问题描述**:  
`src/python/strategy_bindings.rs` 中的 `PyStrategy` 和 `PyStrategyEngine` 已标记为 `#[deprecated]`，但代码库中仍有大量引用导致警告。

**解决方案**:
```rust
// 方案 A: 在 strategy_bindings.rs 顶部添加全局 allow
#![allow(deprecated)]

// 方案 B: 找到所有使用 PyStrategy/PyStrategyEngine 的地方，替换为新的 Strategy 类
// 需要修改的文件可能包括:
// - src/python/bindings.rs
// - examples/ 目录下的示例文件
```

**相关文件**:
- `src/python/strategy_bindings.rs` — 已废弃的类定义
- `src/python/bindings.rs` — 调用 `register_strategy_module()`
- `src/python/mod.rs` — 带 `#[allow(deprecated)]` 的 re-export

---

### 2. Python 文件中的 pass 语句

**位置**: `src/python/cta_strategy.py` 第 156, 160, 168 行

**现状**: 这些是可选回调方法的默认实现 (no-op)，属于正常设计模式，不是问题。

---

### 3. 新增功能的 Python 文档

**待补充**:
- `docs/api.md` — Python API 文档
- `examples/` — 使用新 API 的示例策略:
  - `spot_strategy_example.py` — 现货策略示例
  - `multi_symbol_strategy.py` — 多品种同步示例
  - `risk_managed_strategy.py` — 风控策略示例
  - `message_bus_example.py` — 策略间通信示例

---

### 4. 架构优化建议 (非紧急)

#### 4.1 MessageBus 注入到 Strategy
当前 `MessageBus` 已实现但未自动注入到 `Strategy`。需要修改:

**文件**: `src/python/engine.rs`  
**位置**: `add_strategy()` 方法  
**修改内容**:
```rust
// 创建并注入 MessageBus
let message_bus = MessageBus::new();
let bus_py = Py::new(py, message_bus)?;
strategy.borrow_mut().message_bus = Some(bus_py);
```

**文件**: `src/python/strategy.rs`  
**位置**: `Strategy` struct  
**修改内容**:
```rust
#[pyo3(get)]
pub message_bus: Option<Py<MessageBus>>,
```

#### 4.2 SynchronizedBarGenerator PyO3 绑定
当前 Rust 实现已完成 (`src/chart/sync_bar_generator.rs`)，但未暴露到 Python。

**需要创建**: `src/python/sync_bar_bindings.rs`
**参考**: `src/python/portfolio.rs` 的封装模式

#### 4.3 RiskManager 与 BacktestingEngine 集成
`PyRiskManager` 已实现，但 `PyBacktestingEngine` 在发送订单前未调用风险检查。

**文件**: `src/python/backtesting_bindings.rs`  
**修改**: 在 `send_order` 前调用 `risk_manager.check_order()`

---

### 5. 测试覆盖

**新增测试统计**:
- `identifier.rs`: 15 tests
- `portfolio.rs`: 15 + 5 = 20 tests
- `order_factory.rs`: 17 tests
- `risk_manager.rs`: 35 tests
- `sync_bar_generator.rs`: 8 tests
- `portfolio_stats.rs`: 4 tests
- `message_bus.rs`: 11 tests

**缺失的测试**:
- Python 集成测试 (需要 maturin develop 后在 Python 环境中运行)
- 多线程并发访问测试 (Arc<Mutex<...>> 模式)

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
| `src/python/cta_strategy.py` | CtaStrategy 向后兼容 shim |
| `src/python/spot_strategy.py` | SpotStrategy 基类 |
| `src/chart/sync_bar_generator.rs` | SynchronizedBarGenerator |

## 修改文件清单

| 文件路径 | 修改内容 |
|---------|---------|
| `src/python/strategy.rs` | 添加 portfolio, order_factory 字段 |
| `src/python/engine.rs` | 注入 portfolio + order_factory |
| `src/python/backtesting_bindings.rs` | 添加 portfolio_state |
| `src/python/bindings.rs` | 注册新模块 |
| `src/python/mod.rs` | 导出新模块 |
| `src/trader/mod.rs` | 添加 identifier 模块 |
| `src/lib.rs` | re-exports |
| `src/chart/mod.rs` | 导出 SynchronizedBarGenerator |

---

## Oracle 架构决策记录

| 决策点 | 方案 | 理由 |
|---|---|---|
| 策略统一 | 保留 `PythonStrategyAdapter`，暴露单一 Python 基类 `Strategy` | 适配器已解决 Python↔Rust dispatch 难题 |
| Portfolio | 只读 `PortfolioFacade`，引擎持有可变状态 | 防止策略损坏状态，匹配 nautilus 模式 |
| OrderFactory | `order_factory` + 保留 `buy/sell/short/cover` 便利方法 | CTA 用户保持简洁，高级用户获得完整 API |
| Typed IDs | Newtype struct + `FromStr`/`Display` | 编译时类型安全 + 向后兼容 |
| 向后兼容 | `CtaStrategy(Strategy)` Python 子类作为兼容 shim | 新 API 干净，旧策略继承 shim 无需修改 |

---

## PyO3 0.27+ 关键注意事项

- `Py<PyAny>` **不实现 Clone** — 必须用 `e.clone_ref(py)` 来克隆 Python 引用
- `#[pymethods]` 中 `py: Python` 参数由 PyO3 自动注入，Python 调用时不可见
- `#[pymethods]` 的方法不能从 Rust 直接调用，需创建 Rust-callable `build_*` 方法
- `call1` 需要 `py` 作为第一个参数：`strategy_class.call1(py, (args,))`
- 创建 `Py<T>`: `Py::new(py, rust_instance)?`
- 转换 `Bound<'_, Self>` 到 `Py<PyAny>`: `slf.clone().into_any().unbind()`

---

## 构建与测试命令

```bash
# 编译检查
cargo check --features python

# 运行测试
cargo test --features python

# 构建 Python wheel
maturin develop --release --features python

# 运行 GUI (需要 gui feature)
cargo run --release --bin trade_engine_app --features gui
```

---

## 下一步优先级

1. **高优先级**: 清理 217 个弃用警告 (修改 strategy_bindings.rs 或迁移使用者)
2. **中优先级**: 添加 Python 示例和文档
3. **低优先级**: MessageBus 注入、SynchronizedBarGenerator PyO3 绑定、RiskManager 集成
