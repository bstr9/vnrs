---
id: REQ-058
title: "Alpha 量化研究平台：ML模型与因子分析"
status: completed
completed_at: "2026-04-22T00:00:00"
created_at: "2026-04-22T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P1
level: epic
cluster: Alpha-Research
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: []
  related_to: [REQ-055]
versions:
  - version: 1
    date: "2026-04-22T00:00:00"
    author: ai
    context: "代码审查发现 Alpha 模块已实现多种ML模型、数据管道、因子分析，但无对应需求记录"
    reason: "从代码逆向生成需求，确保需求覆盖已实现功能"
    snapshot: "Alpha 量化研究平台包含ML模型库、Polars数据管道、因子分析"
---

# Alpha 量化研究平台：ML模型与因子分析

## 描述
Alpha 模块提供完整的量化研究基础设施，包括 ML 模型库（LinearRegression、RandomForest、GradientBoosting、Ensemble）、基于 Polars 的高性能数据管道、因子/标签表达式系统、截面标准化处理器，以及 Alpha 策略回测引擎。所有模型实现 `AlphaModel` trait，支持 fit/predict 工作流，数据集支持 Train/Valid/Test 三段分割。

## 验收标准

### ML 模型库

- [x] AlphaModel trait 定义：fit(&mut self, dataset) / predict(&self, dataset, segment) / detail() / name()
- [x] LinearRegressionModel：基于正规方程 + 高斯消元法（部分主元选取）求解权重和偏置
- [x] LinearRegressionModel 支持 with_features() 构造器和 Default 实现
- [x] LinearRegressionModel 预测：y = X*weights + bias
- [x] RandomForestModel：Bagging + 特征子采样（sqrt(n_features)），纯 Rust 决策树
- [x] RandomForestModel 支持可配参数：n_estimators, max_depth, min_samples_split, seed
- [x] RandomForestModel Bootstrap 采样训练每棵树
- [x] RandomForestModel 特征重要性计算（基于方差减少量累积）
- [x] RandomForestModel 预测：多棵树平均
- [x] DecisionTree 内部实现：方差减少分裂准则、NaN 跳过、最小样本数控制
- [x] GradientBoostingModel：残差拟合序列决策树，学习率缩放
- [x] GradientBoostingModel 支持 n_estimators, learning_rate, max_depth 参数
- [x] GradientBoostingModel 初始预测为标签均值，逐轮拟合残差
- [x] GradientBoostingModel 预测：init_prediction + sum(learning_rate * tree(x))
- [x] EnsembleModel：多模型加权组合预测
- [x] EnsembleModel 支持 add_model(model, weight) 动态添加子模型
- [x] EnsembleModel 预测：加权平均 + 总权重归一化
- [x] Gaussian 消元求解器（solve_linear_system）：部分主元选取、奇异矩阵检测

### 数据管道与数据集

- [x] AlphaDataset 结构体：df / raw_df / infer_df / learn_df 多阶段数据
- [x] AlphaDataset Train/Valid/Test 三段时间段分割（data_periods HashMap）
- [x] FeatureExpression 双表示：String 表达式与 Polars Expr 表达式
- [x] AlphaDataset.add_feature() / add_feature_expr() 注册特征
- [x] AlphaDataset.add_feature_result() 注册预计算特征结果
- [x] AlphaDataset.set_label() 设置标签表达式
- [x] AlphaDataset.prepare_data()：计算标签、合并预计算特征、FillNull(Zero)
- [x] AlphaDataset.process_data()：依次应用 infer_processors 和 learn_processors
- [x] AlphaDataset.fetch_raw() / fetch_infer() / fetch_learn() 按时间段查询
- [x] query_by_time() 时间范围过滤 DataFrame
- [x] Segment 枚举：Train / Valid / Test
- [x] to_datetime() 支持 YYYYMMDD 和 YYYY-MM-DD 两种日期格式

### 标签计算

- [x] return_1d：1 日远期收益率标签
- [x] return_5d：5 日远期收益率标签
- [x] label_1d：1 日方向标签（涨=1.0, 跌=0.0, 平=0.5）
- [x] Ref 表达式解析：ref(close, N)/close - 1 格式
- [x] pct_change / shift 表达式解析
- [x] Sign 表达式嵌套解析：sign(return_expr)

### 数据处理器

- [x] drop_na：按列删除空值行
- [x] fill_na：前向填充（forward_fill），适合金融时序
- [x] normalize_zscore：截面 Z-score 标准化（减均值除标准差）
- [x] normalize_rank：截面排名标准化（[0, 1] 区间）
- [x] log_transform：对数变换（正数取 ln，非正 NaN）
- [x] get_all_processors()：处理器注册表
- [x] ProcessorFn 类型别名：fn(&DataFrame, &str) -> PolarsResult<DataFrame>

### Alpha 研究实验室

- [x] AlphaLab：数据集管理（HashMap<String, AlphaDataset>）和模型管理（HashMap<String, Box<dyn AlphaModel>>）
- [x] AlphaLab.save_bar_data()：OHLCV 数据保存为 Parquet 文件（按日线/分钟线路径分类）
- [x] AlphaLab.load_bar_data()：从 Parquet 加载 Bar 数据，按时间范围过滤
- [x] AlphaLab.list_all_datasets() / list_all_models() / list_all_signals()

### Alpha 策略与回测

- [x] AlphaStrategy：策略名、多品种（vt_symbols）、目标仓位管理
- [x] AlphaStrategy 线程安全仓位：Arc<Mutex<HashMap<String, f64>>> 用于 pos_data 和 target_data
- [x] AlphaStrategy 回调：on_init / on_bars / on_trade / on_stop
- [x] AlphaStrategy 交易方法：buy / sell / short / cover / send_order / cancel_order / cancel_all
- [x] AlphaStrategy.execute_trading()：根据目标仓位与当前仓位差值自动下单（含 price_add 滑价）
- [x] AlphaStrategy.get_pos() / get_target() / set_target() 仓位查询与设置
- [x] AlphaStrategy.get_cash_available() / get_holding_value() / get_portfolio_value()
- [x] BacktestingEngine：资金、回测区间、手续费率、滑点、合约规模配置
- [x] BacktestingEngine.set_parameters() / set_cost() / add_data() / add_strategy() / add_model()
- [x] BacktestingEngine.run_backtesting()：按时间排序回放 Bar 数据，触发策略回调
- [x] BacktestingEngine 订单撮合：限价单按 close 价格判断能否成交
- [x] BacktestingEngine.cross_order()：计算成交价（含滑点）、手续费、仓位更新、资金变动
- [x] BacktestingEngine.calculate_result()：按日聚合 PnL，输出 DataFrame
- [x] BacktestingEngine.calculate_statistics()：total_return / sharpe_ratio / max_drawdown / trade_count / win_rate
- [x] ContractDailyResult / PortfolioDailyResult：逐日结果跟踪

### 类型与日志

- [x] AlphaBarData：datetime / symbol / exchange / interval / OHLCV / turnover / open_interest / gateway_name
- [x] AlphaBarData.vt_symbol()：格式 "SYMBOL.EXCHANGE"
- [x] AlphaLogger：基于 tracing 的 info / error / warning / debug 日志
- [x] logger() 全局实例获取 + init_logger() 初始化
