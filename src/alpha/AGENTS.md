# ALPHA MODULE

Quantitative research platform for factor analysis and ML-based strategies.

## OVERVIEW
Data pipeline for alpha research: dataset processing, ML models, strategy backtesting.

## STRUCTURE
```
alpha/
├── mod.rs           # Module exports
├── lab.rs           # AlphaLab - research orchestration
├── model.rs         # ML models (LinearRegression, Ridge, Lasso, RandomForest, GradientBoosting)
├── types.rs         # AlphaBarData
├── logger.rs        # Alpha-specific logging
├── dataset/         # Data processing pipeline
│   ├── template.rs  # AlphaDataset with train/valid/test segmentation
│   ├── processor.rs # Polars processors (normalize, drop_na, log_transform)
│   └── utility.rs   # Segment enum, datetime parsing
└── strategy/        # Alpha strategy framework
    ├── template.rs  # AlphaStrategy template
    └── backtesting.rs # Alpha backtesting engine
```

## WHERE TO LOOK
| Task | Location |
|------|----------|
| Add data processor | `dataset/processor.rs` - ProcessorFn type |
| Add ML model | `model.rs` - implement model traits |
| Factor expressions | `dataset/template.rs` - FeatureExpression |
| Strategy template | `strategy/template.rs` - AlphaStrategy |

## KEY PATTERNS
- **Polars DataFrame**: High-performance data processing
- **Segment-based splitting**: Train/Valid/Test for ML workflow
- **Feature expressions**: Dual representation (String + Polars Expr)
- **Thread-safe positions**: `Arc<Mutex<HashMap<String, f64>>>`

## CONVENTIONS
- Labels computed as `return_1d`, `return_5d`, `label_1d`
- Models use Gaussian elimination solver (LinearRegression)
- Strategy tracks target positions, executes diff from current
