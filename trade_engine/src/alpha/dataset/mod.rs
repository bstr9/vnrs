//! Dataset module for alpha research
//! Provides data structures and utilities for alpha factor analysis

pub mod template;
pub mod utility;
pub mod processor;

pub use template::{AlphaDataset, FeatureExpression, query_by_time};
pub use utility::{Segment, to_datetime};
pub use processor::{
    drop_na,
    fill_na,
    normalize_zscore,
    normalize_rank,
    log_transform,
    get_all_processors,
};