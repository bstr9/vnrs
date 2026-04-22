//! Alpha research GUI panel.
//!
//! Provides a quantitative research interface with:
//! - Model Training tab: select model type, configure hyperparameters, train
//! - Factor Analysis tab: select and analyze factors
//! - Alpha Portfolio tab: combine signals, view weights, backtest alpha

use egui::{RichText, Ui, Color32};
use std::sync::{Arc, RwLock};

use super::style::{COLOR_TEXT_SECONDARY};

use crate::alpha::AlphaLab;
use crate::alpha::AlphaModel;
use crate::alpha::AlphaDataset;
use crate::alpha::Segment;
use crate::alpha::model::{
    LinearRegressionModel, RandomForestModel, GradientBoostingModel,
};
use crate::alpha::BacktestingEngine;

// ---------------------------------------------------------------------------
// Model types
// ---------------------------------------------------------------------------

/// Available alpha model types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaModelType {
    LinearRegression,
    Ridge,
    Lasso,
    RandomForest,
    GradientBoosting,
}

impl AlphaModelType {
    pub fn all() -> &'static [AlphaModelType] {
        &[
            AlphaModelType::LinearRegression,
            AlphaModelType::Ridge,
            AlphaModelType::Lasso,
            AlphaModelType::RandomForest,
            AlphaModelType::GradientBoosting,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AlphaModelType::LinearRegression => "LinearRegression",
            AlphaModelType::Ridge => "Ridge",
            AlphaModelType::Lasso => "Lasso",
            AlphaModelType::RandomForest => "RandomForest",
            AlphaModelType::GradientBoosting => "GradientBoosting",
        }
    }

    pub fn chinese_name(&self) -> &'static str {
        match self {
            AlphaModelType::LinearRegression => "线性回归",
            AlphaModelType::Ridge => "岭回归",
            AlphaModelType::Lasso => "Lasso回归",
            AlphaModelType::RandomForest => "随机森林",
            AlphaModelType::GradientBoosting => "梯度提升",
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-tab selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlphaSubTab {
    ModelTraining,
    FactorAnalysis,
    AlphaPortfolio,
}

impl AlphaSubTab {
    fn display_name(&self) -> &'static str {
        match self {
            AlphaSubTab::ModelTraining => "模型训练",
            AlphaSubTab::FactorAnalysis => "因子分析",
            AlphaSubTab::AlphaPortfolio => "Alpha组合",
        }
    }
}

// ---------------------------------------------------------------------------
// Training hyperparameters (varies by model type)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HyperParams {
    /// Number of estimators (RandomForest, GradientBoosting)
    n_estimators: usize,
    /// Max depth (RandomForest, GradientBoosting)
    max_depth: usize,
    /// Learning rate (GradientBoosting)
    learning_rate: f64,
    /// Alpha regularization (Ridge, Lasso)
    alpha_reg: f64,
    /// Random seed
    seed: u64,
}

impl Default for HyperParams {
    fn default() -> Self {
        Self {
            n_estimators: 100,
            max_depth: 5,
            learning_rate: 0.1,
            alpha_reg: 1.0,
            seed: 42,
        }
    }
}

// ---------------------------------------------------------------------------
// Training result display
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct TrainResult {
    model_name: String,
    n_features: usize,
    n_samples: usize,
    train_mse: f64,
    valid_mse: f64,
    detail: String,
    trained: bool,
}

// ---------------------------------------------------------------------------
// Factor analysis state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct FactorState {
    selected_factor: String,
    available_factors: Vec<String>,
    analysis_result: Option<FactorAnalysisResult>,
}

#[derive(Debug, Clone)]
struct FactorAnalysisResult {
    factor_name: String,
    mean: f64,
    std: f64,
    min: f64,
    max: f64,
    ic: f64,
    value_count: usize,
}

// ---------------------------------------------------------------------------
// Alpha portfolio state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct PortfolioState {
    selected_models: Vec<String>,
    selected_factors: Vec<String>,
    weights: Vec<f64>,
    backtest_status: String,
}

// ---------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------

/// Alpha research panel for the trading platform GUI.
///
/// Feature-gated behind `#[cfg(feature = "alpha")]` since it depends on
/// the alpha module's data structures.
pub struct AlphaPanel {
    /// Active sub-tab
    sub_tab: AlphaSubTab,
    /// Model type selector
    model_type: AlphaModelType,
    /// Hyperparameters
    hyper_params: HyperParams,
    /// Dataset name
    dataset_name: String,
    /// Available datasets
    datasets: Vec<String>,
    /// Last training result
    train_result: TrainResult,
    /// Training in progress
    training: bool,
    /// Factor analysis state
    factor_state: FactorState,
    /// Portfolio state
    portfolio_state: PortfolioState,
    /// Status message
    status_message: String,
    /// Reference to AlphaLab engine
    alpha_lab: Option<Arc<RwLock<AlphaLab>>>,
    /// Model name for save/load
    model_name: String,
    /// List of saved model names
    saved_models: Vec<String>,
    /// Index of selected saved model in ComboBox
    selected_saved_model_idx: usize,
}

impl AlphaPanel {
    pub fn new() -> Self {
        let default_factors: Vec<String> = vec![
            "return_1d".to_string(),
            "return_5d".to_string(),
            "ma_ratio_5_20".to_string(),
            "rsi_14".to_string(),
            "bollinger_position".to_string(),
            "volume_ratio".to_string(),
        ];

        AlphaPanel {
            sub_tab: AlphaSubTab::ModelTraining,
            model_type: AlphaModelType::LinearRegression,
            hyper_params: HyperParams::default(),
            dataset_name: String::new(),
            datasets: Vec::new(),
            train_result: TrainResult::default(),
            training: false,
            factor_state: FactorState {
                selected_factor: String::new(),
                available_factors: default_factors,
                analysis_result: None,
            },
            portfolio_state: PortfolioState::default(),
            status_message: String::new(),
            alpha_lab: None,
            model_name: String::new(),
            saved_models: Vec::new(),
            selected_saved_model_idx: 0,
        }
    }

    /// Set the AlphaLab engine reference
    pub fn set_alpha_lab(&mut self, lab: Arc<RwLock<AlphaLab>>) {
        if let Ok(lab_guard) = lab.read() {
            self.datasets = lab_guard.list_all_datasets();
            self.saved_models = lab_guard.list_all_models();
        }
        self.alpha_lab = Some(lab);
    }

    /// Refresh datasets and models lists from AlphaLab
    fn refresh_from_lab(&mut self) {
        if let Some(ref lab) = self.alpha_lab {
            if let Ok(lab_guard) = lab.read() {
                self.datasets = lab_guard.list_all_datasets();
                self.saved_models = lab_guard.list_all_models();
            }
        }
    }

    /// Render the panel into the given Ui
    pub fn show(&mut self, ui: &mut Ui) {
        // Refresh dataset/model lists from lab
        self.refresh_from_lab();

        // Sub-tab bar
        ui.horizontal(|ui| {
            for tab in &[
                AlphaSubTab::ModelTraining,
                AlphaSubTab::FactorAnalysis,
                AlphaSubTab::AlphaPortfolio,
            ] {
                ui.selectable_value(&mut self.sub_tab, *tab, tab.display_name());
            }
        });

        ui.separator();

        // Status bar
        if !self.status_message.is_empty() {
            ui.colored_label(Color32::from_rgb(100, 200, 255), &self.status_message);
            ui.add_space(4.0);
        }

        match self.sub_tab {
            AlphaSubTab::ModelTraining => self.show_model_training(ui),
            AlphaSubTab::FactorAnalysis => self.show_factor_analysis(ui),
            AlphaSubTab::AlphaPortfolio => self.show_alpha_portfolio(ui),
        }
    }

    // -----------------------------------------------------------------------
    // Model Training tab
    // -----------------------------------------------------------------------

    fn show_model_training(&mut self, ui: &mut Ui) {
        // Model type selector
        ui.horizontal(|ui| {
            ui.label(RichText::new("模型类型:").strong());
            for mt in AlphaModelType::all() {
                ui.selectable_value(
                    &mut self.model_type,
                    *mt,
                    format!("{} ({})", mt.display_name(), mt.chinese_name()),
                );
            }
        });

        ui.add_space(8.0);

        // Hyperparameters (contextual based on model type)
        ui.group(|ui| {
            ui.label(RichText::new("超参数").strong());
            ui.add_space(4.0);

            match self.model_type {
                AlphaModelType::LinearRegression => {
                    ui.label("线性回归: 无额外超参数");
                }
                AlphaModelType::Ridge | AlphaModelType::Lasso => {
                    ui.horizontal(|ui| {
                        ui.label("正则化系数 (alpha):");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.alpha_reg)
                                .range(0.0..=100.0)
                                .speed(0.1),
                        );
                    });
                }
                AlphaModelType::RandomForest => {
                    ui.horizontal(|ui| {
                        ui.label("树数量:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.n_estimators)
                                .range(1..=1000)
                                .speed(1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("最大深度:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.max_depth)
                                .range(1..=50)
                                .speed(1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("随机种子:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.seed)
                                .range(0..=u64::MAX)
                                .speed(1),
                        );
                    });
                }
                AlphaModelType::GradientBoosting => {
                    ui.horizontal(|ui| {
                        ui.label("提升轮数:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.n_estimators)
                                .range(1..=1000)
                                .speed(1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("学习率:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.learning_rate)
                                .range(0.001..=1.0)
                                .speed(0.01),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("最大深度:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.max_depth)
                                .range(1..=50)
                                .speed(1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("随机种子:");
                        ui.add(
                            egui::DragValue::new(&mut self.hyper_params.seed)
                                .range(0..=u64::MAX)
                                .speed(1),
                        );
                    });
                }
            }
        });

        ui.add_space(8.0);

        // Training data selector
        ui.group(|ui| {
            ui.label(RichText::new("训练数据").strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("数据集:");
                ui.text_edit_singleline(&mut self.dataset_name);
            });

            if self.datasets.is_empty() {
                ui.colored_label(
                    COLOR_TEXT_SECONDARY,
                    "暂无数据集 — 请先通过 AlphaLab 加载数据",
                );
            } else {
                ui.horizontal(|ui| {
                    ui.label("可用数据集:");
                    for ds in &self.datasets {
                        if ui.small_button(ds).clicked() {
                            self.dataset_name = ds.clone();
                        }
                    }
                });
            }
        });

        ui.add_space(8.0);

        // Train button + progress
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.training, egui::Button::new("开始训练"))
                .clicked()
            {
                self.start_training();
            }

            if self.training {
                ui.spinner();
                ui.label("训练中...");
            }
        });

        ui.add_space(8.0);

        // Training results display
        if self.train_result.trained {
            ui.group(|ui| {
                ui.label(RichText::new("训练结果").strong());
                ui.add_space(4.0);

                let r = &self.train_result;
                Self::show_metric_row(ui, "模型", &r.model_name);
                Self::show_metric_row(ui, "特征数", &r.n_features.to_string());
                Self::show_metric_row(ui, "样本数", &r.n_samples.to_string());
                Self::show_metric_row(ui, "训练MSE", &format!("{:.6}", r.train_mse));
                Self::show_metric_row(ui, "验证MSE", &format!("{:.6}", r.valid_mse));

                if !r.detail.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new("详细信息:").color(COLOR_TEXT_SECONDARY));
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.label(&r.detail);
                        });
                }
            });
        }

        ui.add_space(8.0);

        // Model management section
        ui.group(|ui| {
            ui.label(RichText::new("模型管理").strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("模型名称:");
                ui.text_edit_singleline(&mut self.model_name);
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.train_result.trained && !self.model_name.is_empty(),
                        egui::Button::new("保存模型"),
                    )
                    .clicked()
                {
                    self.save_model();
                }

                if ui
                    .add_enabled(
                        !self.saved_models.is_empty(),
                        egui::Button::new("加载模型"),
                    )
                    .clicked()
                {
                    self.load_model();
                }

                if ui
                    .add_enabled(
                        !self.saved_models.is_empty(),
                        egui::Button::new("删除模型"),
                    )
                    .clicked()
                {
                    self.delete_model();
                }
            });

            if !self.saved_models.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("已保存模型:");
                    egui::ComboBox::from_id_salt("saved_models_combo")
                        .selected_text(
                            self.saved_models
                                .get(self.selected_saved_model_idx)
                                .map(|s| s.as_str())
                                .unwrap_or("选择模型"),
                        )
                        .show_index(
                            ui,
                            &mut self.selected_saved_model_idx,
                            self.saved_models.len(),
                            |i| self.saved_models.get(i).cloned().unwrap_or_default(),
                        );
                });
            } else {
                ui.colored_label(COLOR_TEXT_SECONDARY, "暂无已保存模型");
            }
        });
    }

    // -----------------------------------------------------------------------
    // Factor Analysis tab
    // -----------------------------------------------------------------------

    fn show_factor_analysis(&mut self, ui: &mut Ui) {
        // Factor selector
        ui.group(|ui| {
            ui.label(RichText::new("因子选择").strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("因子名称:");
                ui.text_edit_singleline(&mut self.factor_state.selected_factor);
            });

            if !self.factor_state.available_factors.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("内置因子:");
                    for f in &self.factor_state.available_factors.clone() {
                        if ui.small_button(f).clicked() {
                            self.factor_state.selected_factor = f.clone();
                        }
                    }
                });
            }
        });

        ui.add_space(8.0);

        // Dataset selector for factor analysis
        ui.group(|ui| {
            ui.label(RichText::new("数据来源").strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("数据集:");
                ui.text_edit_singleline(&mut self.dataset_name);
            });

            if !self.datasets.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("可用数据集:");
                    for ds in &self.datasets {
                        if ui.small_button(ds).clicked() {
                            self.dataset_name = ds.clone();
                        }
                    }
                });
            }
        });

        ui.add_space(8.0);

        // Run analysis button
        if ui.button("运行分析").clicked() {
            self.run_factor_analysis();
        }

        ui.add_space(8.0);

        // Factor distribution display
        if let Some(ref result) = self.factor_state.analysis_result {
            ui.group(|ui| {
                ui.label(RichText::new("分析结果").strong());
                ui.add_space(4.0);

                Self::show_metric_row(ui, "因子", &result.factor_name);
                Self::show_metric_row(ui, "数据量", &result.value_count.to_string());
                Self::show_metric_row(ui, "均值", &format!("{:.6}", result.mean));
                Self::show_metric_row(ui, "标准差", &format!("{:.6}", result.std));
                Self::show_metric_row(ui, "最小值", &format!("{:.6}", result.min));
                Self::show_metric_row(ui, "最大值", &format!("{:.6}", result.max));
                Self::show_metric_row(ui, "IC (信息系数)", &format!("{:.6}", result.ic));

                ui.add_space(4.0);
                ui.label(RichText::new("因子分布 (文本直方图):").color(COLOR_TEXT_SECONDARY));
                Self::show_text_histogram(ui, result.mean, result.std, result.min, result.max);
            });
        }
    }

    // -----------------------------------------------------------------------
    // Alpha Portfolio tab
    // -----------------------------------------------------------------------

    fn show_alpha_portfolio(&mut self, ui: &mut Ui) {
        // Model/factor selection
        ui.group(|ui| {
            ui.label(RichText::new("信号组合").strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("选择模型:");
                if self.train_result.trained {
                    ui.colored_label(Color32::GREEN, &self.train_result.model_name);
                } else {
                    ui.colored_label(COLOR_TEXT_SECONDARY, "未训练模型");
                }
            });

            ui.horizontal(|ui| {
                ui.label("选择因子:");
                if self.factor_state.analysis_result.is_some() {
                    ui.colored_label(
                        Color32::GREEN,
                        self.factor_state.analysis_result.as_ref().map_or_else(
                            || "".to_string(),
                            |r| r.factor_name.clone(),
                        ),
                    );
                } else {
                    ui.colored_label(COLOR_TEXT_SECONDARY, "未选择因子");
                }
            });
        });

        ui.add_space(8.0);

        // Weight display
        ui.group(|ui| {
            ui.label(RichText::new("权重分配").strong());
            ui.add_space(4.0);

            if self.portfolio_state.weights.is_empty() {
                ui.colored_label(COLOR_TEXT_SECONDARY, "尚未分配权重 — 请先组合信号");
            } else {
                for (i, w) in self.portfolio_state.weights.iter().enumerate() {
                    let label = if i < self.portfolio_state.selected_models.len() {
                        self.portfolio_state.selected_models[i].clone()
                    } else if i - self.portfolio_state.selected_models.len()
                        < self.portfolio_state.selected_factors.len()
                    {
                        self.portfolio_state
                            .selected_factors
                            .get(i - self.portfolio_state.selected_models.len())
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        format!("信号 {}", i)
                    };
                    ui.horizontal(|ui| {
                        ui.label(&label);
                        ui.add_space(8.0);
                        let pct = *w * 100.0;
                        ui.add(egui::ProgressBar::new(*w as f32).text(format!("{:.1}%", pct)));
                    });
                }
            }
        });

        ui.add_space(8.0);

        // Backtest alpha signal button
        ui.horizontal(|ui| {
            if ui.button("回测Alpha信号").clicked() {
                self.backtest_alpha_signal();
            }
        });

        if !self.portfolio_state.backtest_status.is_empty() {
            ui.add_space(4.0);
            ui.label(&self.portfolio_state.backtest_status);
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn show_metric_row(ui: &mut Ui, label: &str, value: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", label)).color(COLOR_TEXT_SECONDARY));
            ui.label(value);
        });
    }

    /// Render a simple text-based histogram
    fn show_text_histogram(ui: &mut Ui, mean: f64, std: f64, min: f64, max: f64) {
        if (max - min).abs() < 1e-12 {
            ui.label("  数据范围不足，无法生成直方图");
            return;
        }

        // Simple 10-bin text histogram
        let n_bins = 10;
        let bin_width = (max - min) / n_bins as f64;

        // Generate synthetic bar heights based on normal distribution around mean
        let bars: Vec<String> = (0..n_bins)
            .map(|i| {
                let bin_center = min + (i as f64 + 0.5) * bin_width;
                // Normal PDF approximation for height
                let z = if std > 1e-12 {
                    (bin_center - mean) / std
                } else {
                    0.0
                };
                let height = (-0.5 * z * z).exp();
                let bar_len = (height * 30.0) as usize;
                "█".repeat(bar_len.max(1))
            })
            .collect();

        for (i, bar) in bars.iter().enumerate() {
            let bin_start = min + i as f64 * bin_width;
            let label = format!("{:>8.2} │ {}", bin_start, bar);
            ui.label(
                RichText::new(label)
                    .size(10.0)
                    .color(COLOR_TEXT_SECONDARY)
                    .monospace(),
            );
        }
    }

    /// Create a model based on the current model type and hyperparameters
    fn create_model(&self) -> Box<dyn AlphaModel> {
        match self.model_type {
            AlphaModelType::LinearRegression
            | AlphaModelType::Ridge
            | AlphaModelType::Lasso => {
                // Ridge and Lasso use the same LinearRegressionModel struct
                Box::new(LinearRegressionModel::new())
            }
            AlphaModelType::RandomForest => Box::new(RandomForestModel::with_params(
                self.hyper_params.n_estimators,
                Some(self.hyper_params.max_depth),
                2,
                self.hyper_params.seed,
            )),
            AlphaModelType::GradientBoosting => Box::new(GradientBoostingModel::new(
                self.hyper_params.n_estimators,
                self.hyper_params.learning_rate,
            )),
        }
    }

    /// Start model training with real AlphaLab engine
    fn start_training(&mut self) {
        self.training = true;
        self.status_message = format!("正在训练 {} 模型...", self.model_type.display_name());

        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.train_result = TrainResult {
                    model_name: self.model_type.display_name().to_string(),
                    n_features: 0,
                    n_samples: 0,
                    train_mse: 0.0,
                    valid_mse: 0.0,
                    detail: "错误: AlphaLab 引擎未连接，无法训练模型".to_string(),
                    trained: false,
                };
                self.training = false;
                self.status_message = "训练失败: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        let dataset_name = if self.dataset_name.is_empty() {
            self.train_result = TrainResult {
                model_name: self.model_type.display_name().to_string(),
                n_features: 0,
                n_samples: 0,
                train_mse: 0.0,
                valid_mse: 0.0,
                detail: "错误: 未选择数据集".to_string(),
                trained: false,
            };
            self.training = false;
            self.status_message = "训练失败: 未选择数据集".to_string();
            return;
        } else {
            self.dataset_name.clone()
        };

        // Acquire read lock and get dataset reference, perform training within lock scope
        let alpha_lab_read = match alpha_lab.read() {
            Ok(g) => g,
            Err(e) => {
                self.train_result = TrainResult {
                    model_name: self.model_type.display_name().to_string(),
                    n_features: 0,
                    n_samples: 0,
                    train_mse: 0.0,
                    valid_mse: 0.0,
                    detail: format!("错误: 无法获取 AlphaLab 读锁 — {}", e),
                    trained: false,
                };
                self.training = false;
                self.status_message = format!("训练失败: 无法获取 AlphaLab 读锁 — {}", e);
                return;
            }
        };

        let dataset = match alpha_lab_read.datasets.get(&dataset_name) {
            Some(ds) => ds,
            None => {
                self.train_result = TrainResult {
                    model_name: self.model_type.display_name().to_string(),
                    n_features: 0,
                    n_samples: 0,
                    train_mse: 0.0,
                    valid_mse: 0.0,
                    detail: format!("错误: 数据集 '{}' 不存在", dataset_name),
                    trained: false,
                };
                self.training = false;
                self.status_message = format!("训练失败: 数据集 '{}' 不存在", dataset_name);
                return;
            }
        };

        // Create and fit the model
        let mut model = self.create_model();
        model.fit(dataset);

        // Compute train MSE
        let train_mse = match model.predict(dataset, Segment::Train) {
            Ok(predictions) => {
                let actuals = Self::extract_labels(dataset, Segment::Train);
                Self::compute_mse(&predictions, &actuals)
            }
            Err(_) => f64::NAN,
        };

        // Compute validation MSE
        let valid_mse = match model.predict(dataset, Segment::Valid) {
            Ok(predictions) => {
                let actuals = Self::extract_labels(dataset, Segment::Valid);
                Self::compute_mse(&predictions, &actuals)
            }
            Err(_) => f64::NAN,
        };

        let model_detail = model.detail();
        let model_name = model.name().to_string();
        let (n_features, n_samples) = Self::extract_dataset_info(dataset);

        // Store model in AlphaLab — drop read lock, acquire write lock
        let storage_name = if self.model_name.is_empty() {
            format!("{}_{}", model_name, dataset_name)
        } else {
            self.model_name.clone()
        };

        // Release read lock before acquiring write lock
        drop(alpha_lab_read);

        {
            let mut lab_guard = match alpha_lab.write() {
                Ok(g) => g,
                Err(e) => {
                    self.train_result = TrainResult {
                        model_name: model_name.clone(),
                        n_features,
                        n_samples,
                        train_mse,
                        valid_mse,
                        detail: format!(
                            "{}\n警告: 模型训练成功但无法保存到 AlphaLab — {}",
                            model_detail, e
                        ),
                        trained: true,
                    };
                    self.training = false;
                    self.status_message = format!("{} 模型训练完成 (无法保存)", model_name);
                    return;
                }
            };
            lab_guard.models.insert(storage_name.clone(), model);
        }

        self.model_name = storage_name;

        self.train_result = TrainResult {
            model_name: model_name.clone(),
            n_features,
            n_samples,
            train_mse,
            valid_mse,
            detail: model_detail,
            trained: true,
        };

        self.training = false;
        self.status_message = format!("{} 模型训练完成", model_name);
    }

    /// Extract labels from a dataset segment
    fn extract_labels(dataset: &AlphaDataset, segment: Segment) -> Vec<f64> {
        match dataset.fetch_learn(segment) {
            Some(df) => {
                let height = df.height();
                match df.column("label") {
                    Ok(col) => match col.f64() {
                        Ok(ca) => ca.into_iter().map(|v| v.unwrap_or(0.0)).collect(),
                        Err(_) => vec![0.0; height],
                    },
                    Err(_) => vec![0.0; height],
                }
            }
            None => Vec::new(),
        }
    }

    /// Compute mean squared error between predictions and actuals
    fn compute_mse(predictions: &[f64], actuals: &[f64]) -> f64 {
        if predictions.len() != actuals.len() || predictions.is_empty() {
            return f64::NAN;
        }
        let n = predictions.len() as f64;
        let sum_sq: f64 = predictions
            .iter()
            .zip(actuals.iter())
            .map(|(p, a)| (p - a).powi(2))
            .sum();
        sum_sq / n
    }

    /// Extract feature count and sample count from dataset
    fn extract_dataset_info(dataset: &AlphaDataset) -> (usize, usize) {
        match dataset.fetch_learn(Segment::Train) {
            Some(df) => {
                let n_samples = df.height();
                let cols = df.get_column_names();
                let n_features = cols
                    .iter()
                    .filter(|c| **c != "datetime" && **c != "vt_symbol" && **c != "label")
                    .count();
                (n_features, n_samples)
            }
            None => (0, 0),
        }
    }

    /// Run factor analysis with real dataset data
    fn run_factor_analysis(&mut self) {
        let factor_name = if self.factor_state.selected_factor.is_empty() {
            "未选择".to_string()
        } else {
            self.factor_state.selected_factor.clone()
        };

        self.status_message = format!("正在分析因子: {}...", factor_name);

        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.factor_state.analysis_result = Some(FactorAnalysisResult {
                    factor_name: factor_name.clone(),
                    mean: 0.0,
                    std: 0.0,
                    min: 0.0,
                    max: 0.0,
                    ic: 0.0,
                    value_count: 0,
                });
                self.status_message = "分析失败: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        let dataset_name = if self.dataset_name.is_empty() {
            self.factor_state.analysis_result = Some(FactorAnalysisResult {
                factor_name: factor_name.clone(),
                mean: 0.0,
                std: 0.0,
                min: 0.0,
                max: 0.0,
                ic: 0.0,
                value_count: 0,
            });
            self.status_message = "分析失败: 未选择数据集".to_string();
            return;
        } else {
            self.dataset_name.clone()
        };

        let lab_guard = match alpha_lab.read() {
            Ok(g) => g,
            Err(e) => {
                self.factor_state.analysis_result = Some(FactorAnalysisResult {
                    factor_name: factor_name.clone(),
                    mean: 0.0,
                    std: 0.0,
                    min: 0.0,
                    max: 0.0,
                    ic: 0.0,
                    value_count: 0,
                });
                self.status_message = format!("分析失败: 无法获取 AlphaLab 读锁 — {}", e);
                return;
            }
        };

        let dataset = match lab_guard.datasets.get(&dataset_name) {
            Some(ds) => ds,
            None => {
                self.factor_state.analysis_result = Some(FactorAnalysisResult {
                    factor_name: factor_name.clone(),
                    mean: 0.0,
                    std: 0.0,
                    min: 0.0,
                    max: 0.0,
                    ic: 0.0,
                    value_count: 0,
                });
                self.status_message = format!("分析失败: 数据集 '{}' 不存在", dataset_name);
                return;
            }
        };

        // Get training data from dataset
        let df = match dataset.fetch_learn(Segment::Train) {
            Some(df) => df,
            None => {
                self.factor_state.analysis_result = Some(FactorAnalysisResult {
                    factor_name: factor_name.clone(),
                    mean: 0.0,
                    std: 0.0,
                    min: 0.0,
                    max: 0.0,
                    ic: 0.0,
                    value_count: 0,
                });
                self.status_message = "分析失败: 数据集无训练数据".to_string();
                return;
            }
        };

        // Extract the factor column
        let factor_values: Vec<f64> = match df.column(&factor_name) {
            Ok(col) => match col.f64() {
                Ok(ca) => ca
                    .into_iter()
                    .filter_map(|v| v)
                    .filter(|v| !v.is_nan())
                    .collect(),
                Err(_) => {
                    self.factor_state.analysis_result = Some(FactorAnalysisResult {
                        factor_name: factor_name.clone(),
                        mean: 0.0,
                        std: 0.0,
                        min: 0.0,
                        max: 0.0,
                        ic: 0.0,
                        value_count: 0,
                    });
                    self.status_message =
                        format!("分析失败: 因子 '{}' 数据类型非数值", factor_name);
                    return;
                }
            },
            Err(_) => {
                self.factor_state.analysis_result = Some(FactorAnalysisResult {
                    factor_name: factor_name.clone(),
                    mean: 0.0,
                    std: 0.0,
                    min: 0.0,
                    max: 0.0,
                    ic: 0.0,
                    value_count: 0,
                });
                self.status_message =
                    format!("分析失败: 因子 '{}' 在数据集中不存在", factor_name);
                return;
            }
        };

        if factor_values.is_empty() {
            self.factor_state.analysis_result = Some(FactorAnalysisResult {
                factor_name: factor_name.clone(),
                mean: 0.0,
                std: 0.0,
                min: 0.0,
                max: 0.0,
                ic: 0.0,
                value_count: 0,
            });
            self.status_message = format!("分析失败: 因子 '{}' 无有效数据", factor_name);
            return;
        }

        // Compute statistics
        let n = factor_values.len() as f64;
        let sum: f64 = factor_values.iter().sum();
        let mean = sum / n;

        let variance: f64 = if factor_values.len() > 1 {
            factor_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / (factor_values.len() - 1) as f64
        } else {
            0.0
        };
        let std = variance.sqrt();

        let min = factor_values
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let max = factor_values
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        // Compute IC as Pearson correlation between factor and label
        let ic = Self::compute_ic(&df, &factor_name);

        drop(lab_guard);

        self.factor_state.analysis_result = Some(FactorAnalysisResult {
            factor_name: factor_name.clone(),
            mean,
            std,
            min,
            max,
            ic,
            value_count: factor_values.len(),
        });

        self.status_message = format!("因子 {} 分析完成", factor_name);
    }

    /// Compute IC (Information Coefficient) as Pearson correlation between factor and label
    fn compute_ic(df: &polars::prelude::DataFrame, factor_name: &str) -> f64 {
        let factor_values: Vec<f64> = match df.column(factor_name) {
            Ok(col) => match col.f64() {
                Ok(ca) => ca
                    .into_iter()
                    .map(|v| v.unwrap_or(f64::NAN))
                    .collect(),
                Err(_) => return 0.0,
            },
            Err(_) => return 0.0,
        };

        let label_values: Vec<f64> = match df.column("label") {
            Ok(col) => match col.f64() {
                Ok(ca) => ca
                    .into_iter()
                    .map(|v| v.unwrap_or(f64::NAN))
                    .collect(),
                Err(_) => return 0.0,
            },
            Err(_) => return 0.0,
        };

        if factor_values.len() != label_values.len() || factor_values.len() < 2 {
            return 0.0;
        }

        // Pair-wise filter NaN values
        let pairs: Vec<(f64, f64)> = factor_values
            .iter()
            .zip(label_values.iter())
            .filter(|(f, l)| !f.is_nan() && !l.is_nan())
            .map(|(f, l)| (*f, *l))
            .collect();

        if pairs.len() < 2 {
            return 0.0;
        }

        let n = pairs.len() as f64;
        let f_mean: f64 = pairs.iter().map(|(f, _)| f).sum::<f64>() / n;
        let l_mean: f64 = pairs.iter().map(|(_, l)| l).sum::<f64>() / n;

        let cov: f64 = pairs
            .iter()
            .map(|(f, l)| (f - f_mean) * (l - l_mean))
            .sum::<f64>()
            / n;

        let f_std: f64 = {
            let var: f64 = pairs
                .iter()
                .map(|(f, _)| (f - f_mean).powi(2))
                .sum::<f64>()
                / n;
            var.sqrt()
        };

        let l_std: f64 = {
            let var: f64 = pairs
                .iter()
                .map(|(_, l)| (l - l_mean).powi(2))
                .sum::<f64>()
                / n;
            var.sqrt()
        };

        if f_std < 1e-12 || l_std < 1e-12 {
            return 0.0;
        }

        cov / (f_std * l_std)
    }

    /// Backtest alpha signal with real BacktestingEngine
    fn backtest_alpha_signal(&mut self) {
        self.status_message = "正在回测Alpha信号...".to_string();

        // Build portfolio weights from available signals
        let mut selected_models = Vec::new();
        let mut selected_factors = Vec::new();

        if self.train_result.trained {
            selected_models.push(self.train_result.model_name.clone());
        }
        if let Some(ref result) = self.factor_state.analysis_result {
            selected_factors.push(result.factor_name.clone());
        }

        let total = selected_models.len() + selected_factors.len();
        if total == 0 {
            self.portfolio_state.backtest_status = "请先训练模型或分析因子".to_string();
            self.status_message = "回测失败: 无可用信号".to_string();
            return;
        }

        // Equal weight allocation
        let weight = 1.0 / total as f64;
        let weights = vec![weight; total];

        self.portfolio_state.selected_models = selected_models;
        self.portfolio_state.selected_factors = selected_factors;
        self.portfolio_state.weights = weights;

        // Attempt real backtest via AlphaLab
        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.portfolio_state.backtest_status =
                    "AlphaLab 引擎未连接，回测结果仅供参考".to_string();
                self.status_message = "回测完成: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        let dataset_name = self.dataset_name.clone();
        let model_key = self.model_name.clone();

        let lab_guard = match alpha_lab.read() {
            Ok(g) => g,
            Err(e) => {
                self.portfolio_state.backtest_status =
                    format!("无法获取 AlphaLab 读锁 — {}", e);
                self.status_message = "回测失败: 无法获取 AlphaLab 读锁".to_string();
                return;
            }
        };

        // Check if trained model exists in AlphaLab
        let has_model = lab_guard.models.contains_key(&model_key);

        if !has_model || dataset_name.is_empty() {
            drop(lab_guard);
            self.portfolio_state.backtest_status =
                "请先训练模型并选择数据集以进行回测".to_string();
            self.status_message = "回测失败: 无已训练模型或未选择数据集".to_string();
            return;
        }

        // Get dataset for bar data
        let dataset = match lab_guard.datasets.get(&dataset_name) {
            Some(ds) => ds,
            None => {
                drop(lab_guard);
                self.portfolio_state.backtest_status =
                    format!("数据集 '{}' 不存在", dataset_name);
                self.status_message = format!("回测失败: 数据集 '{}' 不存在", dataset_name);
                return;
            }
        };

        // Extract bar data from dataset's raw DataFrame for backtesting
        let bars = Self::extract_bars_from_dataset(dataset);

        // Get model from AlphaLab (need write lock to take the model out,
        // but we can clone by re-fitting - instead, let's just run
        // backtest with a fresh model fitted on the same data)
        drop(lab_guard);

        // Create a fresh model for backtesting and fit it
        let mut bt_model = self.create_model();

        // Re-acquire read lock for dataset to fit model
        let lab_guard2 = match alpha_lab.read() {
            Ok(g) => g,
            Err(_) => {
                self.portfolio_state.backtest_status =
                    "回测失败: 无法重新获取 AlphaLab 读锁".to_string();
                self.status_message = "回测失败".to_string();
                return;
            }
        };

        let dataset2 = match lab_guard2.datasets.get(&dataset_name) {
            Some(ds) => ds,
            None => {
                self.portfolio_state.backtest_status =
                    "回测失败: 数据集丢失".to_string();
                self.status_message = "回测失败".to_string();
                return;
            }
        };

        bt_model.fit(dataset2);
        drop(lab_guard2);

        // Create and run BacktestingEngine
        let mut engine = BacktestingEngine::new();
        engine.add_model(bt_model);

        for bar in &bars {
            engine.add_data(&bar.vt_symbol(), vec![bar.clone()]);
        }

        engine.run_backtesting();
        engine.calculate_result();
        let stats = engine.calculate_statistics();

        let total_return = stats.get("total_return").copied().unwrap_or(0.0);
        let sharpe = stats.get("sharpe_ratio").copied().unwrap_or(0.0);
        let max_dd = stats.get("max_drawdown").copied().unwrap_or(0.0);
        let trade_count = stats.get("trade_count").copied().unwrap_or(0.0);
        let win_rate = stats.get("win_rate").copied().unwrap_or(0.0);

        self.portfolio_state.backtest_status = format!(
            "回测完成 — 总收益: {:.4}, 夏普比率: {:.4}, 最大回撤: {:.4}, 交易次数: {}, 胜率: {:.2}%",
            total_return,
            sharpe,
            max_dd,
            trade_count as u32,
            win_rate * 100.0,
        );

        self.status_message = "Alpha信号回测完成".to_string();
    }

    /// Extract AlphaBarData from dataset's raw DataFrame
    fn extract_bars_from_dataset(dataset: &AlphaDataset) -> Vec<crate::alpha::AlphaBarData> {
        let df = match dataset.raw_df.as_ref().or(dataset.learn_df.as_ref()) {
            Some(df) => df,
            None => return Vec::new(),
        };

        let height = df.height();
        let mut bars = Vec::with_capacity(height);

        let datetime_ca = df.column("datetime").ok().and_then(|c| c.datetime().ok());
        let symbol_ca = df.column("vt_symbol").ok().and_then(|c| c.str().ok());
        let open_ca = df.column("open").ok().and_then(|c| c.f64().ok());
        let high_ca = df.column("high").ok().and_then(|c| c.f64().ok());
        let low_ca = df.column("low").ok().and_then(|c| c.f64().ok());
        let close_ca = df.column("close").ok().and_then(|c| c.f64().ok());
        let volume_ca = df.column("volume").ok().and_then(|c| c.f64().ok());

        for i in 0..height {
            let datetime = datetime_ca
                .and_then(|ca| ca.get(i))
                .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
                .unwrap_or_else(chrono::Utc::now);

            let symbol = symbol_ca
                .and_then(|ca| ca.get(i))
                .unwrap_or("UNKNOWN")
                .to_string();

            let open = open_ca.and_then(|ca| ca.get(i)).unwrap_or(0.0);
            let high = high_ca.and_then(|ca| ca.get(i)).unwrap_or(0.0);
            let low = low_ca.and_then(|ca| ca.get(i)).unwrap_or(0.0);
            let close = close_ca.and_then(|ca| ca.get(i)).unwrap_or(0.0);
            let volume = volume_ca.and_then(|ca| ca.get(i)).unwrap_or(0.0);

            let vt_symbol = format!("{}.BINANCE", symbol);
            let parts: Vec<&str> = vt_symbol.split('.').collect();
            let sym = parts.first().unwrap_or(&"").to_string();

            bars.push(crate::alpha::AlphaBarData {
                datetime,
                symbol: sym,
                exchange: crate::trader::Exchange::Binance,
                interval: Some(crate::trader::Interval::Daily),
                open,
                high,
                low,
                close,
                volume,
                turnover: 0.0,
                open_interest: 0.0,
                gateway_name: "BACKTEST".to_string(),
            });
        }

        bars
    }

    /// Save current trained model to AlphaLab with the given name
    fn save_model(&mut self) {
        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.status_message = "保存失败: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        if self.model_name.is_empty() {
            self.status_message = "保存失败: 请输入模型名称".to_string();
            return;
        }

        if !self.train_result.trained {
            self.status_message = "保存失败: 无已训练模型".to_string();
            return;
        }

        // The model was already stored during start_training() with storage_name
        // Here we just verify it exists in AlphaLab
        let lab_guard = match alpha_lab.read() {
            Ok(g) => g,
            Err(e) => {
                self.status_message = format!("保存失败: 无法获取 AlphaLab 读锁 — {}", e);
                return;
            }
        };

        if lab_guard.models.contains_key(&self.model_name) {
            self.status_message = format!("模型 '{}' 已存在", self.model_name);
            return;
        }

        // If model with this exact name doesn't exist, we need to create one
        // Release read lock before acquiring write lock
        drop(lab_guard);

        let dataset_name = self.dataset_name.clone();
        let mut lab_guard = match alpha_lab.write() {
            Ok(g) => g,
            Err(e) => {
                self.status_message = format!("保存失败: 无法获取 AlphaLab 写锁 — {}", e);
                return;
            }
        };

        let dataset = match lab_guard.datasets.get(&dataset_name) {
            Some(ds) => ds,
            None => {
                self.status_message = format!("保存失败: 数据集 '{}' 不存在", dataset_name);
                return;
            }
        };

        let mut model = self.create_model();
        model.fit(dataset);
        lab_guard.models.insert(self.model_name.clone(), model);

        self.status_message = format!("模型 '{}' 保存成功", self.model_name);
    }

    /// Load a model from AlphaLab
    fn load_model(&mut self) {
        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.status_message = "加载失败: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        let model_name = match self.saved_models.get(self.selected_saved_model_idx) {
            Some(name) => name.clone(),
            None => {
                self.status_message = "加载失败: 未选择模型".to_string();
                return;
            }
        };

        let lab_guard = match alpha_lab.read() {
            Ok(g) => g,
            Err(e) => {
                self.status_message = format!("加载失败: 无法获取 AlphaLab 读锁 — {}", e);
                return;
            }
        };

        match lab_guard.models.get(&model_name) {
            Some(model) => {
                let name = model.name().to_string();
                let detail = model.detail();
                self.train_result = TrainResult {
                    model_name: name,
                    n_features: 0,
                    n_samples: 0,
                    train_mse: 0.0,
                    valid_mse: 0.0,
                    detail,
                    trained: true,
                };
                self.model_name = model_name.clone();
                self.status_message = format!("模型 '{}' 加载成功", model_name);
            }
            None => {
                self.status_message = format!("加载失败: 模型 '{}' 不存在", model_name);
            }
        }
    }

    /// Delete a model from AlphaLab
    fn delete_model(&mut self) {
        let alpha_lab = match self.alpha_lab {
            Some(ref lab) => lab.clone(),
            None => {
                self.status_message = "删除失败: AlphaLab 引擎未连接".to_string();
                return;
            }
        };

        let model_name = match self.saved_models.get(self.selected_saved_model_idx) {
            Some(name) => name.clone(),
            None => {
                self.status_message = "删除失败: 未选择模型".to_string();
                return;
            }
        };

        let mut lab_guard = match alpha_lab.write() {
            Ok(g) => g,
            Err(e) => {
                self.status_message = format!("删除失败: 无法获取 AlphaLab 写锁 — {}", e);
                return;
            }
        };

        if lab_guard.models.remove(&model_name).is_some() {
            if self.selected_saved_model_idx > 0 && self.selected_saved_model_idx >= lab_guard.models.len() {
                self.selected_saved_model_idx = lab_guard.models.len().saturating_sub(1);
            }
            self.status_message = format!("模型 '{}' 已删除", model_name);
        } else {
            self.status_message = format!("删除失败: 模型 '{}' 不存在", model_name);
        }
    }
}

impl Default for AlphaPanel {
    fn default() -> Self {
        Self::new()
    }
}
