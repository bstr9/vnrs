//! Offline feature store using Parquet files for backtesting.
//!
//! The `OfflineStore` provides persistent storage for feature vectors
//! using the Parquet columnar format. This is gated behind the `alpha`
//! feature since it depends on `polars` and `arrow`.
//!
//! When the `alpha` feature is disabled, stub methods are provided
//! that return appropriate empty/default values.

use super::types::FeatureVector;

/// Offline feature store for persistent storage and backtesting.
///
/// When the `alpha` feature is enabled, uses Parquet files via polars
/// for efficient columnar storage of feature data. When disabled,
/// provides stub implementations.
pub struct OfflineStore {
    /// Base directory for Parquet file storage
    base_path: String,
}

impl OfflineStore {
    /// Create a new offline store with the given base path.
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Create a new offline store with default path.
    pub fn with_default_path() -> Self {
        Self::new(".rstrader/feature_store/offline")
    }

    /// Get the base path.
    pub fn base_path(&self) -> &str {
        &self.base_path
    }
}

#[cfg(feature = "alpha")]
impl OfflineStore {
    /// Save feature vectors to a Parquet file.
    ///
    /// The data is saved in columnar Parquet format for efficient
    /// column-wise reads during backtesting.
    ///
    /// # Arguments
    /// * `path` - File path (relative to base_path) for the Parquet file
    /// * `vectors` - Feature vectors to save
    pub fn save(&self, path: &str, vectors: &[FeatureVector]) -> Result<(), String> {
        use polars::prelude::*;

        if vectors.is_empty() {
            return Ok(());
        }

        // Build columns from feature vectors
        let mut entity_col: Vec<String> = Vec::with_capacity(vectors.len());
        let mut timestamp_col: Vec<String> = Vec::with_capacity(vectors.len());
        let mut feature_names: Vec<String> = Vec::new();

        // Collect all feature names from all vectors
        for v in vectors {
            for key in v.features.keys() {
                if !feature_names.contains(&key.0) {
                    feature_names.push(key.0.clone());
                }
            }
        }
        feature_names.sort();

        // Build data columns
        let mut feature_columns: Vec<Vec<Option<f64>>> =
            vec![vec![None; vectors.len()]; feature_names.len()];

        for (i, v) in vectors.iter().enumerate() {
            entity_col.push(v.entity.clone());
            timestamp_col.push(v.timestamp.to_rfc3339());

            for (j, fname) in feature_names.iter().enumerate() {
                let fid = crate::feature::types::FeatureId::new(fname);
                if let Some(val) = v.features.get(&fid) {
                    feature_columns[j][i] = val.as_f64();
                }
            }
        }

        // Build polars DataFrame
        let entity_col = Column::new("entity".into(), entity_col);
        let timestamp_col = Column::new("timestamp".into(), timestamp_col);

        let mut columns: Vec<Column> = vec![entity_col, timestamp_col];

        for (j, fname) in feature_names.iter().enumerate() {
            let col = Column::new(fname.into(), feature_columns[j].clone());
            columns.push(col);
        }

        let df = DataFrame::new(columns)
            .map_err(|e| format!("Failed to create DataFrame: {e}"))?;

        // Ensure directory exists
        let full_path = std::path::Path::new(&self.base_path).join(path);
        if let Some(parent) = full_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Write Parquet file
        let file = std::fs::File::create(&full_path)
            .map_err(|e| format!("Failed to create file {:?}: {e}", full_path))?;
        ParquetWriter::new(file)
            .finish(&mut df.clone())
            .map_err(|e| format!("Failed to write Parquet: {e}"))?;

        Ok(())
    }

    /// Load feature vectors from a Parquet file for a given entity and time range.
    ///
    /// # Arguments
    /// * `path` - File path (relative to base_path) for the Parquet file
    /// * `entity` - Entity to filter for (empty string = all entities)
    /// * `start` - Start timestamp (RFC3339 format)
    /// * `end` - End timestamp (RFC3339 format)
    pub fn load(
        &self,
        path: &str,
        entity: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<FeatureVector>, String> {
        use polars::prelude::*;

        let full_path = std::path::Path::new(&self.base_path).join(path);

        if !full_path.exists() {
            return Ok(Vec::new());
        }

        let file = std::fs::File::open(&full_path)
            .map_err(|e| format!("Failed to open file {:?}: {e}", full_path))?;

        let df = ParquetReader::new(file)
            .finish()
            .map_err(|e| format!("Failed to read Parquet: {e}"))?;

        // Filter by entity if specified
        let df = if !entity.is_empty() {
            let entity_col = df
                .column("entity")
                .map_err(|e| format!("Missing entity column: {e}"))?;
            let str_col = entity_col
                .str()
                .map_err(|e| format!("Failed to cast entity column: {e}"))?;
            let mask = str_col.equal(entity);
            df.filter(&mask)
                .map_err(|e| format!("Failed to filter entity: {e}"))?
        } else {
            df
        };

        // Filter by timestamp range
        let df = {
            let ts_col = df
                .column("timestamp")
                .map_err(|e| format!("Missing timestamp column: {e}"))?;
            let str_col = ts_col
                .str()
                .map_err(|e| format!("Failed to cast timestamp column: {e}"))?;
            let mask_ge = str_col.gt_eq(start);
            let mask_le = str_col.lt_eq(end);
            let mask = mask_ge & mask_le;
            df.filter(&mask)
                .map_err(|e| format!("Failed to filter timestamp: {e}"))?
        };

        // Convert DataFrame to FeatureVectors
        dataframe_to_feature_vectors(&df)
    }
}

#[cfg(feature = "alpha")]
fn dataframe_to_feature_vectors(
    df: &polars::prelude::DataFrame,
) -> Result<Vec<FeatureVector>, String> {

    let entity_col = df
        .column("entity")
        .map_err(|e| format!("Missing entity column: {e}"))?;
    let ts_col = df
        .column("timestamp")
        .map_err(|e| format!("Missing timestamp column: {e}"))?;

    let entity_values: Vec<Option<&str>> = entity_col
        .str()
        .map_err(|e| format!("Failed to cast entity column: {e}"))?
        .into_iter()
        .collect();
    let ts_values: Vec<Option<&str>> = ts_col
        .str()
        .map_err(|e| format!("Failed to cast timestamp column: {e}"))?
        .into_iter()
        .collect();

    // Collect feature column names (excluding entity and timestamp)
    let feature_names: Vec<String> = df
        .get_column_names()
        .iter()
        .filter(|n| **n != "entity" && **n != "timestamp")
        .map(|n| n.to_string())
        .collect();

    let mut vectors = Vec::with_capacity(df.height());

    for i in 0..df.height() {
        let entity = entity_values[i].unwrap_or("").to_string();
        let ts_str = ts_values[i].unwrap_or("");
        let timestamp = chrono::DateTime::parse_from_rfc3339(ts_str)
            .map(|dt| dt.to_utc())
            .unwrap_or_else(|_| chrono::Utc::now());

        let mut features = std::collections::HashMap::new();
        for fname in &feature_names {
            if let Ok(col) = df.column(fname) {
                if let Ok(f64_col) = col.f64() {
                    if let Some(val) = f64_col.get(i) {
                        features.insert(
                            crate::feature::types::FeatureId::new(fname),
                            crate::feature::types::FeatureValue::Float64(val),
                        );
                    }
                }
            }
        }

        vectors.push(FeatureVector {
            entity,
            timestamp,
            features,
        });
    }

    Ok(vectors)
}

/// Stub implementations when alpha feature is disabled.
#[cfg(not(feature = "alpha"))]
impl OfflineStore {
    /// Save feature vectors (stub — requires alpha feature).
    pub fn save(&self, _path: &str, _vectors: &[FeatureVector]) -> Result<(), String> {
        Err("OfflineStore::save requires the 'alpha' feature to be enabled".to_string())
    }

    /// Load feature vectors (stub — requires alpha feature).
    pub fn load(
        &self,
        _path: &str,
        _entity: &str,
        _start: &str,
        _end: &str,
    ) -> Result<Vec<FeatureVector>, String> {
        Err("OfflineStore::load requires the 'alpha' feature to be enabled".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_store_new() {
        let store = OfflineStore::new("/tmp/test_features");
        assert_eq!(store.base_path(), "/tmp/test_features");
    }

    #[test]
    fn test_offline_store_default_path() {
        let store = OfflineStore::with_default_path();
        assert!(store.base_path().contains("feature_store"));
    }

    #[test]
    #[cfg(not(feature = "alpha"))]
    fn test_offline_store_save_stub() {
        let store = OfflineStore::new("/tmp/test");
        let result = store.save("test.parquet", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alpha"));
    }

    #[test]
    #[cfg(not(feature = "alpha"))]
    fn test_offline_store_load_stub() {
        let store = OfflineStore::new("/tmp/test");
        let result = store.load("test.parquet", "btcusdt", "2024-01-01", "2024-12-31");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alpha"));
    }
}
