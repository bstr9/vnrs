//! Python bindings for Alpha research module
//!
//! Provides PyO3 wrappers for the Alpha module: models, datasets, factor analysis.
//! Connected to the real Rust AlphaLab engine, ML models, and backtesting.

use crate::alpha::dataset::{AlphaDataset, Segment};
use crate::alpha::lab::AlphaLab;
use crate::alpha::model::{
    AlphaModel, GradientBoostingModel, LinearRegressionModel, RandomForestModel,
};
use pyo3::prelude::*;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enum types
// ---------------------------------------------------------------------------

/// Python-facing model type enumeration
#[pyclass]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyAlphaModelType {
    LinearRegression,
    Ridge,
    Lasso,
    RandomForest,
    GradientBoosting,
}

#[pymethods]
impl PyAlphaModelType {
    #[classattr]
    const LINEAR_REGRESSION: &str = "LinearRegression";
    #[classattr]
    const RIDGE: &str = "Ridge";
    #[classattr]
    const LASSO: &str = "Lasso";
    #[classattr]
    const RANDOM_FOREST: &str = "RandomForest";
    #[classattr]
    const GRADIENT_BOOSTING: &str = "GradientBoosting";
}

/// Python-facing data segment enumeration
#[pyclass]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PySegment {
    Train,
    Valid,
    Test,
}

#[pymethods]
impl PySegment {
    #[classattr]
    const TRAIN: &str = "Train";
    #[classattr]
    const VALID: &str = "Valid";
    #[classattr]
    const TEST: &str = "Test";
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Python wrapper for Alpha model training result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyAlphaTrainResult {
    #[pyo3(get)]
    pub model_name: String,
    #[pyo3(get)]
    pub n_features: usize,
    #[pyo3(get)]
    pub n_samples: usize,
    #[pyo3(get)]
    pub train_mse: f64,
    #[pyo3(get)]
    pub valid_mse: f64,
    #[pyo3(get)]
    pub detail: String,
}

#[pymethods]
impl PyAlphaTrainResult {
    fn __repr__(&self) -> String {
        format!(
            "AlphaTrainResult(model='{}', features={}, samples={}, train_mse={:.6}, valid_mse={:.6})",
            self.model_name, self.n_features, self.n_samples, self.train_mse, self.valid_mse
        )
    }
}

/// Python wrapper for Alpha prediction result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyAlphaPrediction {
    #[pyo3(get)]
    pub segment: String,
    #[pyo3(get)]
    pub predictions: Vec<f64>,
    #[pyo3(get)]
    pub mean: f64,
    #[pyo3(get)]
    pub std: f64,
}

#[pymethods]
impl PyAlphaPrediction {
    fn __repr__(&self) -> String {
        format!(
            "AlphaPrediction(segment='{}', len={}, mean={:.6}, std={:.6})",
            self.segment, self.predictions.len(), self.mean, self.std
        )
    }
}

/// Python wrapper for factor analysis result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyFactorAnalysisResult {
    #[pyo3(get)]
    pub factor_name: String,
    #[pyo3(get)]
    pub values: Vec<f64>,
    #[pyo3(get)]
    pub mean: f64,
    #[pyo3(get)]
    pub std: f64,
    #[pyo3(get)]
    pub min: f64,
    #[pyo3(get)]
    pub max: f64,
    #[pyo3(get)]
    pub ic: f64,
}

#[pymethods]
impl PyFactorAnalysisResult {
    fn __repr__(&self) -> String {
        format!(
            "FactorAnalysis(factor='{}', mean={:.6}, std={:.6}, ic={:.6})",
            self.factor_name, self.mean, self.std, self.ic
        )
    }
}

/// Python wrapper for cross-section analysis result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyCrossSectionResult {
    #[pyo3(get)]
    pub date: String,
    #[pyo3(get)]
    pub factor_name: String,
    #[pyo3(get)]
    pub symbols: Vec<String>,
    #[pyo3(get)]
    pub values: Vec<f64>,
    #[pyo3(get)]
    pub mean: f64,
    #[pyo3(get)]
    pub std: f64,
}

#[pymethods]
impl PyCrossSectionResult {
    fn __repr__(&self) -> String {
        format!(
            "CrossSection(date='{}', factor='{}', n_symbols={}, mean={:.6})",
            self.date, self.factor_name, self.symbols.len(), self.mean
        )
    }
}

/// Python wrapper for backtest result from factor signals
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyBacktestResult {
    #[pyo3(get)]
    pub total_return: f64,
    #[pyo3(get)]
    pub sharpe_ratio: f64,
    #[pyo3(get)]
    pub max_drawdown: f64,
    #[pyo3(get)]
    pub trade_count: f64,
    #[pyo3(get)]
    pub win_rate: f64,
    #[pyo3(get)]
    pub detail: String,
}

#[pymethods]
impl PyBacktestResult {
    fn __repr__(&self) -> String {
        format!(
            "BacktestResult(return={:.4}%, sharpe={:.4}, max_dd={:.4}%)",
            self.total_return * 100.0,
            self.sharpe_ratio,
            self.max_drawdown * 100.0
        )
    }
}

// ---------------------------------------------------------------------------
// Helper: build a temporary AlphaDataset from X/y arrays
// ---------------------------------------------------------------------------

/// Build a temporary AlphaDataset from 2D feature array X and 1D label array y.
///
/// The dataset is split into train/valid/test using an 80/10/10 ratio.
/// Feature columns are named f0, f1, ..., fN. Label column is "label".
fn build_dataset_from_arrays(x: &[Vec<f64>], y: &[f64]) -> Result<AlphaDataset, String> {
    use polars::prelude::*;

    if x.is_empty() {
        return Err("Feature array X is empty".to_string());
    }
    if x.len() != y.len() {
        return Err(format!(
            "X and y length mismatch: X={}, y={}",
            x.len(),
            y.len()
        ));
    }

    let n_features = x[0].len();
    let n_rows = x.len();

    // Build columns: datetime (placeholder), vt_symbol (placeholder), features, label
    let mut columns: Vec<Column> = Vec::new();

    // Placeholder datetime column (sequential integers representing timestamps)
    let datetimes: Vec<i64> = (0..n_rows).map(|i| i as i64 * 86_400_000).collect();
    columns.push(Column::new("datetime".into(), datetimes));

    // Placeholder vt_symbol column
    let symbols: Vec<String> = (0..n_rows).map(|i| format!("SYM{}.BINANCE", i % 10)).collect();
    columns.push(Column::new("vt_symbol".into(), symbols));

    // Feature columns: f0, f1, ..., fN
    for j in 0..n_features {
        let col_name = format!("f{}", j);
        let values: Vec<f64> = x.iter().map(|row| row.get(j).copied().unwrap_or(0.0)).collect();
        columns.push(Column::new(col_name.into(), values));
    }

    // Label column
    columns.push(Column::new("label".into(), y.to_vec()));

    let df = DataFrame::new(columns).map_err(|e| format!("Failed to create DataFrame: {}", e))?;

    // Split into 80/10/10
    let train_end = (n_rows as f64 * 0.8) as i64;
    let valid_end = (n_rows as f64 * 0.9) as i64;

    let train_period = (
        format!("{:08}", 0),
        format!("{:08}", train_end),
    );
    let valid_period = (
        format!("{:08}", train_end + 1),
        format!("{:08}", valid_end),
    );
    let test_period = (
        format!("{:08}", valid_end + 1),
        format!("{:08}", n_rows as i64),
    );

    let mut dataset = AlphaDataset::new(df, train_period, valid_period, test_period);

    // Add feature expressions for each feature column
    for j in 0..n_features {
        let col_name = format!("f{}", j);
        dataset.add_feature(col_name.clone(), col_name);
    }
    dataset.set_label("label".to_string());

    // Prepare the data (computes raw_df, infer_df, learn_df)
    dataset
        .prepare_data(None)
        .map_err(|e| format!("Failed to prepare dataset: {}", e))?;

    Ok(dataset)
}

// ---------------------------------------------------------------------------
// Helper: compute MSE
// ---------------------------------------------------------------------------

fn compute_mse(actual: &[f64], predicted: &[f64]) -> f64 {
    if actual.len() != predicted.len() || actual.is_empty() {
        return f64::NAN;
    }
    let n = actual.len() as f64;
    actual
        .iter()
        .zip(predicted.iter())
        .map(|(a, p)| (a - p).powi(2))
        .sum::<f64>()
        / n
}

// ---------------------------------------------------------------------------
// Helper: compute Information Coefficient (rank correlation)
// ---------------------------------------------------------------------------

fn compute_ic(actual: &[f64], predicted: &[f64]) -> f64 {
    if actual.len() != predicted.len() || actual.len() < 2 {
        return f64::NAN;
    }
    let n = actual.len();

    // Compute ranks
    let rank = |data: &[f64]| -> Vec<f64> {
        let mut indexed: Vec<(usize, f64)> = data.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut ranks = vec![0.0f64; n];
        let mut i = 0;
        while i < indexed.len() {
            let mut j = i + 1;
            while j < indexed.len()
                && (indexed[j].1 - indexed[i].1).abs() < 1e-12
            {
                j += 1;
            }
            let avg_rank = ((i + j - 1) as f64) / 2.0 + 1.0;
            for k in i..j {
                ranks[indexed[k].0] = avg_rank;
            }
            i = j;
        }
        ranks
    };

    let ranks_a = rank(actual);
    let ranks_p = rank(predicted);

    let mean_a: f64 = ranks_a.iter().sum::<f64>() / n as f64;
    let mean_p: f64 = ranks_p.iter().sum::<f64>() / n as f64;

    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_p = 0.0;
    for i in 0..n {
        let da = ranks_a[i] - mean_a;
        let dp = ranks_p[i] - mean_p;
        cov += da * dp;
        var_a += da * da;
        var_p += dp * dp;
    }

    let denom = (var_a * var_p).sqrt();
    if denom < 1e-12 {
        return 0.0;
    }
    cov / denom
}

// ---------------------------------------------------------------------------
// PyAlphaModel — Python-facing ML model wrapper
// ---------------------------------------------------------------------------

/// Python-facing Alpha ML model.
///
/// Wraps a Rust AlphaModel implementation. Supports `fit(X, y)` and `predict(X)`.
/// Available model types: LinearRegression, Ridge, Lasso, RandomForest, GradientBoosting.
///
/// Ridge and Lasso use LinearRegressionModel internally (regularization not yet
/// implemented as separate model types).
/// GradientBoosting corresponds to XGBoost-style boosting.
#[pyclass]
pub struct PyAlphaModel {
    model_type: PyAlphaModelType,
    model: Option<Box<dyn AlphaModel>>,
    /// Stored training dataset for segment-based predict
    dataset: Option<AlphaDataset>,
    /// Model hyperparameters stored as string key-value pairs
    params: HashMap<String, String>,
}

#[pymethods]
impl PyAlphaModel {
    /// Create a new PyAlphaModel.
    ///
    /// Args:
    ///     model_type: One of "LinearRegression", "Ridge", "Lasso", "RandomForest", "GradientBoosting"
    ///     n_estimators: Number of trees (for RandomForest/GradientBoosting), default 100
    ///     max_depth: Maximum tree depth (for RandomForest/GradientBoosting), default None
    ///     learning_rate: Learning rate (for GradientBoosting), default 0.1
    #[new]
    #[pyo3(signature = (model_type, n_estimators=100, max_depth=None, learning_rate=0.1))]
    fn new(
        model_type: &str,
        n_estimators: usize,
        max_depth: Option<usize>,
        learning_rate: f64,
    ) -> PyResult<Self> {
        let mt = match model_type {
            "LinearRegression" => PyAlphaModelType::LinearRegression,
            "Ridge" => PyAlphaModelType::Ridge,
            "Lasso" => PyAlphaModelType::Lasso,
            "RandomForest" => PyAlphaModelType::RandomForest,
            "GradientBoosting" => PyAlphaModelType::GradientBoosting,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unknown model_type '{}'. Must be one of: LinearRegression, Ridge, Lasso, RandomForest, GradientBoosting",
                    model_type
                )));
            }
        };

        let mut params = HashMap::new();
        params.insert("model_type".to_string(), model_type.to_string());
        params.insert("n_estimators".to_string(), n_estimators.to_string());
        params.insert("max_depth".to_string(), format!("{:?}", max_depth));
        params.insert("learning_rate".to_string(), learning_rate.to_string());

        Ok(PyAlphaModel {
            model_type: mt,
            model: None,
            dataset: None,
            params,
        })
    }

    /// Fit the model using feature matrix X and label vector y.
    ///
    /// Args:
    ///     X: 2D list of floats (n_samples x n_features)
    ///     y: 1D list of floats (n_samples)
    ///
    /// Returns:
    ///     self (for method chaining)
    fn fit(&mut self, x: Vec<Vec<f64>>, y: Vec<f64>) -> PyResult<Self> {
        let dataset = build_dataset_from_arrays(&x, &y)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        // Create the appropriate Rust model
        let mut model: Box<dyn AlphaModel> = match self.model_type {
            PyAlphaModelType::LinearRegression
            | PyAlphaModelType::Ridge
            | PyAlphaModelType::Lasso => Box::new(LinearRegressionModel::new()),
            PyAlphaModelType::RandomForest => {
                let n_estimators: usize = self
                    .params
                    .get("n_estimators")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(100);
                let max_depth: Option<usize> = match self.params.get("max_depth") {
                    Some(v) if v != "None" => v.parse().ok(),
                    _ => None,
                };
                Box::new(RandomForestModel::with_params(
                    n_estimators,
                    max_depth,
                    2,
                    42,
                ))
            }
            PyAlphaModelType::GradientBoosting => {
                let n_estimators: usize = self
                    .params
                    .get("n_estimators")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(100);
                let learning_rate: f64 = self
                    .params
                    .get("learning_rate")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.1);
                Box::new(GradientBoostingModel::new(n_estimators, learning_rate))
            }
        };

        model.fit(&dataset);

        self.model = Some(model);
        self.dataset = Some(dataset);

        Ok(PyAlphaModel {
            model_type: self.model_type,
            model: self.model.take(),
            dataset: self.dataset.take(),
            params: self.params.clone(),
        })
    }

    /// Make predictions using the trained model.
    ///
    /// Args:
    ///     x: 2D list of floats (n_samples x n_features)
    ///
    /// Returns:
    ///     1D list of float predictions
    fn predict(&self, x: Vec<Vec<f64>>) -> PyResult<Vec<f64>> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Model not fitted yet. Call fit() first."))?;

        // Build a dummy dataset with zeros as labels for prediction
        let y_dummy = vec![0.0; x.len()];
        let dataset = build_dataset_from_arrays(&x, &y_dummy)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        // If we have a stored training dataset, predict on the training segment
        // Otherwise predict on the full dataset (test segment)
        let segment = if self.dataset.is_some() {
            Segment::Train
        } else {
            Segment::Test
        };

        model
            .predict(&dataset, segment)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Prediction failed: {}", e)))
    }

    /// Get model detail string.
    fn detail(&self) -> String {
        match &self.model {
            Some(m) => m.detail(),
            None => "Model not fitted yet".to_string(),
        }
    }

    /// Get model name.
    fn name(&self) -> String {
        match &self.model {
            Some(m) => m.name().to_string(),
            None => format!("{:?}", self.model_type),
        }
    }
}

// ---------------------------------------------------------------------------
// PyAlphaModule — top-level Alpha research entry point
// ---------------------------------------------------------------------------

/// Python-facing Alpha research module.
///
/// Provides high-level methods for training ML models, factor analysis,
/// cross-section analysis, alpha weight computation, signal creation,
/// and factor-based backtesting — all wired to the real Rust alpha engine.
#[pyclass]
pub struct PyAlphaModule {
    lab: AlphaLab,
    /// Trained models stored by name
    models: HashMap<String, PyAlphaModel>,
}

#[pymethods]
impl PyAlphaModule {
    #[new]
    fn new() -> Self {
        PyAlphaModule {
            lab: AlphaLab::new(),
            models: HashMap::new(),
        }
    }

    /// Train an Alpha ML model.
    ///
    /// Args:
    ///     name: Model name for later reference
    ///     model_type: One of "LinearRegression", "Ridge", "Lasso", "RandomForest", "GradientBoosting"
    ///     x: 2D feature array (n_samples x n_features)
    ///     y: 1D label array (n_samples)
    ///     n_estimators: Number of trees (RandomForest/GradientBoosting), default 100
    ///     max_depth: Max tree depth (RandomForest/GradientBoosting), default None
    ///     learning_rate: Learning rate (GradientBoosting), default 0.1
    ///
    /// Returns:
    ///     PyAlphaTrainResult with training metrics
    #[pyo3(signature = (name, model_type, x, y, n_estimators=100, max_depth=None, learning_rate=0.1))]
    fn train(
        &mut self,
        name: String,
        model_type: &str,
        x: Vec<Vec<f64>>,
        y: Vec<f64>,
        n_estimators: usize,
        max_depth: Option<usize>,
        learning_rate: f64,
    ) -> PyResult<PyAlphaTrainResult> {
        let mut py_model = PyAlphaModel::new(model_type, n_estimators, max_depth, learning_rate)?;
        py_model = py_model.fit(x.clone(), y.clone())?;

        let model_name = py_model.name();
        let detail = py_model.detail();
        let n_features = x.first().map(|r| r.len()).unwrap_or(0);
        let n_samples = x.len();

        // Compute train MSE and valid MSE
        let train_mse = match py_model.predict(x.clone()) {
            Ok(preds) => compute_mse(&y, &preds),
            Err(_) => f64::NAN,
        };

        // Valid MSE: use a portion of data as validation (last 10%)
        let valid_count = (n_samples as f64 * 0.1) as usize;
        let valid_start = n_samples.saturating_sub(valid_count);
        let valid_x: Vec<Vec<f64>> = x[valid_start..].to_vec();
        let valid_y: Vec<f64> = y[valid_start..].to_vec();
        let valid_mse = if !valid_x.is_empty() {
            match py_model.predict(valid_x) {
                Ok(preds) => compute_mse(&valid_y, &preds),
                Err(_) => f64::NAN,
            }
        } else {
            f64::NAN
        };

        self.models.insert(name, py_model);

        Ok(PyAlphaTrainResult {
            model_name,
            n_features,
            n_samples,
            train_mse,
            valid_mse,
            detail,
        })
    }

    /// Make predictions using a trained model.
    ///
    /// Args:
    ///     name: Model name (as used in train())
    ///     x: 2D feature array
    ///     segment: Data segment ("Train", "Valid", or "Test")
    ///
    /// Returns:
    ///     PyAlphaPrediction with predictions and statistics
    fn predict(&self, name: &str, x: Vec<Vec<f64>>, segment: &str) -> PyResult<PyAlphaPrediction> {
        let py_model = self
            .models
            .get(name)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!(
                "Model '{}' not found. Train it first with train().",
                name
            )))?;

        let predictions = py_model.predict(x)?;

        let mean = if predictions.is_empty() {
            0.0
        } else {
            predictions.iter().sum::<f64>() / predictions.len() as f64
        };
        let std = if predictions.len() < 2 {
            0.0
        } else {
            let variance = predictions
                .iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / (predictions.len() - 1) as f64;
            variance.sqrt()
        };

        Ok(PyAlphaPrediction {
            segment: segment.to_string(),
            predictions,
            mean,
            std,
        })
    }

    /// Analyze a single factor.
    ///
    /// Computes descriptive statistics and Information Coefficient (IC)
    /// between the factor values and the label values.
    ///
    /// Args:
    ///     factor_name: Name of the factor
    ///     factor_values: 1D array of factor values
    ///     label_values: 1D array of corresponding label/return values
    ///
    /// Returns:
    ///     PyFactorAnalysisResult with statistics and IC
    fn analyze_factor(
        &self,
        factor_name: &str,
        factor_values: Vec<f64>,
        label_values: Vec<f64>,
    ) -> PyResult<PyFactorAnalysisResult> {
        if factor_values.len() != label_values.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "factor_values and label_values must have the same length",
            ));
        }

        let n = factor_values.len();
        if n == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Empty arrays provided",
            ));
        }

        let mean = factor_values.iter().sum::<f64>() / n as f64;
        let variance = if n < 2 {
            0.0
        } else {
            factor_values
                .iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / (n - 1) as f64
        };
        let std = variance.sqrt();
        let min = factor_values
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max = factor_values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);

        let ic = compute_ic(&factor_values, &label_values);

        Ok(PyFactorAnalysisResult {
            factor_name: factor_name.to_string(),
            values: factor_values,
            mean,
            std,
            min,
            max,
            ic,
        })
    }

    /// Perform cross-section analysis for a given date and factor.
    ///
    /// Groups factor values by symbol and computes cross-sectional statistics.
    ///
    /// Args:
    ///     date: Date string (e.g., "2024-01-15")
    ///     factor_name: Name of the factor
    ///     symbols: List of symbol identifiers
    ///     values: Factor values corresponding to each symbol
    ///
    /// Returns:
    ///     PyCrossSectionResult with cross-sectional statistics
    fn cross_section_analysis(
        &self,
        date: &str,
        factor_name: &str,
        symbols: Vec<String>,
        values: Vec<f64>,
    ) -> PyResult<PyCrossSectionResult> {
        if symbols.len() != values.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "symbols and values must have the same length",
            ));
        }

        let n = values.len();
        let mean = if n == 0 {
            0.0
        } else {
            values.iter().sum::<f64>() / n as f64
        };
        let std = if n < 2 {
            0.0
        } else {
            let variance = values
                .iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / (n - 1) as f64;
            variance.sqrt()
        };

        Ok(PyCrossSectionResult {
            date: date.to_string(),
            factor_name: factor_name.to_string(),
            symbols,
            values,
            mean,
            std,
        })
    }

    /// Compute alpha combination weights based on factor IC values.
    ///
    /// Uses IC-weighted scheme: each factor's weight is proportional to its
    /// absolute IC value, normalized to sum to 1.0.
    ///
    /// Args:
    ///     factor_names: List of factor names
    ///     ic_values: Corresponding IC values for each factor
    ///
    /// Returns:
    ///     Dict mapping factor name to its weight
    fn compute_alpha_weights(
        &self,
        factor_names: Vec<String>,
        ic_values: Vec<f64>,
    ) -> PyResult<HashMap<String, f64>> {
        if factor_names.len() != ic_values.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "factor_names and ic_values must have the same length",
            ));
        }

        let abs_ics: Vec<f64> = ic_values.iter().map(|v| v.abs()).collect();
        let total: f64 = abs_ics.iter().sum();

        let weights: HashMap<String, f64> = if total < 1e-12 {
            // Equal weights if all ICs are near zero
            let uniform = 1.0 / factor_names.len() as f64;
            factor_names
                .iter()
                .map(|name| (name.clone(), uniform))
                .collect()
        } else {
            factor_names
                .iter()
                .zip(abs_ics.iter())
                .map(|(name, &abs_ic)| (name.clone(), abs_ic / total))
                .collect()
        };

        Ok(weights)
    }

    /// Create a trading signal from model predictions.
    ///
    /// Converts continuous predictions to discrete signals:
    /// - prediction > 0 → signal = 1 (long)
    /// - prediction < 0 → signal = -1 (short)
    /// - prediction == 0 → signal = 0 (flat)
    ///
    /// Args:
    ///     model_name: Name of the trained model
    ///     x: 2D feature array
    ///
    /// Returns:
    ///     1D list of integer signals (-1, 0, or 1)
    fn create_signal_from_model(
        &self,
        model_name: &str,
        x: Vec<Vec<f64>>,
    ) -> PyResult<Vec<i32>> {
        let prediction = self.predict(model_name, x, "Signal")?;
        let signals: Vec<i32> = prediction
            .predictions
            .iter()
            .map(|&v| {
                if v > 0.0 {
                    1
                } else if v < 0.0 {
                    -1
                } else {
                    0
                }
            })
            .collect();
        Ok(signals)
    }

    /// Run backtest using factor-based signals.
    ///
    /// Creates a simple backtest that trades based on factor signals:
    /// positive signal → long, negative signal → short.
    ///
    /// Args:
    ///     predictions: 1D array of model predictions (used as signals)
    ///     prices: 1D array of corresponding prices
    ///     capital: Starting capital, default 1,000,000
    ///
    /// Returns:
    ///     PyBacktestResult with backtest statistics
    #[pyo3(signature = (predictions, prices, capital=1_000_000.0))]
    fn run_backtest_with_factors(
        &self,
        predictions: Vec<f64>,
        prices: Vec<f64>,
        capital: f64,
    ) -> PyResult<PyBacktestResult> {
        if predictions.len() != prices.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "predictions and prices must have the same length",
            ));
        }
        if prices.len() < 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Need at least 2 price points for backtesting",
            ));
        }

        let n = predictions.len();
        let mut equity = capital;
        let mut position: f64 = 0.0;
        let mut entry_price: f64 = 0.0;
        let mut trade_count: u32 = 0;
        let mut wins: u32 = 0;
        let mut total_trades: u32 = 0;
        let mut peak_equity = capital;
        let mut max_drawdown = 0.0_f64;
        let mut daily_returns: Vec<f64> = Vec::new();

        for i in 1..n {
            let signal: f64 = if predictions[i] > 0.0 {
                1.0
            } else if predictions[i] < 0.0 {
                -1.0
            } else {
                0.0
            };

            let prev_signal: f64 = if predictions[i - 1] > 0.0 {
                1.0
            } else if predictions[i - 1] < 0.0 {
                -1.0
            } else {
                0.0
            };

            // Signal change → close old position, open new one
            if (signal - prev_signal).abs() > 1e-10 {
                // Close existing position
                if position.abs() > 1e-10 {
                    let pnl = position * (prices[i - 1] - entry_price);
                    equity += pnl;
                    total_trades += 1;
                    if pnl > 0.0 {
                        wins += 1;
                    }
                    position = 0.0;
                }
                // Open new position
                if signal.abs() > 1e-10 {
                    position = signal;
                    entry_price = prices[i - 1];
                    trade_count += 1;
                }
            }

            // Mark-to-market PnL for current position
            let mtm_pnl = position * (prices[i] - prices[i - 1]);
            let current_equity = equity + mtm_pnl;

            if current_equity > peak_equity {
                peak_equity = current_equity;
            }
            let dd = (current_equity - peak_equity) / peak_equity;
            if dd < max_drawdown {
                max_drawdown = dd;
            }

            let daily_return = mtm_pnl / capital;
            daily_returns.push(daily_return);
        }

        // Close final position
        if position.abs() > 1e-10 && n > 0 {
            let pnl = position * (prices[n - 1] - entry_price);
            equity += pnl;
            total_trades += 1;
            if pnl > 0.0 {
                wins += 1;
            }
        }

        let total_return = (equity - capital) / capital;
        let win_rate = if total_trades > 0 {
            wins as f64 / total_trades as f64
        } else {
            0.0
        };

        // Sharpe ratio (annualized, assuming 252 trading days)
        let sharpe_ratio = if daily_returns.len() >= 2 {
            let mean_ret = daily_returns.iter().sum::<f64>() / daily_returns.len() as f64;
            let variance = daily_returns
                .iter()
                .map(|r| (r - mean_ret).powi(2))
                .sum::<f64>()
                / (daily_returns.len() - 1) as f64;
            let std_ret = variance.sqrt();
            if std_ret > 1e-12 {
                (mean_ret / std_ret) * (252.0_f64).sqrt()
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(PyBacktestResult {
            total_return,
            sharpe_ratio,
            max_drawdown,
            trade_count: trade_count as f64,
            win_rate,
            detail: format!(
                "Factor backtest: {} trades, return={:.4}%, sharpe={:.4}, max_dd={:.4}%",
                trade_count,
                total_return * 100.0,
                sharpe_ratio,
                max_drawdown * 100.0
            ),
        })
    }

    /// List all trained model names.
    fn list_models(&self) -> Vec<String> {
        self.models.keys().cloned().collect()
    }

    /// List all datasets in the AlphaLab.
    fn list_datasets(&self) -> Vec<String> {
        self.lab.list_all_datasets()
    }
}

impl Default for PyAlphaModule {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register the alpha research submodule with the parent Python module.
pub fn register_alpha_module(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent_module.py(), "alpha")?;

    m.add_class::<PyAlphaModelType>()?;
    m.add_class::<PySegment>()?;
    m.add_class::<PyAlphaTrainResult>()?;
    m.add_class::<PyAlphaPrediction>()?;
    m.add_class::<PyFactorAnalysisResult>()?;
    m.add_class::<PyCrossSectionResult>()?;
    m.add_class::<PyBacktestResult>()?;
    m.add_class::<PyAlphaModel>()?;
    m.add_class::<PyAlphaModule>()?;

    parent_module.add_submodule(&m)?;
    Ok(())
}
