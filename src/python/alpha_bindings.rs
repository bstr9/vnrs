//! Python bindings for Alpha research module
//!
//! Provides PyO3 stubs for the Alpha module: models, datasets, factor analysis.

use pyo3::prelude::*;

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

/// Python stub for Alpha model training result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyAlphaTrainResult {
    /// Model name
    #[pyo3(get)]
    pub model_name: String,
    /// Number of features used
    #[pyo3(get)]
    pub n_features: usize,
    /// Number of training samples
    #[pyo3(get)]
    pub n_samples: usize,
    /// Training MSE (mean squared error)
    #[pyo3(get)]
    pub train_mse: f64,
    /// Validation MSE
    #[pyo3(get)]
    pub valid_mse: f64,
    /// Model detail string
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

/// Python stub for Alpha prediction result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyAlphaPrediction {
    /// Segment used for prediction
    #[pyo3(get)]
    pub segment: String,
    /// Prediction values
    #[pyo3(get)]
    pub predictions: Vec<f64>,
    /// Mean of predictions
    #[pyo3(get)]
    pub mean: f64,
    /// Std of predictions
    #[pyo3(get)]
    pub std: f64,
}

#[pymethods]
impl PyAlphaPrediction {
    fn __repr__(&self) -> String {
        format!(
            "AlphaPrediction(segment='{}', len={}, mean={:.6}, std={:.6})",
            self.segment,
            self.predictions.len(),
            self.mean,
            self.std
        )
    }
}

/// Python stub for factor analysis result
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyFactorAnalysisResult {
    /// Factor name
    #[pyo3(get)]
    pub factor_name: String,
    /// Factor values (sample)
    #[pyo3(get)]
    pub values: Vec<f64>,
    /// Mean of factor values
    #[pyo3(get)]
    pub mean: f64,
    /// Std of factor values
    #[pyo3(get)]
    pub std: f64,
    /// Min value
    #[pyo3(get)]
    pub min: f64,
    /// Max value
    #[pyo3(get)]
    pub max: f64,
    /// IC (information coefficient) against label
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

/// Python wrapper for Alpha research module.
///
/// Provides access to model training, prediction, factor analysis,
/// and dataset management for quantitative research workflows.
///
/// This is currently a **stub implementation** that defines the API surface.
/// Full functionality will be connected to the Rust AlphaLab engine in a
/// future release.
#[pyclass]
pub struct PyAlphaModule {
    /// Model type for training
    model_type: String,
    /// Dataset name
    dataset_name: String,
    /// Trained model info (stub)
    trained: bool,
    /// Training result
    last_train_result: Option<PyAlphaTrainResult>,
    /// Available datasets
    datasets: Vec<String>,
    /// Available factors
    factors: Vec<String>,
}

#[pymethods]
impl PyAlphaModule {
    #[new]
    #[pyo3(signature = (model_type="LinearRegression".to_string()))]
    fn new(model_type: String) -> PyResult<Self> {
        let valid_models = [
            "LinearRegression",
            "Ridge",
            "Lasso",
            "RandomForest",
            "GradientBoosting",
        ];
        if !valid_models.contains(&model_type.as_str()) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid model_type '{}'. Must be one of: {:?}",
                model_type, valid_models
            )));
        }

        Ok(PyAlphaModule {
            model_type,
            dataset_name: String::new(),
            trained: false,
            last_train_result: None,
            datasets: Vec::new(),
            factors: Vec::new(),
        })
    }

    /// Get the model type
    #[getter]
    fn model_type(&self) -> &str {
        &self.model_type
    }

    /// Set the model type
    #[setter]
    fn set_model_type(&mut self, value: String) -> PyResult<()> {
        let valid_models = [
            "LinearRegression",
            "Ridge",
            "Lasso",
            "RandomForest",
            "GradientBoosting",
        ];
        if !valid_models.contains(&value.as_str()) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid model_type '{}'. Must be one of: {:?}",
                value, valid_models
            )));
        }
        self.model_type = value;
        self.trained = false;
        Ok(())
    }

    /// Whether a model has been trained
    #[getter]
    fn is_trained(&self) -> bool {
        self.trained
    }

    /// List available datasets
    fn list_datasets(&self) -> Vec<String> {
        self.datasets.clone()
    }

    /// List available factors
    fn list_factors(&self) -> Vec<String> {
        self.factors.clone()
    }

    /// Set the active dataset by name
    fn set_dataset(&mut self, name: String) {
        self.dataset_name = name;
    }

    /// Get the active dataset name
    fn get_dataset(&self) -> &str {
        &self.dataset_name
    }

    /// Train the model on the active dataset.
    ///
    /// Args:
    ///     hyperparams: Optional dict of hyperparameters
    ///
    /// Returns:
    ///     PyAlphaTrainResult with training metrics
    ///
    /// Note: This is currently a stub. Full implementation pending.
    #[pyo3(signature = (hyperparams=None))]
    fn train(
        &mut self,
        hyperparams: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<PyAlphaTrainResult> {
        // Log hyperparams if provided (stub: not used yet)
        if let Some(params) = hyperparams {
            let _param_count = params.len();
        }

        // Stub: return placeholder training result
        let result = PyAlphaTrainResult {
            model_name: self.model_type.clone(),
            n_features: 0,
            n_samples: 0,
            train_mse: 0.0,
            valid_mse: 0.0,
            detail: format!(
                "Stub: {} model training not yet connected to AlphaLab",
                self.model_type
            ),
        };
        self.trained = true;
        self.last_train_result = Some(result.clone());
        Ok(result)
    }

    /// Make predictions using the trained model.
    ///
    /// Args:
    ///     segment: Data segment - "Train", "Valid", or "Test"
    ///
    /// Returns:
    ///     PyAlphaPrediction with prediction results
    ///
    /// Note: This is currently a stub. Full implementation pending.
    fn predict(&self, segment: &str) -> PyResult<PyAlphaPrediction> {
        if !self.trained {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Model has not been trained yet. Call train() first.",
            ));
        }

        let valid_segments = ["Train", "Valid", "Test"];
        if !valid_segments.contains(&segment) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid segment '{}'. Must be one of: {:?}",
                segment, valid_segments
            )));
        }

        // Stub: return empty predictions
        Ok(PyAlphaPrediction {
            segment: segment.to_string(),
            predictions: Vec::new(),
            mean: 0.0,
            std: 0.0,
        })
    }

    /// Analyze a factor's distribution and IC.
    ///
    /// Args:
    ///     factor_name: Name of the factor to analyze
    ///     segment: Data segment - "Train", "Valid", or "Test"
    ///
    /// Returns:
    ///     PyFactorAnalysisResult with factor statistics
    ///
    /// Note: This is currently a stub. Full implementation pending.
    #[pyo3(signature = (factor_name, segment="Train"))]
    fn analyze_factor(&self, factor_name: &str, segment: &str) -> PyResult<PyFactorAnalysisResult> {
        let valid_segments = ["Train", "Valid", "Test"];
        if !valid_segments.contains(&segment) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid segment '{}'. Must be one of: {:?}",
                segment, valid_segments
            )));
        }

        // Stub: return placeholder factor analysis
        Ok(PyFactorAnalysisResult {
            factor_name: factor_name.to_string(),
            values: Vec::new(),
            mean: 0.0,
            std: 0.0,
            min: 0.0,
            max: 0.0,
            ic: 0.0,
        })
    }

    /// Get model detail string.
    ///
    /// Returns the detail string from the last trained model,
    /// or a placeholder if no model has been trained.
    fn detail(&self) -> String {
        match &self.last_train_result {
            Some(r) => r.detail.clone(),
            None => format!("{} model (not trained)", self.model_type),
        }
    }

    /// Get available model types.
    #[staticmethod]
    fn available_models() -> Vec<String> {
        vec![
            "LinearRegression".to_string(),
            "Ridge".to_string(),
            "Lasso".to_string(),
            "RandomForest".to_string(),
            "GradientBoosting".to_string(),
        ]
    }

    fn __repr__(&self) -> String {
        format!(
            "AlphaModule(model_type='{}', trained={}, dataset='{}')",
            self.model_type, self.trained, self.dataset_name
        )
    }
}

/// Register the alpha module as a sub-module of trade_engine
pub fn register_alpha_module(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "alpha")?;
    m.add_class::<PyAlphaModule>()?;
    m.add_class::<PyAlphaModelType>()?;
    m.add_class::<PySegment>()?;
    m.add_class::<PyAlphaTrainResult>()?;
    m.add_class::<PyAlphaPrediction>()?;
    m.add_class::<PyFactorAnalysisResult>()?;

    // Module-level convenience constants
    m.add("LINEAR_REGRESSION", "LinearRegression")?;
    m.add("RIDGE", "Ridge")?;
    m.add("LASSO", "Lasso")?;
    m.add("RANDOM_FOREST", "RandomForest")?;
    m.add("GRADIENT_BOOSTING", "GradientBoosting")?;
    m.add("SEGMENT_TRAIN", "Train")?;
    m.add("SEGMENT_VALID", "Valid")?;
    m.add("SEGMENT_TEST", "Test")?;

    parent.add_submodule(&m)?;
    Ok(())
}
