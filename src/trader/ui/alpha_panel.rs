//! Alpha research GUI panel.
//!
//! Provides a quantitative research interface with:
//! - Model Training tab: select model type, configure hyperparameters, train
//! - Factor Analysis tab: select and analyze factors
//! - Alpha Portfolio tab: combine signals, view weights, backtest alpha

use egui::{RichText, Ui, Color32};

use super::style::{COLOR_TEXT_SECONDARY};

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
        }
    }

    /// Render the panel into the given Ui
    pub fn show(&mut self, ui: &mut Ui) {
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
                        &self.factor_state.analysis_result.as_ref().map_or_else(
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

    /// Start model training (stub: sets placeholder result)
    fn start_training(&mut self) {
        self.training = true;
        self.status_message = format!("正在训练 {} 模型...", self.model_type.display_name());

        // Stub: simulate training result
        self.train_result = TrainResult {
            model_name: self.model_type.display_name().to_string(),
            n_features: 0,
            n_samples: 0,
            train_mse: 0.0,
            valid_mse: 0.0,
            detail: format!(
                "Stub: {} 模型训练 — 需要连接 AlphaLab 引擎以获取真实结果",
                self.model_type.display_name()
            ),
            trained: true,
        };

        self.training = false;
        self.status_message = format!("{} 模型训练完成 (stub)", self.model_type.display_name());
    }

    /// Run factor analysis (stub: sets placeholder result)
    fn run_factor_analysis(&mut self) {
        let factor_name = if self.factor_state.selected_factor.is_empty() {
            "未选择".to_string()
        } else {
            self.factor_state.selected_factor.clone()
        };

        self.status_message = format!("正在分析因子: {}...", factor_name);

        // Stub: return placeholder analysis
        self.factor_state.analysis_result = Some(FactorAnalysisResult {
            factor_name: factor_name.clone(),
            mean: 0.0,
            std: 1.0,
            min: -3.0,
            max: 3.0,
            ic: 0.0,
            value_count: 0,
        });

        self.status_message = format!("因子 {} 分析完成 (stub)", factor_name);
    }

    /// Backtest alpha signal (stub)
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
        self.portfolio_state.backtest_status = format!(
            "Alpha信号回测完成 (stub) — {} 个模型 + {} 个因子, 等权分配",
            self.portfolio_state.selected_models.len(),
            self.portfolio_state.selected_factors.len()
        );

        self.status_message = "Alpha信号回测完成 (stub)".to_string();
    }
}

impl Default for AlphaPanel {
    fn default() -> Self {
        Self::new()
    }
}
