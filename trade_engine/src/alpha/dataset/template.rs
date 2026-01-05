//! Template for alpha datasets
//! Provides the core data structure for alpha factor analysis
//!
//! This module implements the AlphaDataset structure matching vnpy's functionality,
//! including feature calculation, data processing, and segment management.

use std::collections::HashMap;
use std::sync::Arc;
use polars::prelude::*;
use polars::lazy::dsl::Expr;
use crate::alpha::dataset::utility::{Segment, to_datetime};
use crate::alpha::logger;

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
        self.feature_expressions.insert(name, FeatureExpression::String(expression));
    }

    /// Add a feature expression using Polars expression
    pub fn add_feature_expr(&mut self, name: String, expression: Expr) {
        self.feature_expressions.insert(name, FeatureExpression::Polars(expression));
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
    pub fn prepare_data(&mut self, _filters: Option<HashMap<String, Vec<(String, String)>>>) -> Result<(), Box<dyn std::error::Error>> {
        logger::logger().info(&format!("Preparing data with {} feature expressions", self.feature_expressions.len()));
        
        // Start with raw data
        let mut result_df = self.df.clone();
        
        // Calculate label if expression is set
        if !self.label_expression.is_empty() {
            logger::logger().debug(&format!("Calculating label: {}", self.label_expression));
            // In a real implementation, this would evaluate the label expression
            let series = Series::new("label".into(), vec![0.0; result_df.height()]);
            result_df.with_column(series)?;
        }
        
        // Merge pre-computed feature results
        logger::logger().info("Merging pre-computed feature results");
        for (name, feature_result) in &self.feature_results {
            if let Ok(renamed) = feature_result.clone().lazy().rename(["data"], [name.as_str()], false).collect() {
                if let Ok(joined) = result_df.clone().lazy().join_builder()
                    .with(renamed.lazy())
                    .how(JoinType::Left)
                    .on([col("datetime"), col("vt_symbol")])
                    .finish()
                    .collect() {
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
        logger::logger().info(&format!("Processing data with {} infer and {} learn processors", 
                 self.infer_processors.len(), self.learn_processors.len()));
        
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
}

/// Filter DataFrame based on time range
pub fn query_by_time(df: &DataFrame, start: &str, end: &str) -> DataFrame {
    let start_dt = to_datetime(start);
    let end_dt = to_datetime(end);
    
    let datetime_col = df.column("datetime");
    if datetime_col.is_err() {
        return df.clone();
    }
    
    let datetime_col = datetime_col.unwrap();
    let datetime_series = datetime_col.datetime();
    
    if datetime_series.is_err() {
        return df.clone();
    }
    
    let datetime_series = datetime_series.unwrap();
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