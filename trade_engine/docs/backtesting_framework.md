# 回测框架 (Backtesting Framework)

## 概述

参照 `vnpy_ctabacktester` 实现的回测框架,支持 Python 策略在 Rust 引擎中运行回测。

### 主要特点

- **Python 策略,Rust 引擎**: 策略在 Python 实现,回测引擎使用 Rust 获得高性能
- **支持现货和期货**: 不仅限于 CTA 策略,支持多种交易品种
- **与 Strategy 模块集成**: 充分利用已实现的 `StrategyTemplate` trait
- **独立于 Alpha 模块**: Alpha 模块用于研究,此模块用于 CTA 策略回测

## 架构

### Rust 侧模块

```
src/backtesting/
├── mod.rs           # 模块导出
├── base.rs          # 基础类型和数据结构
├── statistics.rs    # 统计指标计算
└── engine.rs        # 核心回测引擎
```

### Python 绑定

- `src/python/backtesting_bindings.rs`: PyO3 Python 绑定
- `examples/backtesting_example.py`: Python 使用示例

## 核心类型

### BacktestingMode

```rust
pub enum BacktestingMode {
    Bar,   // K线回测
    Tick,  // Tick回测 (TODO)
}
```

### DailyResult

每日交易统计:

```rust
pub struct DailyResult {
    pub date: NaiveDate,         // 日期
    pub close_price: f64,        // 收盘价
    pub pre_close: f64,          // 昨收价
    pub trades: Vec<TradeData>,  // 成交记录
    pub trade_count: usize,      // 成交数量
    pub start_pos: f64,          // 起始持仓
    pub end_pos: f64,            // 结束持仓
    pub turnover: f64,           // 成交额
    pub commission: f64,         // 手续费
    pub slippage: f64,           // 滑点
    pub trading_pnl: f64,        // 交易盈亏
    pub holding_pnl: f64,        // 持仓盈亏
    pub total_pnl: f64,          // 总盈亏
    pub net_pnl: f64,            // 净盈亏
}
```

### BacktestingStatistics

综合统计指标:

```rust
pub struct BacktestingStatistics {
    pub start_date: NaiveDate,   // 开始日期
    pub end_date: NaiveDate,     // 结束日期
    pub total_days: usize,       // 总天数
    pub profit_days: usize,      // 盈利天数
    pub loss_days: usize,        // 亏损天数
    pub capital: f64,            // 初始资金
    pub end_balance: f64,        // 结束余额
    pub max_drawdown: f64,       // 最大回撤
    pub max_ddpercent: f64,      // 最大回撤百分比
    pub total_return: f64,       // 总收益率
    pub annual_return: f64,      // 年化收益率
    pub sharpe_ratio: f64,       // 夏普比率
    pub return_std: f64,         // 收益率标准差
    pub total_commission: f64,   // 总手续费
    pub total_slippage: f64,     // 总滑点
    pub total_turnover: f64,     // 总成交额
    pub total_trade_count: usize,// 总成交笔数
}
```

## 使用方法

### 1. 定义策略

策略需要实现 `StrategyTemplate` trait:

```python
from trade_engine import CtaTemplate

class MyStrategy(CtaTemplate):
    def __init__(self):
        super().__init__()
        self.fast_window = 10
        self.slow_window = 20
        self.fast_ma = 0.0
        self.slow_ma = 0.0
    
    def on_init(self):
        print("[STRATEGY] Initializing...")
        self.load_bar(10)
    
    def on_start(self):
        print("[STRATEGY] Starting...")
    
    def on_bar(self, bar):
        # 计算均线
        # 生成交易信号
        # 执行下单
        pass
```

### 2. 配置回测参数

```python
from trade_engine import PyBacktestingEngine

engine = PyBacktestingEngine()

# 设置参数
engine.set_parameters(
    vt_symbol="BTCUSDT.BINANCE",
    interval="1m",          # K线周期
    start="20230101",       # 开始日期
    end="20231231",         # 结束日期
    rate=0.0003,            # 手续费率
    slippage=0.0001,        # 滑点
    size=1.0,               # 合约乘数
    pricetick=0.01,         # 最小价格变动
    capital=100000.0,       # 初始资金
    mode="bar"              # 回测模式
)
```

### 3. 加载历史数据

```python
# 准备历史数据
bars = []
for i in range(1000):
    bar = {
        'symbol': 'BTCUSDT',
        'exchange': 'BINANCE',
        'datetime': datetime.now() + timedelta(minutes=i),
        'interval': '1m',
        'open_price': 30000.0,
        'high_price': 30100.0,
        'low_price': 29900.0,
        'close_price': 30000.0,
        'volume': 1.0,
        'open_interest': 0.0
    }
    bars.append(bar)

# 设置数据
engine.set_history_data(bars)
```

### 4. 运行回测

```python
# 设置策略
strategy = MyStrategy()
engine.add_strategy(strategy)

# 运行回测
result = engine.calculate_result()

# 获取统计
stats = engine.calculate_statistics()

print(f"总收益率: {stats['total_return']*100:.2f}%")
print(f"年化收益: {stats['annual_return']*100:.2f}%")
print(f"夏普比率: {stats['sharpe_ratio']:.2f}")
print(f"最大回撤: {stats['max_ddpercent']*100:.2f}%")
```

## 回测逻辑

### Bar 回测流程

1. **初始化**: 调用策略 `on_init()`
2. **循环历史数据**: 
   - 更新当前 bar
   - 触发止损单
   - 撮合限价单
   - 调用策略 `on_bar()`
   - 更新持仓
   - 记录每日统计
3. **结算**: 最后一根 bar 收盘
4. **统计**: 计算综合指标

### 订单撮合

**限价单撮合**:
- 买入单: `order.price >= bar.low_price`
- 卖出单: `order.price <= bar.high_price`
- 成交价: 订单限价

**止损单触发**:
- 多头止损: `bar.high_price >= stop_order.price`
- 空头止损: `bar.low_price <= stop_order.price`
- 触发后转为限价单进行撮合

### 持仓管理

- 支持多头/空头/净持仓
- 开平仓逻辑 (现货 vs 期货)
- 持仓盈亏实时计算

### 统计计算

- **交易盈亏**: 平仓产生的盈亏
- **持仓盈亏**: 未平仓持仓的浮动盈亏
- **总盈亏**: 交易盈亏 + 持仓盈亏
- **净盈亏**: 总盈亏 - 手续费 - 滑点

## 性能优势

相比 Python 实现的回测:

- **计算速度**: Rust 实现的数值计算速度提升 10-100 倍
- **内存效率**: 更紧凑的内存布局,减少 GC 压力
- **并发能力**: 易于实现参数优化的并行回测

## 与 vnpy_ctabacktester 的差异

### 相同点

- 回测逻辑一致 (bar 回测、订单撮合、统计计算)
- 策略接口类似 (on_init, on_start, on_bar 等)
- 统计指标相同 (夏普比率、最大回撤等)

### 不同点

1. **实现语言**: Rust 引擎 vs 纯 Python
2. **支持品种**: 现货 + 期货 vs 主要是期货
3. **性能**: 更快的回测速度
4. **扩展性**: 更容易集成到其他 Rust 模块

## 后续扩展

### 待实现功能

- [ ] Tick 级别回测
- [ ] 从数据库加载历史数据
- [ ] 参数优化支持
- [ ] 多品种组合回测
- [ ] 回测结果可视化
- [ ] 性能报告生成

### 集成计划

- [ ] 与 `vnpy_datarecorder` 集成获取数据
- [ ] 与 `vnpy_postgresql` 集成数据存储
- [ ] 与策略引擎集成实盘交易
- [ ] UI 面板展示回测结果

## 示例

完整示例见:
- `examples/backtesting_example.py`: Python 策略回测示例
- `examples/strategy_example.py`: 策略实现示例 (双均线、网格策略)

## 编译和安装

```bash
# 编译 Rust 库
cd trade_engine
cargo build --release

# 构建 Python 包 (TODO)
pip install maturin
maturin develop

# 运行示例
python examples/backtesting_example.py
```

## 技术细节

### 借用检查器优化

为避免借用冲突,使用以下模式:

```rust
// 错误: 同时 immut 和 mut 借用 self
for (id, order) in self.orders.iter() {
    self.remove_order(id);  // ERROR
}

// 正确: 先收集再操作
let orders: Vec<_> = self.orders.iter()
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect();

for (id, order) in orders {
    self.remove_order(&id);  // OK
}
```

### PyO3 API 兼容性

注意 PyO3 版本差异:

```rust
// PyO3 0.27.2 使用旧 API
let dict = PyDict::new(py);  // 返回 Py<PyDict>

// 新版本使用
let dict = PyDict::new_bound(py);  // 返回 Bound<PyDict>
```

## 常见问题

**Q: 回测速度有多快?**

A: Rust 实现通常比纯 Python 快 10-100 倍,具体取决于策略复杂度。

**Q: 支持哪些交易品种?**

A: 支持现货和期货,通过 `Offset` 区分开平仓。

**Q: 如何导入历史数据?**

A: 目前通过 `set_history_data()` 导入,后续会支持从数据库加载。

**Q: 能否并行回测多个参数?**

A: 当前版本尚未实现,计划后续添加参数优化功能。

## 参考

- `vnpy_ctabacktester`: [官方文档](https://github.com/vnpy/vnpy_ctabacktester)
- `src/strategy/`: 策略模块实现
- `src/alpha/`: Alpha 研究模块 (独立)
