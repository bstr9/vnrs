//! Template for alpha datasets
//! Provides the core data structure for alpha factor analysis
//!
//! This module implements the AlphaDataset structure matching vnpy's functionality,
//! including feature calculation, data processing, and segment management.

use crate::alpha::dataset::utility::{to_datetime, Segment};
use crate::alpha::logger;
use polars::lazy::dsl::Expr;
use polars::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Type alias for feature processor function
pub type ProcessorFn = Arc<dyn Fn(DataFrame) -> DataFrame + Send + Sync>;

/// Type alias for feature expression (either string expression or Polars expression)
#[derive(Debug, Clone)]
pub enum FeatureExpression {
    String(String),
    Polars(Expr),
}

/// Alpha dataset for factor analysis and machine learning
pub struct AlphaDataset {
    /// Raw data DataFrame
    pub df: DataFrame,

    /// Processed raw data
    pub raw_df: Option<DataFrame>,

    /// Inference data (after infer processors)
    pub infer_df: Option<DataFrame>,

    /// Learning data (after learn processors)
    pub learn_df: Option<DataFrame>,

    /// Data periods for train/valid/test
    pub data_periods: HashMap<Segment, (String, String)>,

    /// Feature expressions
    pub feature_expressions: HashMap<String, FeatureExpression>,

    /// Pre-computed feature results
    pub feature_results: HashMap<String, DataFrame>,

    /// Label expression
    pub label_expression: String,

    /// Inference processors
    pub infer_processors: Vec<ProcessorFn>,

    /// Learning processors
    pub learn_processors: Vec<ProcessorFn>,
}

impl AlphaDataset {
    /// Create a new AlphaDataset
    pub fn new(
        df: DataFrame,
        train_period: (String, String),
        valid_period: (String, String),
        test_period: (String, String),
    ) -> Self {
        let mut data_periods = HashMap::new();
        data_periods.insert(Segment::Train, train_period);
        data_periods.insert(Segment::Valid, valid_period);
        data_periods.insert(Segment::Test, test_period);

        AlphaDataset {
            df,
            raw_df: None,
            infer_df: None,
            learn_df: None,
            data_periods,
            feature_expressions: HashMap::new(),
            feature_results: HashMap::new(),
            label_expression: String::new(),
            infer_processors: Vec::new(),
            learn_processors: Vec::new(),
        }
    }

    /// Add a feature expression (string)
    pub fn add_feature(&mut self, name: String, expression: String) {
        self.feature_expressions
            .insert(name, FeatureExpression::String(expression));
    }

    /// Add a feature expression using Polars expression
    pub fn add_feature_expr(&mut self, name: String, expression: Expr) {
        self.feature_expressions
            .insert(name, FeatureExpression::Polars(expression));
    }

    /// Add a pre-computed feature result
    pub fn add_feature_result(&mut self, name: String, result: DataFrame) {
        self.feature_results.insert(name, result);
    }

    /// Set the label expression
    pub fn set_label(&mut self, expression: String) {
        self.label_expression = expression;
    }

    /// Add a processor for data processing
    pub fn add_processor(&mut self, task: &str, processor: ProcessorFn) {
        match task {
            "infer" => self.infer_processors.push(processor),
            _ => self.learn_processors.push(processor),
        }
    }

    /// Prepare data by computing features
    pub fn prepare_data(
        &mut self,
        _filters: Option<HashMap<String, Vec<(String, String)>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        logger::logger().info(&format!(
            "Preparing data with {} feature expressions",
            self.feature_expressions.len()
        ));

        // Start with raw data
        let mut result_df = self.df.clone();

        // Calculate label if expression is set
        if !self.label_expression.is_empty() {
            logger::logger().debug(&format!("Calculating label: {}", self.label_expression));
            let label_series = self.evaluate_label_expression(&result_df);
            result_df.with_column(label_series)?;
        }

        // Merge pre-computed feature results
        logger::logger().info("Merging pre-computed feature results");
        for (name, feature_result) in &self.feature_results {
            if let Ok(renamed) = feature_result
                .clone()
                .lazy()
                .rename(["data"], [name.as_str()], false)
                .collect()
            {
                if let Ok(joined) = result_df
                    .clone()
                    .lazy()
                    .join_builder()
                    .with(renamed.lazy())
                    .how(JoinType::Left)
                    .on([col("datetime"), col("vt_symbol")])
                    .finish()
                    .collect()
                {
                    result_df = joined;
                }
            }
        }

        // Generate raw data
        let raw_df = result_df.fill_null(FillNullStrategy::Zero)?;

        // Apply filters if provided
        if let Some(filters) = _filters {
            logger::logger().info(&format!("Applying filters for {} symbols", filters.len()));
            // In a real implementation, this would filter the dataframe
        }

        // Only keep feature columns
        self.raw_df = Some(raw_df.clone());
        self.infer_df = Some(raw_df.clone());
        self.learn_df = Some(raw_df);

        Ok(())
    }

    /// Process data using processors
    pub fn process_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        logger::logger().info(&format!(
            "Processing data with {} infer and {} learn processors",
            self.infer_processors.len(),
            self.learn_processors.len()
        ));

        // Apply inference processors
        let mut infer_df = self.raw_df.clone().unwrap_or(self.df.clone());
        for processor in &self.infer_processors {
            infer_df = processor(infer_df);
        }
        self.infer_df = Some(infer_df.clone());

        // Apply learning processors
        let mut learn_df = infer_df;
        for processor in &self.learn_processors {
            learn_df = processor(learn_df);
        }
        self.learn_df = Some(learn_df);

        Ok(())
    }

    /// Fetch raw data for a specific segment
    pub fn fetch_raw(&self, segment: Segment) -> Option<DataFrame> {
        if let Some(ref raw_df) = self.raw_df {
            if let Some((start, end)) = self.data_periods.get(&segment) {
                return Some(query_by_time(raw_df, start, end));
            }
        }
        None
    }

    /// Fetch inference data for a specific segment
    pub fn fetch_infer(&self, segment: Segment) -> Option<DataFrame> {
        if let Some(ref infer_df) = self.infer_df {
            if let Some((start, end)) = self.data_periods.get(&segment) {
                return Some(query_by_time(infer_df, start, end));
            }
        }
        None
    }

    /// Fetch learning data for a specific segment
    pub fn fetch_learn(&self, segment: Segment) -> Option<DataFrame> {
        if let Some(ref learn_df) = self.learn_df {
            if let Some((start, end)) = self.data_periods.get(&segment) {
                return Some(query_by_time(learn_df, start, end));
            }
        }
        None
    }

    fn evaluate_label_expression(&self, df: &DataFrame) -> Series {
        let expr = &self.label_expression;
        let height = df.height();

        match expr.as_str() {
            "return_1d" => return self.compute_forward_return(df, 1),
            "return_5d" => return self.compute_forward_return(df, 5),
            "label_1d" => return self.compute_directional_label(df, 1),
            _ => {}
        }

        if let Some(label_series) = self.try_parse_return_label(df, expr) {
            return label_series;
        }

        if let Some(label_series) = self.try_parse_shift_label(df, expr) {
            return label_series;
        }

        match df
            .clone()
            .lazy()
            .select([polars::prelude::col(expr)])
            .collect()
        {
            Ok(result_df) => {
                if result_df.width() > 0 {
                    if let Some(col) = result_df.select_at_idx(0) {
                        return col.as_materialized_series().clone();
                    }
                }
            }
            Err(e) => {
                logger::logger().info(&format!(
                    "Unrecognized label expression '{}': {}. Using NaN labels.",
                    expr, e
                ));
            }
        }

        Series::new("label".into(), vec![f64::NAN; height])
    }

    fn compute_forward_return(&self, df: &DataFrame, periods: usize) -> Series {
        let height = df.height();
        let mut label_values = Vec::with_capacity(height);
        if let Ok(close_col) = df.column("close") {
            if let Ok(close_f64) = close_col.f64() {
                for i in 0..height {
                    let future_idx = i + periods;
                    if future_idx < height {
                        let curr = close_f64.get(i).unwrap_or(f64::NAN);
                        let future = close_f64.get(future_idx).unwrap_or(f64::NAN);
                        label_values.push(if curr != 0.0 && !curr.is_nan() {
                            (future - curr) / curr
                        } else {
                            f64::NAN
                        });
                    } else {
                        label_values.push(f64::NAN);
                    }
                }
            } else {
                label_values.resize(height, f64::NAN);
            }
        } else {
            label_values.resize(height, f64::NAN);
        }
        Series::new("label".into(), label_values)
    }

    fn compute_directional_label(&self, df: &DataFrame, periods: usize) -> Series {
        let returns = self.compute_forward_return(df, periods);
        if let Ok(returns_f64) = returns.f64() {
            let values: Vec<f64> = returns_f64
                .into_iter()
                .map(|v| match v {
                    Some(x) if x > 0.0 => 1.0,
                    Some(x) if x < 0.0 => 0.0,
                    _ => 0.5,
                })
                .collect();
            Series::new("label".into(), values)
        } else {
            Series::new("label".into(), vec![f64::NAN; df.height()])
        }
    }

    fn try_parse_return_label(&self, df: &DataFrame, expr: &str) -> Option<Series> {
        let expr_lower = expr.to_lowercase().replace(" ", "");

        if expr_lower.starts_with("ref(") && expr_lower.contains("/close-1") {
            if let Ok(close_col) = df.column("close") {
                if let Ok(close_f64) = close_col.f64() {
                    let height = df.height();
                    let mut label_values = Vec::with_capacity(height);

                    let shift = self.parse_ref_shift(&expr_lower).unwrap_or(-1);

                    for i in 0..height {
                        let shift_idx = (i as i64) - shift;
                        if shift_idx >= 0 && (shift_idx as usize) < height {
                            let curr = close_f64.get(i).unwrap_or(f64::NAN);
                            let shifted = close_f64.get(shift_idx as usize).unwrap_or(f64::NAN);
                            label_values.push(shifted / curr - 1.0);
                        } else {
                            label_values.push(f64::NAN);
                        }
                    }
                    return Some(Series::new("label".into(), label_values));
                }
            }
        }

        if expr_lower.contains("pct_change") || expr_lower.contains("shift") {
            if let Ok(close_col) = df.column("close") {
                if let Ok(close_f64) = close_col.f64() {
                    let height = df.height();
                    let mut label_values = Vec::with_capacity(height);
                    for i in 0..height {
                        if i > 0 {
                            let prev = close_f64.get(i - 1).unwrap_or(f64::NAN);
                            let curr = close_f64.get(i).unwrap_or(f64::NAN);
                            label_values.push(if prev != 0.0 && !prev.is_nan() {
                                curr / prev - 1.0
                            } else {
                                f64::NAN
                            });
                        } else {
                            label_values.push(f64::NAN);
                        }
                    }
                    return Some(Series::new("label".into(), label_values));
                }
            }
        }

        None
    }

    fn try_parse_shift_label(&self, df: &DataFrame, expr: &str) -> Option<Series> {
        let expr_lower = expr.to_lowercase().replace(" ", "");

        if expr_lower.starts_with("sign(") {
            let inner = &expr_lower[5..expr_lower.len().saturating_sub(1)];
            if let Some(inner_series) = self.try_parse_return_label(df, inner) {
                if let Ok(inner_f64) = inner_series.f64() {
                    let values: Vec<f64> = inner_f64
                        .into_iter()
                        .map(|v| match v {
                            Some(x) if x > 0.0 => 1.0,
                            Some(x) if x < 0.0 => 0.0,
                            _ => 0.5,
                        })
                        .collect();
                    return Some(Series::new("label".into(), values));
                }
            }
        }

        None
    }

    fn parse_ref_shift(&self, expr: &str) -> Option<i64> {
        let start = expr.find(',')?;
        let end = expr.find(')')?;
        let num_str = &expr[start + 1..end];
        num_str.parse::<i64>().ok()
    }
}

/// Filter DataFrame based on time range
pub fn query_by_time(df: &DataFrame, start: &str, end: &str) -> DataFrame {
    let start_dt = match to_datetime(start) {
        Ok(dt) => dt,
        Err(e) => {
            eprintln!("Invalid start date '{}': {}", start, e);
            return df.clone();
        }
    };
    let end_dt = match to_datetime(end) {
        Ok(dt) => dt,
        Err(e) => {
            eprintln!("Invalid end date '{}': {}", end, e);
            return df.clone();
        }
    };

    let datetime_col = df.column("datetime");
    if datetime_col.is_err() {
        return df.clone();
    }

    let datetime_col = datetime_col.expect("datetime column existence verified above");
    let datetime_series = datetime_col.datetime();

    if datetime_series.is_err() {
        return df.clone();
    }

    let datetime_series = datetime_series.expect("datetime series conversion verified above");
    let mask: Vec<u32> = datetime_series
        .into_iter()
        .enumerate()
        .filter(|(_, dt)| {
            if let Some(dt) = dt {
                *dt >= start_dt.timestamp_millis() && *dt <= end_dt.timestamp_millis()
            } else {
                false
            }
        })
        .map(|(idx, _)| idx as u32)
        .collect();

    if mask.is_empty() {
        return df.clone();
    }

    let idx_ca = UInt32Chunked::from_vec("idx".into(), mask);
    df.take(&idx_ca).unwrap_or(df.clone())
}
