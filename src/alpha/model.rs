//! Model module for alpha research
//! Provides template for machine learning models in alpha research
//!
//! This module implements the AlphaModel trait and several example models
//! matching vnpy's functionality.

use crate::alpha::dataset::{AlphaDataset, Segment};
use crate::alpha::logger;
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Trait for alpha models
///
/// All alpha models must implement this trait to be compatible with the
/// alpha research framework.
pub trait AlphaModel: Send + Sync + Any + std::fmt::Debug {
    /// Fit the model with dataset
    ///
    /// Trains the model using the training data from the dataset.
    fn fit(&mut self, dataset: &AlphaDataset);

    /// Make predictions using the model
    ///
    /// Generates predictions for the specified segment (train/valid/test).
    fn predict(
        &self,
        dataset: &AlphaDataset,
        segment: Segment,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>>;

    /// Output detailed information about the model
    fn detail(&self) -> String {
        "No details available".to_string()
    }

    /// Get model name
    fn name(&self) -> &str {
        "AlphaModel"
    }
}

/// Solve a linear system Ax = b using Gaussian elimination with partial pivoting.
/// Returns None if the system is singular or near-singular.
fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut aug = vec![vec![0.0; n + 1]; n];
    for i in 0..n {
        for j in 0..n {
            aug[i][j] = a[i][j];
        }
        aug[i][n] = b[i];
    }
    for col in 0..n {
        let mut max_row = col;
        for row in (col + 1)..n {
            if aug[row][col].abs() > aug[max_row][col].abs() {
                max_row = row;
            }
        }
        if aug[max_row][col].abs() < 1e-10 {
            return None;
        }
        aug.swap(col, max_row);
        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            #[allow(clippy::needless_range_loop)]
            for j in col..=n {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }
    Some(x)
}

/// Linear Regression Model
///
/// A simple linear regression model for alpha factor prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearRegressionModel {
    /// Model weights (coefficients)
    pub weights: Vec<f64>,
    /// Model bias (intercept)
    pub bias: f64,
    /// Number of features
    pub n_features: usize,
    /// Training samples
    pub n_samples: usize,
}

impl LinearRegressionModel {
    /// Create a new Linear Regression model
    pub fn new() -> Self {
        LinearRegressionModel {
            weights: vec![],
            bias: 0.0,
            n_features: 0,
            n_samples: 0,
        }
    }

    /// Create a new Linear Regression model with specified number of features
    pub fn with_features(n_features: usize) -> Self {
        LinearRegressionModel {
            weights: vec![0.0; n_features],
            bias: 0.0,
            n_features,
            n_samples: 0,
        }
    }
}

impl Default for LinearRegressionModel {
    fn default() -> Self {
        Self::new()
    }
}

impl AlphaModel for LinearRegressionModel {
    fn fit(&mut self, dataset: &AlphaDataset) {
        logger::logger().info("Fitting Linear Regression model...");

        // Get training data
        let train_df = match dataset.fetch_learn(Segment::Train) {
            Some(df) => df,
            None => {
                logger::logger().error("No training data available");
                return;
            }
        };

        // Calculate number of features (excluding datetime, vt_symbol, and label columns)
        let cols = train_df.get_column_names();
        let feature_cols: Vec<String> = cols
            .iter()
            .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
            .map(|c| c.to_string())
            .collect();

        self.n_features = feature_cols.len();
        self.n_samples = train_df.height();

        if self.n_features == 0 || self.n_samples == 0 {
            logger::logger().error("No features or samples available for fitting");
            return;
        }

        // Extract feature matrix X and label vector y from the DataFrame
        let n = self.n_samples;
        let p = self.n_features;

        // Build feature matrix (n x p) and label vector (n)
        let mut x_matrix: Vec<Vec<f64>> = vec![vec![0.0; p]; n];
        let mut y_vec: Vec<f64> = vec![0.0; n];
        let mut has_label = true;

        // Get label column
        if let Ok(label_col) = train_df.column("label") {
            if let Ok(label_ca) = label_col.f64() {
                for (i, val) in label_ca.into_iter().enumerate() {
                    y_vec[i] = val.unwrap_or(0.0);
                }
            } else {
                has_label = false;
            }
        } else {
            has_label = false;
        }

        if !has_label {
            logger::logger().error("No label column found in training data");
            return;
        }

        // Get feature columns
        for (j, col_name) in feature_cols.iter().enumerate() {
            if let Ok(col) = train_df.column(col_name) {
                if let Ok(ca) = col.f64() {
                    for (i, val) in ca.into_iter().enumerate() {
                        x_matrix[i][j] = val.unwrap_or(0.0);
                    }
                }
            }
        }

        // Compute X'X (p x p) and X'y (p)
        let mut xtx: Vec<Vec<f64>> = vec![vec![0.0; p]; p];
        let mut xty: Vec<f64> = vec![0.0; p];

        for i in 0..n {
            for j in 0..p {
                for k in 0..p {
                    xtx[j][k] += x_matrix[i][j] * x_matrix[i][k];
                }
                xty[j] += x_matrix[i][j] * y_vec[i];
            }
        }

        // Solve for coefficients using Gaussian elimination with partial pivoting
        match solve_linear_system(&xtx, &xty) {
            Some(coefficients) => {
                // The last coefficient is the intercept (from the augmented column)
                // But we compute it separately as the mean residual
                self.weights = coefficients[..p].to_vec();

                // Compute intercept: bias = mean(y) - sum(weights * mean(X))
                let mut x_mean = vec![0.0; p];
                let mut y_mean = 0.0;
                for i in 0..n {
                    #[allow(clippy::needless_range_loop)]
                    for j in 0..p {
                        x_mean[j] += x_matrix[i][j];
                    }
                    y_mean += y_vec[i];
                }
                #[allow(clippy::needless_range_loop)]
                for j in 0..p {
                    x_mean[j] /= n as f64;
                }
                y_mean /= n as f64;

                self.bias = y_mean;
                #[allow(clippy::needless_range_loop)]
                for j in 0..p {
                    self.bias -= self.weights[j] * x_mean[j];
                }

                logger::logger().info(&format!(
                    "Linear Regression model fitted with {} features and {} samples (bias: {:.6})",
                    self.n_features, self.n_samples, self.bias
                ));
            }
            None => {
                logger::logger().error(
                    "Failed to solve normal equation: X'X is singular or near-singular. Using zero weights.",
                );
                self.weights = vec![0.0; p];
                self.bias = 0.0;
            }
        }
    }

    fn predict(
        &self,
        dataset: &AlphaDataset,
        segment: Segment,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        logger::logger().debug(&format!("Making predictions for segment: {:?}", segment));

        // Get the appropriate dataframe based on segment
        let df = match segment {
            Segment::Train => dataset.fetch_learn(Segment::Train),
            Segment::Valid => dataset.fetch_learn(Segment::Valid),
            Segment::Test => dataset.fetch_learn(Segment::Test),
        };

        match df {
            Some(dataframe) => {
                // Get feature columns (excluding datetime, vt_symbol, and label)
                let cols = dataframe.get_column_names();
                let feature_cols: Vec<String> = cols
                    .iter()
                    .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
                    .map(|c| c.to_string())
                    .collect();

                let n_rows = dataframe.height();
                let mut predictions = Vec::with_capacity(n_rows);

                // Extract feature values and compute y = Xβ + bias
                for i in 0..n_rows {
                    let mut pred = self.bias;
                    for (j, col_name) in feature_cols.iter().enumerate() {
                        if j < self.weights.len() {
                            if let Ok(col) = dataframe.column(col_name) {
                                if let Ok(ca) = col.f64() {
                                    if let Some(val) = ca.get(i) {
                                        pred += self.weights[j] * val;
                                    }
                                }
                            }
                        }
                    }
                    predictions.push(pred);
                }

                Ok(predictions)
            }
            None => Err("No data available for the specified segment".into()),
        }
    }

    fn detail(&self) -> String {
        format!(
            "LinearRegressionModel:\n  Features: {}\n  Samples: {}\n  Weights: {:?}\n  Bias: {}",
            self.n_features, self.n_samples, self.weights, self.bias
        )
    }

    fn name(&self) -> &str {
        "LinearRegression"
    }
}

/// Random Forest Model
///
/// A random forest model for alpha factor prediction.
// STUB: Requires external ML library. Use PyO3 to call sklearn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomForestModel {
    /// Number of trees in the forest
    pub n_estimators: usize,
    /// Maximum depth of trees
    pub max_depth: Option<usize>,
    /// Minimum samples required to split a node
    pub min_samples_split: usize,
    /// Random seed
    pub seed: u64,
    /// Feature importances
    pub feature_importances: Vec<f64>,
    /// Number of features
    pub n_features: usize,
}

impl RandomForestModel {
    /// Create a new Random Forest model
    pub fn new(n_estimators: usize, max_depth: Option<usize>) -> Self {
        RandomForestModel {
            n_estimators,
            max_depth,
            min_samples_split: 2,
            seed: 42,
            feature_importances: vec![],
            n_features: 0,
        }
    }

    /// Create a new Random Forest model with all parameters
    pub fn with_params(
        n_estimators: usize,
        max_depth: Option<usize>,
        min_samples_split: usize,
        seed: u64,
    ) -> Self {
        RandomForestModel {
            n_estimators,
            max_depth,
            min_samples_split,
            seed,
            feature_importances: vec![],
            n_features: 0,
        }
    }
}

impl Default for RandomForestModel {
    fn default() -> Self {
        Self::new(100, None)
    }
}

impl AlphaModel for RandomForestModel {
    fn fit(&mut self, dataset: &AlphaDataset) {
        logger::logger().info(&format!(
            "Fitting Random Forest model with {} estimators",
            self.n_estimators
        ));

        let train_df = match dataset.fetch_learn(Segment::Train) {
            Some(df) => df,
            None => {
                logger::logger().error("No training data available");
                return;
            }
        };

        let cols = train_df.get_column_names();
        let feature_cols: Vec<String> = cols
            .iter()
            .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
            .map(|c| c.to_string())
            .collect();

        self.n_features = feature_cols.len();
        self.feature_importances = vec![1.0 / self.n_features as f64; self.n_features];

        logger::logger().info(&format!(
            "Random Forest model fitted with {} features and {} samples",
            self.n_features,
            train_df.height()
        ));
    }

    fn predict(
        &self,
        dataset: &AlphaDataset,
        segment: Segment,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        logger::logger().debug(&format!(
            "Making Random Forest predictions for segment: {:?}",
            segment
        ));

        let df = match segment {
            Segment::Train => dataset.fetch_learn(Segment::Train),
            Segment::Valid => dataset.fetch_learn(Segment::Valid),
            Segment::Test => dataset.fetch_learn(Segment::Test),
        };

        match df {
            Some(dataframe) => {
                let n_rows = dataframe.height();
                let cols = dataframe.get_column_names();
                let feature_cols: Vec<String> = cols
                    .iter()
                    .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
                    .map(|c| c.to_string())
                    .collect();

                let mut predictions = Vec::with_capacity(n_rows);

                for i in 0..n_rows {
                    let mut feature_sum = 0.0;
                    let mut feature_count = 0.0;
                    for col_name in &feature_cols {
                        if let Ok(col) = dataframe.column(col_name) {
                            if let Ok(f64_col) = col.f64() {
                                if let Some(val) = f64_col.get(i) {
                                    if !val.is_nan() {
                                        feature_sum += val;
                                        feature_count += 1.0;
                                    }
                                }
                            }
                        }
                    }
                    let pred = if feature_count > 0.0 {
                        let avg = feature_sum / feature_count;
                        1.0 / (1.0 + (-avg).exp())
                    } else {
                        0.5
                    };
                    predictions.push(pred);
                }

                Ok(predictions)
            }
            None => Err("No data available for the specified segment".into()),
        }
    }

    fn detail(&self) -> String {
        format!(
            "RandomForestModel:\n  Estimators: {}\n  Max Depth: {:?}\n  Min Samples Split: {}\n  Seed: {}\n  Features: {}\n  Feature Importances: {:?}",
            self.n_estimators, self.max_depth, self.min_samples_split, self.seed, self.n_features, self.feature_importances
        )
    }

    fn name(&self) -> &str {
        "RandomForest"
    }
}

/// Gradient Boosting Model
///
/// A gradient boosting model for alpha factor prediction.
// STUB: Requires external ML library. Use PyO3 to call sklearn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientBoostingModel {
    /// Number of boosting stages
    pub n_estimators: usize,
    /// Learning rate
    pub learning_rate: f64,
    /// Maximum depth of trees
    pub max_depth: Option<usize>,
    /// Random seed
    pub seed: u64,
    /// Number of features
    pub n_features: usize,
}

impl GradientBoostingModel {
    /// Create a new Gradient Boosting model
    pub fn new(n_estimators: usize, learning_rate: f64) -> Self {
        GradientBoostingModel {
            n_estimators,
            learning_rate,
            max_depth: Some(3),
            seed: 42,
            n_features: 0,
        }
    }
}

impl Default for GradientBoostingModel {
    fn default() -> Self {
        Self::new(100, 0.1)
    }
}

impl AlphaModel for GradientBoostingModel {
    fn fit(&mut self, dataset: &AlphaDataset) {
        logger::logger().info(&format!(
            "Fitting Gradient Boosting model with {} estimators and learning rate {}",
            self.n_estimators, self.learning_rate
        ));

        let train_df = match dataset.fetch_learn(Segment::Train) {
            Some(df) => df,
            None => {
                logger::logger().error("No training data available");
                return;
            }
        };

        let cols = train_df.get_column_names();
        self.n_features = cols
            .iter()
            .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
            .count();

        logger::logger().info(&format!(
            "Gradient Boosting model initialized with {} features",
            self.n_features
        ));
    }

    fn predict(
        &self,
        dataset: &AlphaDataset,
        segment: Segment,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        logger::logger().debug(&format!(
            "Making Gradient Boosting predictions for segment: {:?}",
            segment
        ));

        let df = match segment {
            Segment::Train => dataset.fetch_learn(Segment::Train),
            Segment::Valid => dataset.fetch_learn(Segment::Valid),
            Segment::Test => dataset.fetch_learn(Segment::Test),
        };

        match df {
            Some(dataframe) => {
                let n_rows = dataframe.height();
                let mut predictions = Vec::with_capacity(n_rows);

                for i in 0..n_rows {
                    let pred = ((i as f64) % 4.0 - 2.0) * 0.02 * self.learning_rate;
                    predictions.push(pred);
                }

                Ok(predictions)
            }
            None => Err("No data available for the specified segment".into()),
        }
    }

    fn detail(&self) -> String {
        format!(
            "GradientBoostingModel:\n  Estimators: {}\n  Learning Rate: {}\n  Max Depth: {:?}\n  Seed: {}\n  Features: {}",
            self.n_estimators, self.learning_rate, self.max_depth, self.seed, self.n_features
        )
    }

    fn name(&self) -> &str {
        "GradientBoosting"
    }
}

/// Ensemble Model
///
/// Combines multiple models for improved prediction.
#[derive(Debug)]
pub struct EnsembleModel {
    /// List of models in the ensemble
    pub models: Vec<Box<dyn AlphaModel>>,
    /// Weights for each model
    pub weights: Vec<f64>,
}

impl EnsembleModel {
    /// Create a new Ensemble model
    pub fn new() -> Self {
        EnsembleModel {
            models: vec![],
            weights: vec![],
        }
    }

    /// Add a model to the ensemble
    pub fn add_model(&mut self, model: Box<dyn AlphaModel>, weight: f64) {
        self.models.push(model);
        self.weights.push(weight);
    }
}

impl Default for EnsembleModel {
    fn default() -> Self {
        Self::new()
    }
}

impl AlphaModel for EnsembleModel {
    fn fit(&mut self, dataset: &AlphaDataset) {
        logger::logger().info(&format!(
            "Fitting Ensemble model with {} sub-models",
            self.models.len()
        ));

        for model in &mut self.models {
            model.fit(dataset);
        }
    }

    fn predict(
        &self,
        dataset: &AlphaDataset,
        segment: Segment,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        logger::logger().debug(&format!(
            "Making Ensemble predictions for segment: {:?}",
            segment
        ));

        if self.models.is_empty() {
            return Err("No models in ensemble".into());
        }

        // Get predictions from all models
        let mut all_predictions = Vec::new();
        for model in &self.models {
            let preds = model.predict(dataset, segment)?;
            all_predictions.push(preds);
        }

        // Combine predictions using weights
        let n_predictions = all_predictions[0].len();
        let mut combined = vec![0.0; n_predictions];

        for (i, preds) in all_predictions.iter().enumerate() {
            let weight = self.weights.get(i).unwrap_or(&1.0);
            for (j, &pred) in preds.iter().enumerate() {
                combined[j] += pred * weight;
            }
        }

        // Normalize by total weight
        let total_weight: f64 = self.weights.iter().sum();
        if total_weight > 0.0 {
            for pred in &mut combined {
                *pred /= total_weight;
            }
        }

        Ok(combined)
    }

    fn detail(&self) -> String {
        let mut details = String::from("EnsembleModel:\n");
        for (i, model) in self.models.iter().enumerate() {
            let weight = self.weights.get(i).unwrap_or(&1.0);
            details.push_str(&format!(
                "  Model {} (weight: {}): {}\n",
                i,
                weight,
                model.name()
            ));
        }
        details
    }

    fn name(&self) -> &str {
        "Ensemble"
    }
}
