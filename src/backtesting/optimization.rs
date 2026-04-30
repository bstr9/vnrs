//! Parameter Optimization Module
//!
//! Provides genetic algorithm and grid search for strategy parameter optimization
//! Uses Rayon for parallel backtesting execution

use chrono::{DateTime, Duration, Utc};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::base::{BacktestingMode, BacktestingStatistics};
use super::engine::BacktestingEngine;
use crate::strategy::StrategyTemplate;
use crate::trader::{BarData, Interval, TickData};

/// Parameter definition for optimization
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub step: f64,
}

impl Parameter {
    pub fn new(name: &str, start: f64, end: f64, step: f64) -> Self {
        Self {
            name: name.to_string(),
            start,
            end,
            step,
        }
    }

    /// Get all possible values for this parameter
    pub fn get_values(&self) -> Vec<f64> {
        let mut values = Vec::new();
        let mut i = 0;
        loop {
            let value = self.start + f64::from(i) * self.step;
            if value > self.end + 1e-10 {
                break;
            }
            values.push(value);
            i += 1;
        }
        values
    }
}

/// Parameter combination for backtesting
pub type ParameterSet = HashMap<String, f64>;

/// Optimization result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub parameters: ParameterSet,
    pub statistics: BacktestingStatistics,
    pub target_value: f64,
}

/// Result of out-of-sample testing
#[derive(Debug, Clone)]
pub struct OutOfSampleResult {
    /// In-sample optimization results (best parameters on training data)
    pub training_result: OptimizationResult,
    /// Out-of-sample performance with those parameters
    pub testing_statistics: BacktestingStatistics,
    /// Ratio of out-of-sample to in-sample performance (1.0 = no degradation)
    pub degradation_ratio: f64,
    /// Whether the strategy passes out-of-sample validation
    pub passes_oos: bool,
}

/// Single window in walk-forward analysis
#[derive(Debug, Clone)]
pub struct WalkForwardWindow {
    /// Window index
    pub window_index: usize,
    /// Training period start
    pub train_start: DateTime<Utc>,
    /// Training period end
    pub train_end: DateTime<Utc>,
    /// Testing period start
    pub test_start: DateTime<Utc>,
    /// Testing period end
    pub test_end: DateTime<Utc>,
    /// Optimal parameters found during training
    pub optimal_parameters: ParameterSet,
    /// Training target value achieved
    pub train_target_value: f64,
    /// Testing target value achieved
    pub test_target_value: f64,
    /// Testing period statistics
    pub test_statistics: BacktestingStatistics,
}

/// Result of walk-forward analysis
#[derive(Debug, Clone)]
pub struct WalkForwardResult {
    /// Individual window results
    pub windows: Vec<WalkForwardWindow>,
    /// Aggregate test target value across all windows
    pub aggregate_test_target: f64,
    /// Walk-forward efficiency (aggregate_test / average_train_target)
    pub walk_forward_efficiency: f64,
    /// Whether the strategy passes walk-forward efficiency test
    pub passes_wfe: bool,
}

/// Stability info for a single parameter
#[derive(Debug, Clone)]
pub struct ParameterStabilityInfo {
    /// Optimal value of this parameter
    pub optimal_value: f64,
    /// Target value when parameter is decreased by one step
    pub minus_one_target: f64,
    /// Target value when parameter is increased by one step
    pub plus_one_target: f64,
    /// Sensitivity as percentage: (max - min) / optimal_target * 100
    pub sensitivity_percent: f64,
    /// Whether the parameter is stable (low sensitivity)
    pub is_stable: bool,
}

/// Parameter stability report
#[derive(Debug, Clone)]
pub struct ParameterStabilityReport {
    /// Optimal parameters that were tested
    pub optimal_parameters: ParameterSet,
    /// Target value at the optimal parameter set
    pub optimal_target_value: f64,
    /// Per-parameter stability info (keyed by parameter name)
    pub parameter_stability: HashMap<String, ParameterStabilityInfo>,
    /// Overall stability score (0.0 to 1.0)
    pub overall_stability_score: f64,
}

/// Optimization settings
pub struct OptimizationSettings {
    pub vt_symbol: String,
    pub interval: Interval,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub rate: f64,
    pub slippage: f64,
    pub size: f64,
    pub pricetick: f64,
    pub capital: f64,
    pub mode: BacktestingMode,
}

/// Parameter optimization engine
pub struct OptimizationEngine {
    settings: OptimizationSettings,
    parameters: Vec<Parameter>,
    history_data: Vec<BarData>,
    tick_data: Vec<TickData>,
    results: Arc<Mutex<Vec<OptimizationResult>>>,
}

impl OptimizationEngine {
    pub fn new(settings: OptimizationSettings) -> Self {
        Self {
            settings,
            parameters: Vec::new(),
            history_data: Vec::new(),
            tick_data: Vec::new(),
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add parameter for optimization
    pub fn add_parameter(&mut self, param: Parameter) {
        self.parameters.push(param);
    }

    /// Set historical data
    pub fn set_history_data(&mut self, data: Vec<BarData>) {
        self.history_data = data;
    }

    /// Set tick data
    pub fn set_tick_data(&mut self, data: Vec<TickData>) {
        self.tick_data = data;
    }

    /// Run grid search optimization
    pub fn run_grid_search<F>(
        &mut self,
        strategy_factory: F,
        target: OptimizationTarget,
    ) -> Vec<OptimizationResult>
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync + 'static,
    {
        // Generate all parameter combinations
        let combinations = self.generate_combinations();
        println!("生成{}组参数组合", combinations.len());

        // Clear previous results
        self.results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        // Parallel backtesting
        let factory = Arc::new(strategy_factory);
        let results = Arc::clone(&self.results);
        let settings = self.settings.clone();
        let history_data = self.history_data.clone();
        let tick_data = self.tick_data.clone();

        combinations.par_iter().for_each(|params| {
            // Create strategy with current parameters
            let strategy = factory(params);

            // Run backtesting
            let mut engine = BacktestingEngine::new();
            engine.set_parameters(
                settings.vt_symbol.clone(),
                settings.interval,
                settings.start,
                settings.end,
                settings.rate,
                settings.slippage,
                settings.size,
                settings.pricetick,
                settings.capital,
                settings.mode,
            );

            match settings.mode {
                BacktestingMode::Bar => engine.set_history_data(history_data.clone()),
                BacktestingMode::Tick => engine.set_tick_data(tick_data.clone()),
            }

            engine.add_strategy(strategy);

            // Run backtesting (blocking)
            let runtime = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for backtesting");
            if runtime.block_on(engine.run_backtesting()).is_ok() {
                let _result = engine.calculate_result();
                let stats = engine.calculate_statistics(false);
                let target_value = extract_target_value(&stats, &target);

                let result = OptimizationResult {
                    parameters: params.clone(),
                    statistics: stats,
                    target_value,
                };

                results
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(result);
            }
        });

        // Return sorted results
        let mut final_results = self
            .results
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        final_results.sort_by(|a, b| {
            b.target_value
                .partial_cmp(&a.target_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        final_results
    }

    /// Run genetic algorithm optimization
    pub fn run_genetic_algorithm<F>(
        &mut self,
        strategy_factory: F,
        target: OptimizationTarget,
        population_size: usize,
        generations: usize,
    ) -> Vec<OptimizationResult>
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync + 'static,
    {
        // Initialize population
        let mut population = self.generate_random_population(population_size);
        println!("初始化种群，大小: {population_size}");

        let factory = Arc::new(strategy_factory);

        for gen in 0..generations {
            println!("第 {} 代优化开始", gen + 1);

            // Evaluate fitness
            let fitness_scores = self.evaluate_population(&population, &factory, &target);

            // Select parents (tournament selection)
            let parents = self.select_parents(&population, &fitness_scores, population_size / 2);

            // Crossover and mutation
            let offspring = self.crossover_and_mutate(&parents);

            let offspring_fitness = self.evaluate_population(&offspring, &factory, &target);

            population = self.select_next_generation(
                &population,
                &offspring,
                &fitness_scores,
                &offspring_fitness,
                population_size,
            );

            // Print best result
            if let Some((_best_params, best_score)) = population
                .iter()
                .zip(fitness_scores.iter())
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            {
                println!("第 {} 代最优结果: {:.4}", gen + 1, best_score);
            }
        }

        // Final evaluation
        let factory_ref = Arc::clone(&factory);
        let final_results: Vec<_> = population
            .par_iter()
            .filter_map(|params| {
                let strategy = factory_ref(params);
                self.run_single_backtest(strategy, params, &target)
            })
            .collect();

        let mut sorted_results = final_results;
        sorted_results.sort_by(|a, b| {
            b.target_value
                .partial_cmp(&a.target_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted_results
    }

    /// Run out-of-sample test
    ///
    /// Splits historical data into training and testing portions,
    /// optimizes on training data, then validates on testing data.
    pub fn run_out_of_sample_test<F>(
        &mut self,
        strategy_factory: F,
        target: OptimizationTarget,
        train_ratio: f64,
    ) -> OutOfSampleResult
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync + 'static,
    {
        let split_index = (self.history_data.len() as f64 * train_ratio) as usize;
        let training_data: Vec<BarData> = self.history_data[..split_index].to_vec();
        let testing_data: Vec<BarData> = self.history_data[split_index..].to_vec();

        // Determine time boundary for split
        let split_time = self.history_data.get(split_index)
            .map(|b| b.datetime)
            .unwrap_or(self.settings.start);

        // Run grid search on training data using a sub-engine
        let factory = Arc::new(strategy_factory);

        let best_training = {
            let mut train_engine = OptimizationEngine::new(OptimizationSettings {
                vt_symbol: self.settings.vt_symbol.clone(),
                interval: self.settings.interval,
                start: self.settings.start,
                end: split_time,
                rate: self.settings.rate,
                slippage: self.settings.slippage,
                size: self.settings.size,
                pricetick: self.settings.pricetick,
                capital: self.settings.capital,
                mode: self.settings.mode,
            });

            for param in &self.parameters {
                train_engine.add_parameter(param.clone());
            }
            train_engine.set_history_data(training_data);

            let factory_ref = Arc::clone(&factory);
            let target_ref = target.clone();
            let results = train_engine.run_grid_search(
                move |params: &ParameterSet| factory_ref(params),
                target_ref,
            );

            results.first().cloned().unwrap_or_else(|| OptimizationResult {
                parameters: ParameterSet::new(),
                statistics: BacktestingStatistics::default(),
                target_value: 0.0,
            })
        };

        // Run backtest on testing data with best parameters
        let test_strategy = factory(&best_training.parameters);
        let testing_stats = {
            let mut test_engine = BacktestingEngine::new();
            test_engine.set_parameters(
                self.settings.vt_symbol.clone(),
                self.settings.interval,
                split_time,
                self.settings.end,
                self.settings.rate,
                self.settings.slippage,
                self.settings.size,
                self.settings.pricetick,
                self.settings.capital,
                self.settings.mode,
            );
            test_engine.set_history_data(testing_data);
            test_engine.add_strategy(test_strategy);

            let runtime = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = runtime {
                if rt.block_on(test_engine.run_backtesting()).is_ok() {
                    let _result = test_engine.calculate_result();
                    test_engine.calculate_statistics(false)
                } else {
                    BacktestingStatistics::default()
                }
            } else {
                BacktestingStatistics::default()
            }
        };

        let test_target = extract_target_value(&testing_stats, &target);
        let degradation_ratio = if best_training.target_value.abs() > 1e-10 {
            test_target / best_training.target_value
        } else {
            0.0
        };

        OutOfSampleResult {
            training_result: best_training,
            testing_statistics: testing_stats,
            degradation_ratio,
            passes_oos: degradation_ratio >= 0.5,
        }
    }

    /// Run walk-forward analysis
    ///
    /// Uses rolling windows with date-based boundaries. Each window has a training
    /// period followed by a testing period. The window rolls forward by `step_days`.
    #[allow(clippy::cast_possible_truncation)] // window sizes fit usize
    pub fn run_walk_forward_analysis<F>(
        &mut self,
        strategy_factory: F,
        target: OptimizationTarget,
        window_size_days: usize,
        test_size_days: usize,
        step_days: usize,
    ) -> WalkForwardResult
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync + 'static,
    {
        let total_len = self.history_data.len();
        if total_len == 0 || window_size_days == 0 || step_days == 0 {
            return WalkForwardResult {
                windows: Vec::new(),
                aggregate_test_target: 0.0,
                walk_forward_efficiency: 0.0,
                passes_wfe: false,
            };
        }

        let factory = Arc::new(strategy_factory);
        let target_clone = target.clone();

        // Determine date range from data
        let data_start = self.history_data.first().map(|b| b.datetime).unwrap_or(self.settings.start);
        let data_end = self.history_data.last().map(|b| b.datetime).unwrap_or(self.settings.end);

        let mut windows: Vec<WalkForwardWindow> = Vec::new();
        let mut window_index = 0usize;

        // Generate windows by stepping through dates
        let mut window_start = data_start;
        loop {
            let train_end = window_start + Duration::days(window_size_days as i64);
            let test_start = train_end;
            let test_end = test_start + Duration::days(test_size_days as i64);

            // Stop if the training period exceeds available data
            if train_end > data_end {
                break;
            }

            // Filter data for training and testing periods
            let training_data: Vec<BarData> = self.history_data.iter()
                .filter(|b| b.datetime >= window_start && b.datetime < train_end)
                .cloned()
                .collect();
            let testing_data: Vec<BarData> = self.history_data.iter()
                .filter(|b| b.datetime >= test_start && b.datetime < test_end)
                .cloned()
                .collect();

            if training_data.is_empty() || testing_data.is_empty() {
                window_start += Duration::days(step_days as i64);
                window_index += 1;
                continue;
            }

            // Optimize on training data using a sub-engine
            let best_training = {
                let mut train_engine = OptimizationEngine::new(OptimizationSettings {
                    vt_symbol: self.settings.vt_symbol.clone(),
                    interval: self.settings.interval,
                    start: window_start,
                    end: train_end,
                    rate: self.settings.rate,
                    slippage: self.settings.slippage,
                    size: self.settings.size,
                    pricetick: self.settings.pricetick,
                    capital: self.settings.capital,
                    mode: self.settings.mode,
                });

                for param in &self.parameters {
                    train_engine.add_parameter(param.clone());
                }
                train_engine.set_history_data(training_data);

                let factory_ref = Arc::clone(&factory);
                let target_ref = target_clone.clone();
                let results = train_engine.run_grid_search(
                    move |params: &ParameterSet| factory_ref(params),
                    target_ref,
                );

                results.first().cloned().unwrap_or_else(|| OptimizationResult {
                    parameters: ParameterSet::new(),
                    statistics: BacktestingStatistics::default(),
                    target_value: 0.0,
                })
            };

            // Test on testing data with best parameters
            let test_strategy = factory(&best_training.parameters);
            let test_stats = {
                let mut test_engine = BacktestingEngine::new();
                test_engine.set_parameters(
                    self.settings.vt_symbol.clone(),
                    self.settings.interval,
                    test_start,
                    test_end,
                    self.settings.rate,
                    self.settings.slippage,
                    self.settings.size,
                    self.settings.pricetick,
                    self.settings.capital,
                    self.settings.mode,
                );
                test_engine.set_history_data(testing_data);
                test_engine.add_strategy(test_strategy);

                let runtime = tokio::runtime::Runtime::new().ok();
                if let Some(rt) = runtime {
                    if rt.block_on(test_engine.run_backtesting()).is_ok() {
                        let _result = test_engine.calculate_result();
                        test_engine.calculate_statistics(false)
                    } else {
                        BacktestingStatistics::default()
                    }
                } else {
                    BacktestingStatistics::default()
                }
            };

            let test_target = extract_target_value(&test_stats, &target);

            windows.push(WalkForwardWindow {
                window_index,
                train_start: window_start,
                train_end,
                test_start,
                test_end,
                optimal_parameters: best_training.parameters,
                train_target_value: best_training.target_value,
                test_target_value: test_target,
                test_statistics: test_stats,
            });

            window_start += Duration::days(step_days as i64);
            window_index += 1;
        }

        // Compute aggregate metrics
        let aggregate_test_target = if windows.is_empty() {
            0.0
        } else {
            windows.iter().map(|w| w.test_target_value).sum::<f64>() / windows.len() as f64
        };

        let avg_train_target = if windows.is_empty() {
            0.0
        } else {
            windows.iter().map(|w| w.train_target_value).sum::<f64>() / windows.len() as f64
        };

        let walk_forward_efficiency = if avg_train_target.abs() > 1e-10 {
            aggregate_test_target / avg_train_target
        } else {
            0.0
        };

        WalkForwardResult {
            windows,
            aggregate_test_target,
            walk_forward_efficiency,
            passes_wfe: walk_forward_efficiency >= 0.5,
        }
    }

    /// Analyze parameter stability by perturbing each parameter ± step
    ///
    /// For each parameter, evaluates the target function with the parameter
    /// decreased by one step and increased by one step, computing sensitivity.
    pub fn analyze_parameter_stability<F>(
        &self,
        strategy_factory: F,
        target: &OptimizationTarget,
        optimal_parameters: &ParameterSet,
    ) -> ParameterStabilityReport
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync,
    {
        // Run baseline backtest with optimal parameters
        let optimal_target_value = {
            let strategy = strategy_factory(optimal_parameters);
            self.run_single_backtest_stats(strategy)
        };
        let optimal_target_value = extract_target_value(&optimal_target_value, target);

        let mut parameter_stability = HashMap::new();

        for param in &self.parameters {
            let name = &param.name;
            let optimal_val = match optimal_parameters.get(name) {
                Some(&v) => v,
                None => continue,
            };

            // Perturb minus one step
            let minus_val = (optimal_val - param.step).max(param.start);
            let mut minus_params = optimal_parameters.clone();
            minus_params.insert(name.clone(), minus_val);
            let minus_strategy = strategy_factory(&minus_params);
            let minus_stats = self.run_single_backtest_stats(minus_strategy);
            let minus_one_target = extract_target_value(&minus_stats, target);

            // Perturb plus one step
            let plus_val = (optimal_val + param.step).min(param.end);
            let mut plus_params = optimal_parameters.clone();
            plus_params.insert(name.clone(), plus_val);
            let plus_strategy = strategy_factory(&plus_params);
            let plus_stats = self.run_single_backtest_stats(plus_strategy);
            let plus_one_target = extract_target_value(&plus_stats, target);

            // Compute sensitivity: (max - min) / |optimal_target| * 100
            let min_target = minus_one_target.min(plus_one_target);
            let max_target = minus_one_target.max(plus_one_target);
            let sensitivity_percent = if optimal_target_value.abs() > 1e-10 {
                (max_target - min_target) / optimal_target_value.abs() * 100.0
            } else {
                0.0
            };

            // Stable if sensitivity is less than 20%
            let is_stable = sensitivity_percent < 20.0;

            parameter_stability.insert(name.clone(), ParameterStabilityInfo {
                optimal_value: optimal_val,
                minus_one_target,
                plus_one_target,
                sensitivity_percent,
                is_stable,
            });
        }

        // Overall stability score: fraction of stable parameters
        let total_params = parameter_stability.len();
        let stable_count = parameter_stability.values().filter(|info| info.is_stable).count();
        let overall_stability_score = if total_params > 0 {
            stable_count as f64 / total_params as f64
        } else {
            0.0
        };

        ParameterStabilityReport {
            optimal_parameters: optimal_parameters.clone(),
            optimal_target_value,
            parameter_stability,
            overall_stability_score,
        }
    }

    /// Run a single backtest and return only the statistics (no parameter tracking)
    fn run_single_backtest_stats(
        &self,
        strategy: Box<dyn StrategyTemplate>,
    ) -> BacktestingStatistics {
        let mut engine = BacktestingEngine::new();
        engine.set_parameters(
            self.settings.vt_symbol.clone(),
            self.settings.interval,
            self.settings.start,
            self.settings.end,
            self.settings.rate,
            self.settings.slippage,
            self.settings.size,
            self.settings.pricetick,
            self.settings.capital,
            self.settings.mode,
        );

        match self.settings.mode {
            BacktestingMode::Bar => engine.set_history_data(self.history_data.clone()),
            BacktestingMode::Tick => engine.set_tick_data(self.tick_data.clone()),
        }

        engine.add_strategy(strategy);

        let runtime = tokio::runtime::Runtime::new().ok();
        if let Some(rt) = runtime {
            if rt.block_on(engine.run_backtesting()).is_ok() {
                let _result = engine.calculate_result();
                return engine.calculate_statistics(false);
            }
        }

        BacktestingStatistics::default()
    }

    /// Generate all parameter combinations for grid search
    fn generate_combinations(&self) -> Vec<ParameterSet> {
        if self.parameters.is_empty() {
            return vec![HashMap::new()];
        }

        let mut combinations = vec![HashMap::new()];

        for param in &self.parameters {
            let values = param.get_values();
            let mut new_combinations = Vec::new();

            for combo in &combinations {
                for &value in &values {
                    let mut new_combo = combo.clone();
                    new_combo.insert(param.name.clone(), value);
                    new_combinations.push(new_combo);
                }
            }

            combinations = new_combinations;
        }

        combinations
    }

    /// Generate random population for genetic algorithm
    fn generate_random_population(&self, size: usize) -> Vec<ParameterSet> {
        use rand::Rng;
        let mut rng = rand::rng();

        (0..size)
            .map(|_| {
                self.parameters
                    .iter()
                    .map(|param| {
                        let range = param.end - param.start;
                        let value = param.start + rng.random::<f64>() * range;
                        // Align to step
                        let aligned =
                            ((value - param.start) / param.step).round() * param.step + param.start;
                        (param.name.clone(), aligned.min(param.end).max(param.start))
                    })
                    .collect()
            })
            .collect()
    }

    /// Evaluate population fitness
    fn evaluate_population<F>(
        &self,
        population: &[ParameterSet],
        factory: &Arc<F>,
        target: &OptimizationTarget,
    ) -> Vec<f64>
    where
        F: Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync,
    {
        population
            .par_iter()
            .map(|params| {
                let strategy = factory(params);
                if let Some(result) = self.run_single_backtest(strategy, params, target) {
                    result.target_value
                } else {
                    f64::MIN
                }
            })
            .collect()
    }

    /// Select parents using tournament selection
    fn select_parents(
        &self,
        population: &[ParameterSet],
        fitness: &[f64],
        count: usize,
    ) -> Vec<ParameterSet> {
        use rand::Rng;
        let mut rng = rand::rng();
        let mut parents = Vec::new();

        for _ in 0..count {
            let idx1 = rng.random_range(0..population.len());
            let idx2 = rng.random_range(0..population.len());

            if fitness[idx1] > fitness[idx2] {
                parents.push(population[idx1].clone());
            } else {
                parents.push(population[idx2].clone());
            }
        }

        parents
    }

    /// Crossover and mutation
    fn crossover_and_mutate(&self, parents: &[ParameterSet]) -> Vec<ParameterSet> {
        use rand::Rng;
        let mut rng = rand::rng();
        let mut offspring = Vec::new();

        for i in (0..parents.len()).step_by(2) {
            if i + 1 < parents.len() {
                let parent1 = &parents[i];
                let parent2 = &parents[i + 1];

                // Crossover
                let mut child1 = HashMap::new();
                let mut child2 = HashMap::new();

                for param in &self.parameters {
                    if rng.random::<f64>() < 0.5 {
                        child1.insert(param.name.clone(), parent1[&param.name]);
                        child2.insert(param.name.clone(), parent2[&param.name]);
                    } else {
                        child1.insert(param.name.clone(), parent2[&param.name]);
                        child2.insert(param.name.clone(), parent1[&param.name]);
                    }
                }

                // Mutation
                self.mutate(&mut child1, &mut rng);
                self.mutate(&mut child2, &mut rng);

                offspring.push(child1);
                offspring.push(child2);
            }
        }

        offspring
    }

    /// Mutate a parameter set
    fn mutate(&self, params: &mut ParameterSet, rng: &mut impl rand::Rng) {
        let mutation_rate = 0.1;

        for param in &self.parameters {
            if rng.random::<f64>() < mutation_rate {
                let values = param.get_values();
                if !values.is_empty() {
                    let new_value = values[rng.random_range(0..values.len())];
                    params.insert(param.name.clone(), new_value);
                }
            }
        }
    }

    /// Select next generation
    fn select_next_generation(
        &self,
        population: &[ParameterSet],
        offspring: &[ParameterSet],
        fitness: &[f64],
        offspring_fitness: &[f64],
        size: usize,
    ) -> Vec<ParameterSet> {
        let mut combined: Vec<_> = population
            .iter()
            .zip(fitness.iter())
            .map(|(p, f)| (p.clone(), *f))
            .collect();

        for (params, fit) in offspring.iter().zip(offspring_fitness.iter()) {
            combined.push((params.clone(), *fit));
        }

        combined.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N
        combined.into_iter().take(size).map(|(p, _)| p).collect()
    }

    /// Run single backtest
    fn run_single_backtest(
        &self,
        strategy: Box<dyn StrategyTemplate>,
        params: &ParameterSet,
        target: &OptimizationTarget,
    ) -> Option<OptimizationResult> {
        let mut engine = BacktestingEngine::new();
        engine.set_parameters(
            self.settings.vt_symbol.clone(),
            self.settings.interval,
            self.settings.start,
            self.settings.end,
            self.settings.rate,
            self.settings.slippage,
            self.settings.size,
            self.settings.pricetick,
            self.settings.capital,
            self.settings.mode,
        );

        match self.settings.mode {
            BacktestingMode::Bar => engine.set_history_data(self.history_data.clone()),
            BacktestingMode::Tick => engine.set_tick_data(self.tick_data.clone()),
        }

        engine.add_strategy(strategy);

        let runtime = tokio::runtime::Runtime::new().ok()?;
        runtime.block_on(engine.run_backtesting()).ok()?;
        let _result = engine.calculate_result();
        let stats = engine.calculate_statistics(false);

        let target_value = extract_target_value(&stats, target);

        Some(OptimizationResult {
            parameters: params.clone(),
            statistics: stats,
            target_value,
        })
    }
}

/// Optimization target
#[derive(Clone)]
pub enum OptimizationTarget {
    TotalReturn,
    SharpeRatio,
    MaxDrawdown,
    AnnualReturn,
    Custom(std::sync::Arc<dyn Fn(&BacktestingStatistics) -> f64 + Send + Sync>),
}

impl std::fmt::Debug for OptimizationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptimizationTarget::TotalReturn => write!(f, "TotalReturn"),
            OptimizationTarget::SharpeRatio => write!(f, "SharpeRatio"),
            OptimizationTarget::MaxDrawdown => write!(f, "MaxDrawdown"),
            OptimizationTarget::AnnualReturn => write!(f, "AnnualReturn"),
            OptimizationTarget::Custom(_) => write!(f, "Custom(<closure>)"),
        }
    }
}

/// Extract target value from statistics
fn extract_target_value(stats: &BacktestingStatistics, target: &OptimizationTarget) -> f64 {
    match target {
        OptimizationTarget::TotalReturn => {
            if stats.end_balance.abs() > 1e-10 {
                stats.total_net_pnl / stats.end_balance
            } else {
                0.0
            }
        }
        OptimizationTarget::SharpeRatio => stats.sharpe_ratio,
        OptimizationTarget::MaxDrawdown => -stats.max_drawdown_percent, // Minimize drawdown
        OptimizationTarget::AnnualReturn => stats.return_mean,
        OptimizationTarget::Custom(f) => f(stats),
    }
}

impl Clone for OptimizationSettings {
    fn clone(&self) -> Self {
        Self {
            vt_symbol: self.vt_symbol.clone(),
            interval: self.interval,
            start: self.start,
            end: self.end,
            rate: self.rate,
            slippage: self.slippage,
            size: self.size,
            pricetick: self.pricetick,
            capital: self.capital,
            mode: self.mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_optimization_target() {
        let target = OptimizationTarget::Custom(std::sync::Arc::new(|stats| {
            stats.total_net_pnl + stats.sharpe_ratio * 100.0
        }));

        let mut stats = BacktestingStatistics::default();
        stats.total_net_pnl = 5000.0;
        stats.sharpe_ratio = 1.5;

        let value = extract_target_value(&stats, &target);
        assert!((value - 5150.0).abs() < 1e-10);
    }

    #[test]
    fn test_custom_optimization_target_return_mean() {
        let target = OptimizationTarget::Custom(std::sync::Arc::new(|stats| stats.return_mean));

        let mut stats = BacktestingStatistics::default();
        stats.return_mean = 0.25;

        let value = extract_target_value(&stats, &target);
        assert!((value - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_parameter_get_values() {
        let param = Parameter::new("window", 1.0, 5.0, 1.0);
        let values = param.get_values();
        assert_eq!(values.len(), 5);
        assert!((values[0] - 1.0).abs() < 1e-10);
        assert!((values[4] - 5.0).abs() < 1e-10);

        // Test with non-integer step
        let param2 = Parameter::new("threshold", 0.1, 0.5, 0.1);
        let values2 = param2.get_values();
        assert_eq!(values2.len(), 5);
        assert!((values2[0] - 0.1).abs() < 1e-10);
        assert!((values2[4] - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_walk_forward_result_construction() {
        let window = WalkForwardWindow {
            window_index: 0,
            train_start: DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            train_end: DateTime::parse_from_rfc3339("2023-06-01T00:00:00Z")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            test_start: DateTime::parse_from_rfc3339("2023-06-01T00:00:00Z")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            test_end: DateTime::parse_from_rfc3339("2023-09-01T00:00:00Z")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            optimal_parameters: {
                let mut p = ParameterSet::new();
                p.insert("window".to_string(), 10.0);
                p
            },
            train_target_value: 1.5,
            test_target_value: 0.9,
            test_statistics: BacktestingStatistics::default(),
        };

        let result = WalkForwardResult {
            windows: vec![window],
            aggregate_test_target: 0.9,
            walk_forward_efficiency: 0.6,
            passes_wfe: true,
        };

        assert_eq!(result.windows.len(), 1);
        assert!((result.aggregate_test_target - 0.9).abs() < 1e-10);
        assert!(result.passes_wfe);
        assert!((result.windows[0].test_target_value - 0.9).abs() < 1e-10);
    }

    #[test]
    fn test_parameter_stability_report_construction() {
        // Test that ParameterStabilityReport and ParameterStabilityInfo can be constructed
        let mut optimal_params = ParameterSet::new();
        optimal_params.insert("fast".to_string(), 10.0);
        optimal_params.insert("slow".to_string(), 30.0);

        let mut stability = HashMap::new();
        stability.insert("fast".to_string(), ParameterStabilityInfo {
            optimal_value: 10.0,
            minus_one_target: 1.3,
            plus_one_target: 1.6,
            sensitivity_percent: 10.0,
            is_stable: true,
        });
        stability.insert("slow".to_string(), ParameterStabilityInfo {
            optimal_value: 30.0,
            minus_one_target: 1.45,
            plus_one_target: 1.55,
            sensitivity_percent: 3.3,
            is_stable: true,
        });

        let report = ParameterStabilityReport {
            optimal_parameters: optimal_params,
            optimal_target_value: 1.5,
            parameter_stability: stability,
            overall_stability_score: 1.0,
        };

        assert_eq!(report.parameter_stability.len(), 2);
        assert!(report.overall_stability_score > 0.0);
        assert!(report.parameter_stability.get("fast").unwrap().is_stable);
        assert!(report.parameter_stability.get("slow").unwrap().is_stable);
    }
}
