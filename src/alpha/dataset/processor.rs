//! Provides common data processing functions for alpha factor analysis
//!
//! This module implements simplified data processing functions for alpha research.

#[cfg(feature = "alpha")]
use polars::prelude::*;

/// Drop NA values from a column
#[cfg(feature = "alpha")]
pub fn drop_na(df: &DataFrame, col_name: &str) -> PolarsResult<DataFrame> {
    df.clone().lazy()
        .filter(col(col_name).is_not_null())
        .collect()
}

#[cfg(not(feature = "alpha"))]
pub fn drop_na(_df: &(), _col_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("Alpha feature not enabled".into())
}

/// Fill NA values with zero
#[cfg(feature = "alpha")]
pub fn fill_na(df: &DataFrame, col_name: &str) -> PolarsResult<DataFrame> {
    let series = df.column(col_name)?;
    let values: Vec<f64> = series.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
    
    let fill_series = Column::new(col_name.into(), values);
    Ok(df.hstack(&[fill_series])?)
}

/// Cross-sectional normalization (Z-score)
#[cfg(feature = "alpha")]
pub fn normalize_zscore(df: &DataFrame, col_name: &str) -> PolarsResult<DataFrame> {
    let series = df.column(col_name)?.as_materialized_series();
    let mean_val = series.mean().unwrap_or(0.0);
    let std_val = series.std(1).unwrap_or(1.0);

    if std_val == 0.0 {
        return Ok(df.clone());
    }

    df.clone().lazy()
        .with_column((col(col_name) - lit(mean_val)) / lit(std_val))
        .collect()
}

/// Cross-sectional rank normalization
#[cfg(feature = "alpha")]
pub fn normalize_rank(df: &DataFrame, col_name: &str) -> PolarsResult<DataFrame> {
    let series = df.column(col_name)?;
    let values: Vec<f64> = series.f64()?.into_iter().flatten().collect();
    
    let mut indexed: Vec<(usize, f64)> = values.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    
    let mut ranks = vec![0.0; values.len()];
    for (rank, (idx, _)) in indexed.iter().enumerate() {
        ranks[*idx] = rank as f64;
    }
    
    let rank_series = Series::new(col_name.into(), ranks);
    Ok(df.hstack(&[rank_series.into()])?)
}

/// Log transformation
#[cfg(feature = "alpha")]
pub fn log_transform(df: &DataFrame, col_name: &str) -> PolarsResult<DataFrame> {
    let series = df.column(col_name)?;
    let values: Vec<f64> = series.f64()?.into_iter().flatten().collect();
    
    let log_values: Vec<f64> = values.iter().map(|&v| if v > 0.0 { v.ln() } else { 0.0 }).collect();
    let log_series = Series::new(col_name.into(), log_values);

    Ok(df.hstack(&[log_series.into()])?)
}

/// Type alias for processor function
#[cfg(feature = "alpha")]
pub type ProcessorFn = fn(&DataFrame, &str) -> PolarsResult<DataFrame>;

/// Get all available processors (only those with matching signature)
#[cfg(feature = "alpha")]
pub fn get_all_processors() -> Vec<(&'static str, ProcessorFn)> {
    vec![
        ("drop_na", drop_na),
        ("fill_na", fill_na),
        ("normalize_zscore", normalize_zscore),
        ("normalize_rank", normalize_rank),
        ("log_transform", log_transform),
    ]
}

#[cfg(not(feature = "alpha"))]
pub fn get_all_processors() -> Vec<(&'static str, fn())> {
    vec![]
}