//! Model module for alpha research
//! Provides template for machine learning models in alpha research
//!
//! This module implements the AlphaModel trait and several example models
//! matching vnpy's functionality.

use crate::alpha::dataset::{AlphaDataset, Segment};
use crate::alpha::logger;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TreeNode {
    Leaf(f64),
    Split {
        feature_idx: usize,
        threshold: f64,
        gain: f64,
        left: Box<TreeNode>,
        right: Box<TreeNode>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecisionTree {
    max_depth: Option<usize>,
    min_samples_split: usize,
    root: Option<TreeNode>,
}

impl DecisionTree {
    fn new(max_depth: Option<usize>, min_samples_split: usize) -> Self {
        DecisionTree {
            max_depth,
            min_samples_split,
            root: None,
        }
    }

    fn fit(&mut self, x: &[Vec<f64>], y: &[f64], rng: &mut StdRng, max_features: usize) {
        let n_samples = y.len();
        if n_samples == 0 {
            self.root = Some(TreeNode::Leaf(0.0));
            return;
        }
        let n_features = if x.is_empty() { 0 } else { x[0].len() };
        if n_features == 0 {
            self.root = Some(TreeNode::Leaf(mean(y)));
            return;
        }
        let indices: Vec<usize> = (0..n_samples).collect();
        self.root = Some(self.build_tree(x, y, &indices, 0, rng, max_features, n_features));
    }

    #[allow(clippy::too_many_arguments)]
    fn build_tree(
        &self,
        x: &[Vec<f64>],
        y: &[f64],
        indices: &[usize],
        depth: usize,
        rng: &mut StdRng,
        max_features: usize,
        n_features: usize,
    ) -> TreeNode {
        let n = indices.len();

        if n < self.min_samples_split {
            return TreeNode::Leaf(mean_of_indices(y, indices));
        }
        if let Some(max_d) = self.max_depth {
            if depth >= max_d {
                return TreeNode::Leaf(mean_of_indices(y, indices));
            }
        }
        let y_mean = mean_of_indices(y, indices);
        let all_same = indices.iter().all(|&i| (y[i] - y_mean).abs() < 1e-12);
        if all_same {
            return TreeNode::Leaf(y_mean);
        }

        // Subsample features
        let mut feature_indices: Vec<usize> = (0..n_features).collect();
        feature_indices.shuffle(rng);
        let candidate_features: Vec<usize> =
            feature_indices.into_iter().take(max_features).collect();

        // Find best split
        let parent_var = variance_of_indices(y, indices);
        let mut best_gain = 0.0;
        let mut best_feature = 0;
        let mut best_threshold = 0.0;
        let mut best_left: Vec<usize> = Vec::new();
        let mut best_right: Vec<usize> = Vec::new();

        for &feat_idx in &candidate_features {
            // Collect (value, sample_index) pairs, skip NaN
            let mut val_idx: Vec<(f64, usize)> = indices
                .iter()
                .filter_map(|&i| {
                    let v = x[i][feat_idx];
                    if v.is_nan() {
                        None
                    } else {
                        Some((v, i))
                    }
                })
                .collect();
            if val_idx.len() < 2 {
                continue;
            }
            val_idx.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let mut running_left_sum = 0.0;
            let mut running_left_sq_sum = 0.0;
            let mut left_count = 0usize;

            for split_pos in 0..val_idx.len() - 1 {
                let (val, idx) = val_idx[split_pos];
                let yv = y[idx];
                running_left_sum += yv;
                running_left_sq_sum += yv * yv;
                left_count += 1;

                // Move from right to left
                let right_count = val_idx.len() - left_count;

                // Only split between different values
                let next_val = val_idx[split_pos + 1].0;
                if (val - next_val).abs() < 1e-12 {
                    continue;
                }
                if left_count < self.min_samples_split || right_count < self.min_samples_split {
                    continue;
                }

                let left_mean = running_left_sum / left_count as f64;
                let left_var = running_left_sq_sum / left_count as f64 - left_mean * left_mean;
                let right_sum: f64 = val_idx[split_pos + 1..].iter().map(|&(_, i)| y[i]).sum();
                let right_sq_sum: f64 = val_idx[split_pos + 1..]
                    .iter()
                    .map(|&(_, i)| y[i] * y[i])
                    .sum();
                let right_mean_new = right_sum / right_count as f64;
                let right_var_new =
                    right_sq_sum / right_count as f64 - right_mean_new * right_mean_new;

                let n_total = val_idx.len() as f64;
                let gain = parent_var
                    - (left_count as f64 / n_total) * left_var
                    - (right_count as f64 / n_total) * right_var_new;

                if gain > best_gain {
                    best_gain = gain;
                    best_feature = feat_idx;
                    best_threshold = (val + next_val) / 2.0;
                    best_left = val_idx[..=split_pos].iter().map(|&(_, i)| i).collect();
                    best_right = val_idx[split_pos + 1..].iter().map(|&(_, i)| i).collect();
                }
            }
        }

        if best_gain <= 0.0 || best_left.is_empty() || best_right.is_empty() {
            return TreeNode::Leaf(y_mean);
        }

        let left_node = self.build_tree(x, y, &best_left, depth + 1, rng, max_features, n_features);
        let right_node =
            self.build_tree(x, y, &best_right, depth + 1, rng, max_features, n_features);

        TreeNode::Split {
            feature_idx: best_feature,
            threshold: best_threshold,
            gain: best_gain,
            left: Box::new(left_node),
            right: Box::new(right_node),
        }
    }

    /// Predict for a single sample.
    fn predict_one(&self, sample: &[f64]) -> f64 {
        match &self.root {
            Some(node) => Self::traverse(node, sample),
            None => 0.0,
        }
    }

    fn traverse(node: &TreeNode, sample: &[f64]) -> f64 {
        match node {
            TreeNode::Leaf(val) => *val,
            TreeNode::Split {
                feature_idx,
                threshold,
                left,
                right,
                ..
            } => {
                let val = sample.get(*feature_idx).copied().unwrap_or(0.0);
                if val.is_nan() || val <= *threshold {
                    Self::traverse(left, sample)
                } else {
                    Self::traverse(right, sample)
                }
            }
        }
    }

    /// Accumulate feature importance: for each split, add the gain to the feature's importance.
    fn feature_importance(&self, importances: &mut [f64]) {
        if let Some(node) = &self.root {
            Self::accumulate_importance(node, importances);
        }
    }

    fn accumulate_importance(node: &TreeNode, importances: &mut [f64]) {
        match node {
            TreeNode::Leaf(_) => {}
            TreeNode::Split {
                feature_idx,
                gain,
                left,
                right,
                ..
            } => {
                if *feature_idx < importances.len() {
                    importances[*feature_idx] += gain;
                }
                Self::accumulate_importance(left, importances);
                Self::accumulate_importance(right, importances);
            }
        }
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn mean_of_indices(y: &[f64], indices: &[usize]) -> f64 {
    if indices.is_empty() {
        return 0.0;
    }
    let sum: f64 = indices.iter().map(|&i| y[i]).sum();
    sum / indices.len() as f64
}

fn variance_of_indices(y: &[f64], indices: &[usize]) -> f64 {
    if indices.len() < 2 {
        return 0.0;
    }
    let n = indices.len() as f64;
    let sum: f64 = indices.iter().map(|&i| y[i]).sum();
    let sum_sq: f64 = indices.iter().map(|&i| y[i] * y[i]).sum();
    let mean = sum / n;
    sum_sq / n - mean * mean
}

type ExtractDataResult = Option<(Vec<Vec<f64>>, Vec<f64>, Vec<String>)>;

fn extract_data(df: &polars::prelude::DataFrame) -> ExtractDataResult {
    let cols = df.get_column_names();
    let feature_cols: Vec<String> = cols
        .iter()
        .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
        .map(|c| c.to_string())
        .collect();

    let n_rows = df.height();
    let n_features = feature_cols.len();
    if n_features == 0 || n_rows == 0 {
        return None;
    }

    let mut x_matrix: Vec<Vec<f64>> = vec![vec![0.0; n_features]; n_rows];
    let mut y_vec: Vec<f64> = vec![0.0; n_rows];

    if let Ok(label_col) = df.column("label") {
        if let Ok(label_ca) = label_col.f64() {
            for (i, val) in label_ca.into_iter().enumerate() {
                y_vec[i] = val.unwrap_or(0.0);
            }
        } else {
            return None;
        }
    } else {
        return None;
    }

    for (j, col_name) in feature_cols.iter().enumerate() {
        if let Ok(col) = df.column(col_name) {
            if let Ok(ca) = col.f64() {
                for (i, val) in ca.into_iter().enumerate() {
                    x_matrix[i][j] = val.unwrap_or(0.0);
                }
            }
        }
    }

    Some((x_matrix, y_vec, feature_cols))
}

fn extract_features(df: &polars::prelude::DataFrame, feature_cols: &[String]) -> Vec<Vec<f64>> {
    let n_rows = df.height();
    let n_features = feature_cols.len();
    let mut x_matrix: Vec<Vec<f64>> = vec![vec![0.0; n_features]; n_rows];
    for (j, col_name) in feature_cols.iter().enumerate() {
        if let Ok(col) = df.column(col_name) {
            if let Ok(ca) = col.f64() {
                for (i, val) in ca.into_iter().enumerate() {
                    x_matrix[i][j] = val.unwrap_or(0.0);
                }
            }
        }
    }
    x_matrix
}

/// Random Forest Model
///
/// A random forest model for alpha factor prediction using bagging and
/// feature subsampling with pure Rust decision trees.
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
    /// Trained decision trees
    #[serde(default)]
    trees: Vec<DecisionTree>,
    /// Feature column names (stored for prediction)
    #[serde(default)]
    feature_cols: Vec<String>,
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
            trees: vec![],
            feature_cols: vec![],
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
            trees: vec![],
            feature_cols: vec![],
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

        let (x_matrix, y_vec, feature_cols) = match extract_data(&train_df) {
            Some(d) => d,
            None => {
                logger::logger().error("No features or samples available for fitting");
                return;
            }
        };

        self.n_features = feature_cols.len();
        self.feature_cols = feature_cols;
        let n_samples = y_vec.len();

        if n_samples == 0 {
            logger::logger().error("No samples available for fitting");
            return;
        }

        let max_features = (self.n_features as f64).sqrt().ceil() as usize;
        let max_features = max_features.max(1);

        let mut rng = StdRng::seed_from_u64(self.seed);
        self.trees.clear();

        for _ in 0..self.n_estimators {
            let bootstrap_indices: Vec<usize> = (0..n_samples)
                .map(|_| rng.random_range(0..n_samples))
                .collect();

            let bootstrap_x: Vec<Vec<f64>> = bootstrap_indices
                .iter()
                .map(|&i| x_matrix[i].clone())
                .collect();
            let bootstrap_y: Vec<f64> = bootstrap_indices.iter().map(|&i| y_vec[i]).collect();

            let mut tree = DecisionTree::new(self.max_depth, self.min_samples_split);
            tree.fit(&bootstrap_x, &bootstrap_y, &mut rng, max_features);
            self.trees.push(tree);
        }

        let mut importances = vec![0.0; self.n_features];
        for tree in &self.trees {
            tree.feature_importance(&mut importances);
        }
        let total: f64 = importances.iter().sum();
        if total > 0.0 {
            for imp in &mut importances {
                *imp /= total;
            }
        } else {
            let uniform = 1.0 / self.n_features as f64;
            importances.fill(uniform);
        }
        self.feature_importances = importances;

        logger::logger().info(&format!(
            "Random Forest model fitted with {} features, {} samples, {} trees",
            self.n_features,
            n_samples,
            self.trees.len()
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

        if self.trees.is_empty() {
            return Err("Model not fitted yet".into());
        }

        let df = match segment {
            Segment::Train => dataset.fetch_learn(Segment::Train),
            Segment::Valid => dataset.fetch_learn(Segment::Valid),
            Segment::Test => dataset.fetch_learn(Segment::Test),
        };

        match df {
            Some(dataframe) => {
                let x_matrix = extract_features(&dataframe, &self.feature_cols);
                let n_rows = x_matrix.len();
                let n_trees = self.trees.len();

                let mut predictions = Vec::with_capacity(n_rows);
                for row in x_matrix.iter().take(n_rows) {
                    let sum: f64 = self.trees.iter().map(|t| t.predict_one(row)).sum();
                    predictions.push(sum / n_trees as f64);
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
/// A gradient boosting model for alpha factor prediction using sequential
/// decision trees fitted on residuals (pure Rust implementation).
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
    /// Initial prediction (mean of labels)
    #[serde(default)]
    init_prediction: f64,
    /// Trained decision trees
    #[serde(default)]
    trees: Vec<DecisionTree>,
    /// Feature column names (stored for prediction)
    #[serde(default)]
    feature_cols: Vec<String>,
    /// Minimum samples to split
    #[serde(default = "default_min_samples_split")]
    min_samples_split: usize,
}

fn default_min_samples_split() -> usize {
    2
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
            init_prediction: 0.0,
            trees: vec![],
            feature_cols: vec![],
            min_samples_split: 2,
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

        let (x_matrix, y_vec, feature_cols) = match extract_data(&train_df) {
            Some(d) => d,
            None => {
                logger::logger().error("No features or samples available for fitting");
                return;
            }
        };

        self.n_features = feature_cols.len();
        self.feature_cols = feature_cols;
        let n_samples = y_vec.len();

        if n_samples == 0 {
            logger::logger().error("No samples available for fitting");
            return;
        }

        let max_features = self.n_features;

        let mut rng = StdRng::seed_from_u64(self.seed);

        self.init_prediction = mean(&y_vec);
        let mut current_predictions = vec![self.init_prediction; n_samples];

        self.trees.clear();

        for _ in 0..self.n_estimators {
            let residuals: Vec<f64> = y_vec
                .iter()
                .zip(current_predictions.iter())
                .map(|(&y, &pred)| y - pred)
                .collect();

            let mut tree = DecisionTree::new(self.max_depth, self.min_samples_split);
            tree.fit(&x_matrix, &residuals, &mut rng, max_features);
            self.trees.push(tree);

            for (i, pred) in current_predictions.iter_mut().enumerate() {
                *pred += self.learning_rate
                    * self
                        .trees
                        .last()
                        .expect("tree was just pushed")
                        .predict_one(&x_matrix[i]);
            }
        }

        logger::logger().info(&format!(
            "Gradient Boosting model fitted with {} features, {} samples, {} trees, init={:.6}",
            self.n_features,
            n_samples,
            self.trees.len(),
            self.init_prediction
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

        if self.trees.is_empty() {
            return Err("Model not fitted yet".into());
        }

        let df = match segment {
            Segment::Train => dataset.fetch_learn(Segment::Train),
            Segment::Valid => dataset.fetch_learn(Segment::Valid),
            Segment::Test => dataset.fetch_learn(Segment::Test),
        };

        match df {
            Some(dataframe) => {
                let x_matrix = extract_features(&dataframe, &self.feature_cols);
                let n_rows = x_matrix.len();

                let mut predictions = vec![self.init_prediction; n_rows];
                for tree in &self.trees {
                    for (i, pred) in predictions.iter_mut().enumerate() {
                        *pred += self.learning_rate * tree.predict_one(&x_matrix[i]);
                    }
                }

                Ok(predictions)
            }
            None => Err("No data available for the specified segment".into()),
        }
    }

    fn detail(&self) -> String {
        format!(
            "GradientBoostingModel:\n  Estimators: {}\n  Learning Rate: {}\n  Max Depth: {:?}\n  Seed: {}\n  Features: {}\n  Init Prediction: {:.6}",
            self.n_estimators, self.learning_rate, self.max_depth, self.seed, self.n_features, self.init_prediction
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_regression_new() {
        let model = LinearRegressionModel::new();
        assert!(model.weights.is_empty());
        assert!((model.bias - 0.0).abs() < 1e-10);
        assert_eq!(model.n_features, 0);
        assert_eq!(model.n_samples, 0);
    }

    #[test]
    fn test_linear_regression_with_features() {
        let model = LinearRegressionModel::with_features(3);
        assert_eq!(model.weights.len(), 3);
        assert_eq!(model.n_features, 3);
        assert!(model.weights.iter().all(|&w| (w - 0.0).abs() < 1e-10));
    }

    #[test]
    fn test_linear_regression_default() {
        let model = LinearRegressionModel::default();
        assert!(model.weights.is_empty());
    }

    #[test]
    fn test_linear_regression_detail() {
        let model = LinearRegressionModel::with_features(2);
        let detail = model.detail();
        assert!(detail.contains("LinearRegressionModel"));
        assert!(detail.contains("Features: 2"));
    }

    #[test]
    fn test_linear_regression_name() {
        let model = LinearRegressionModel::new();
        assert_eq!(model.name(), "LinearRegression");
    }

    #[test]
    fn test_random_forest_new() {
        let model = RandomForestModel::new(50, Some(5));
        assert_eq!(model.n_estimators, 50);
        assert_eq!(model.max_depth, Some(5));
        assert_eq!(model.min_samples_split, 2);
        assert_eq!(model.seed, 42);
        assert!(model.feature_importances.is_empty());
    }

    #[test]
    fn test_random_forest_default() {
        let model = RandomForestModel::default();
        assert_eq!(model.n_estimators, 100);
        assert_eq!(model.max_depth, None);
    }

    #[test]
    fn test_random_forest_with_params() {
        let model = RandomForestModel::with_params(200, Some(10), 5, 123);
        assert_eq!(model.n_estimators, 200);
        assert_eq!(model.max_depth, Some(10));
        assert_eq!(model.min_samples_split, 5);
        assert_eq!(model.seed, 123);
    }

    #[test]
    fn test_random_forest_detail() {
        let model = RandomForestModel::new(100, Some(5));
        let detail = model.detail();
        assert!(detail.contains("RandomForestModel"));
        assert!(detail.contains("100"));
    }

    #[test]
    fn test_random_forest_name() {
        let model = RandomForestModel::new(100, None);
        assert_eq!(model.name(), "RandomForest");
    }

    #[test]
    fn test_gradient_boosting_new() {
        let model = GradientBoostingModel::new(200, 0.05);
        assert_eq!(model.n_estimators, 200);
        assert!((model.learning_rate - 0.05).abs() < 1e-10);
        assert_eq!(model.max_depth, Some(3));
        assert_eq!(model.seed, 42);
    }

    #[test]
    fn test_gradient_boosting_default() {
        let model = GradientBoostingModel::default();
        assert_eq!(model.n_estimators, 100);
        assert!((model.learning_rate - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_gradient_boosting_detail() {
        let model = GradientBoostingModel::new(100, 0.1);
        let detail = model.detail();
        assert!(detail.contains("GradientBoostingModel"));
        assert!(detail.contains("100"));
    }

    #[test]
    fn test_gradient_boosting_name() {
        let model = GradientBoostingModel::new(100, 0.1);
        assert_eq!(model.name(), "GradientBoosting");
    }

    #[test]
    fn test_ensemble_new() {
        let model = EnsembleModel::new();
        assert!(model.models.is_empty());
        assert!(model.weights.is_empty());
    }

    #[test]
    fn test_ensemble_default() {
        let model = EnsembleModel::default();
        assert!(model.models.is_empty());
    }

    #[test]
    fn test_ensemble_add_model() {
        let mut ensemble = EnsembleModel::new();
        ensemble.add_model(Box::new(LinearRegressionModel::new()), 0.6);
        ensemble.add_model(Box::new(RandomForestModel::default()), 0.4);
        assert_eq!(ensemble.models.len(), 2);
        assert_eq!(ensemble.weights.len(), 2);
        assert!((ensemble.weights[0] - 0.6).abs() < 1e-10);
        assert!((ensemble.weights[1] - 0.4).abs() < 1e-10);
    }

    #[test]
    fn test_ensemble_name() {
        let ensemble = EnsembleModel::new();
        assert_eq!(ensemble.name(), "Ensemble");
    }

    #[test]
    fn test_ensemble_detail() {
        let mut ensemble = EnsembleModel::new();
        ensemble.add_model(Box::new(LinearRegressionModel::new()), 0.5);
        ensemble.add_model(Box::new(RandomForestModel::default()), 0.5);
        let detail = ensemble.detail();
        assert!(detail.contains("EnsembleModel"));
        assert!(detail.contains("LinearRegression"));
        assert!(detail.contains("RandomForest"));
    }

    #[test]
    fn test_solve_linear_system_identity() {
        let a = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let b = vec![3.0, 5.0];
        let result = solve_linear_system(&a, &b);
        assert!(result.is_some());
        let x = result.unwrap();
        assert!((x[0] - 3.0).abs() < 1e-10);
        assert!((x[1] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_solve_linear_system_2x2() {
        let a = vec![vec![2.0, 1.0], vec![1.0, 3.0]];
        let b = vec![5.0, 10.0];
        let result = solve_linear_system(&a, &b);
        assert!(result.is_some());
        let x = result.unwrap();
        assert!((x[0] - 1.0).abs() < 1e-6);
        assert!((x[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_solve_linear_system_singular() {
        let a = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let b = vec![3.0, 6.0];
        let result = solve_linear_system(&a, &b);
        assert!(result.is_none());
    }

    #[test]
    fn test_solve_linear_system_3x3() {
        let a = vec![
            vec![1.0, 2.0, -1.0],
            vec![2.0, 1.0, -2.0],
            vec![-3.0, 1.0, 1.0],
        ];
        let b = vec![3.0, 3.0, -6.0];
        let result = solve_linear_system(&a, &b);
        assert!(result.is_some());
        let x = result.unwrap();
        assert!((x[0] - 3.0).abs() < 1e-6, "x[0] = {}", x[0]);
        assert!((x[1] - 1.0).abs() < 1e-6, "x[1] = {}", x[1]);
        assert!((x[2] - 2.0).abs() < 1e-6, "x[2] = {}", x[2]);
    }

    #[test]
    fn test_ensemble_predict_empty_models() {
        let ensemble = EnsembleModel::new();
        let df = polars::prelude::DataFrame::empty();
        let dataset = AlphaDataset::new(
            df,
            ("20240101".to_string(), "20240201".to_string()),
            ("20240201".to_string(), "20240301".to_string()),
            ("20240301".to_string(), "20240401".to_string()),
        );
        let result = ensemble.predict(&dataset, Segment::Train);
        assert!(result.is_err());
    }
}
