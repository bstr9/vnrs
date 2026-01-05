//! Parameter Optimization Module
//! 
//! Provides genetic algorithm and grid search for strategy parameter optimization
//! Uses Rayon for parallel backtesting execution

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;
use chrono::{DateTime, Utc};

use crate::trader::{BarData, TickData, Exchange, Interval};
use crate::strategy::StrategyTemplate;
use super::engine::BacktestingEngine;
use super::base::{BacktestingMode, BacktestingStatistics};

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
        let mut current = self.start;
        while current <= self.end {
            values.push(current);
            current += self.step;
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
    pub fn run_grid_search<F>(&mut self, 
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
        self.results.lock().unwrap().clear();

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
                settings.mode.clone(),
            );

            match settings.mode {
                BacktestingMode::Bar => engine.set_history_data(history_data.clone()),
                BacktestingMode::Tick => engine.set_tick_data(tick_data.clone()),
            }

            engine.add_strategy(strategy);

            // Run backtesting (blocking)
            let runtime = tokio::runtime::Runtime::new().unwrap();
            if runtime.block_on(engine.run_backtesting()).is_ok() {
                let result = engine.calculate_result();
                let stats = engine.calculate_statistics(false);
                let target_value = extract_target_value(&stats, &target);
                
                let result = OptimizationResult {
                    parameters: params.clone(),
                    statistics: stats,
                    target_value,
                };
                
                results.lock().unwrap().push(result);
            }
        });

        // Return sorted results
        let mut final_results = self.results.lock().unwrap().clone();
        final_results.sort_by(|a, b| b.target_value.partial_cmp(&a.target_value).unwrap());
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
        println!("初始化种群，大小: {}", population_size);

        let factory = Arc::new(strategy_factory);

        for gen in 0..generations {
            println!("第 {} 代优化开始", gen + 1);

            // Evaluate fitness
            let fitness_scores = self.evaluate_population(&population, &factory, &target);

            // Select parents (tournament selection)
            let parents = self.select_parents(&population, &fitness_scores, population_size / 2);

            // Crossover and mutation
            let offspring = self.crossover_and_mutate(&parents);

            // Create new population
            population = self.select_next_generation(&population, &offspring, &fitness_scores, population_size);

            // Print best result
            if let Some((best_params, best_score)) = population.iter()
                .zip(fitness_scores.iter())
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            {
                println!("第 {} 代最优结果: {:.4}", gen + 1, best_score);
            }
        }

        // Final evaluation
        let factory_ref = Arc::clone(&factory);
        let final_results: Vec<_> = population.par_iter()
            .filter_map(|params| {
                let strategy = factory_ref(params);
                self.run_single_backtest(strategy, params, &target)
            })
            .collect();

        let mut sorted_results = final_results;
        sorted_results.sort_by(|a, b| b.target_value.partial_cmp(&a.target_value).unwrap());
        sorted_results
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
                self.parameters.iter()
                    .map(|param| {
                        let range = param.end - param.start;
                        let value = param.start + rng.random::<f64>() * range;
                        // Align to step
                        let aligned = ((value - param.start) / param.step).round() * param.step + param.start;
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
        population.par_iter()
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
        size: usize,
    ) -> Vec<ParameterSet> {
        let mut combined: Vec<_> = population.iter()
            .zip(fitness.iter())
            .map(|(p, f)| (p.clone(), *f))
            .collect();

        // Evaluate offspring
        let offspring_fitness: Vec<_> = offspring.iter()
            .map(|params| {
                // Quick estimate, could be parallelized
                0.0 // Placeholder
            })
            .collect();

        for (params, fitness) in offspring.iter().zip(offspring_fitness.iter()) {
            combined.push((params.clone(), *fitness));
        }

        // Sort by fitness
        combined.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap());

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
            self.settings.mode.clone(),
        );

        match self.settings.mode {
            BacktestingMode::Bar => engine.set_history_data(self.history_data.clone()),
            BacktestingMode::Tick => engine.set_tick_data(self.tick_data.clone()),
        }

        engine.add_strategy(strategy);

        let runtime = tokio::runtime::Runtime::new().ok()?;
        runtime.block_on(engine.run_backtesting()).ok()?;
        let result = engine.calculate_result();
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
#[derive(Debug, Clone)]
pub enum OptimizationTarget {
    TotalReturn,
    SharpeRatio,
    MaxDrawdown,
    AnnualReturn,
    Custom(String),
}

/// Extract target value from statistics
fn extract_target_value(stats: &BacktestingStatistics, target: &OptimizationTarget) -> f64 {
    match target {
        OptimizationTarget::TotalReturn => stats.total_net_pnl / stats.end_balance,
        OptimizationTarget::SharpeRatio => stats.sharpe_ratio,
        OptimizationTarget::MaxDrawdown => -stats.max_drawdown_percent, // Minimize drawdown
        OptimizationTarget::AnnualReturn => stats.return_mean * 252.0, // Approx annual
        OptimizationTarget::Custom(_) => 0.0, // Placeholder
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
            mode: self.mode.clone(),
        }
    }
}
