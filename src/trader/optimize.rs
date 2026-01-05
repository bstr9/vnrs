//! Optimization module for running parameter optimization.

use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

/// Output function type
pub type OutputFn = Box<dyn Fn(&str) + Send + Sync>;

/// Evaluate function type
pub type EvaluateFn = Box<dyn Fn(HashMap<String, f64>) -> HashMap<String, f64> + Send + Sync>;

/// Key function type for sorting results
pub type KeyFn = Box<dyn Fn(&HashMap<String, f64>) -> f64 + Send + Sync>;

/// Setting for running optimization
#[derive(Debug, Clone, Default)]
pub struct OptimizationSetting {
    pub params: HashMap<String, Vec<f64>>,
    pub target_name: String,
}

impl OptimizationSetting {
    /// Create a new optimization setting
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a parameter with range
    pub fn add_parameter(
        &mut self,
        name: &str,
        start: f64,
        end: Option<f64>,
        step: Option<f64>,
    ) -> Result<String, String> {
        match (end, step) {
            (None, _) | (_, None) => {
                self.params.insert(name.to_string(), vec![start]);
                Ok("固定参数添加成功".to_string())
            }
            (Some(end_val), Some(step_val)) => {
                if start >= end_val {
                    return Err("参数优化起始点必须小于终止点".to_string());
                }
                if step_val <= 0.0 {
                    return Err("参数优化步进必须大于0".to_string());
                }

                let mut value = start;
                let mut value_list = Vec::new();

                while value <= end_val {
                    value_list.push(value);
                    value += step_val;
                }

                let count = value_list.len();
                self.params.insert(name.to_string(), value_list);
                Ok(format!("范围参数添加成功，数量{}", count))
            }
        }
    }

    /// Set the target for optimization
    pub fn set_target(&mut self, target_name: &str) {
        self.target_name = target_name.to_string();
    }

    /// Generate all parameter combinations
    pub fn generate_settings(&self) -> Vec<HashMap<String, f64>> {
        let keys: Vec<&String> = self.params.keys().collect();
        let values: Vec<&Vec<f64>> = self.params.values().collect();

        if keys.is_empty() {
            return vec![];
        }

        // Calculate total combinations
        let mut indices = vec![0usize; keys.len()];
        let mut settings = Vec::new();

        loop {
            // Create current setting
            let mut setting = HashMap::new();
            for (i, key) in keys.iter().enumerate() {
                setting.insert((*key).clone(), values[i][indices[i]]);
            }
            settings.push(setting);

            // Increment indices
            let mut carry = true;
            for i in (0..indices.len()).rev() {
                if carry {
                    indices[i] += 1;
                    if indices[i] >= values[i].len() {
                        indices[i] = 0;
                    } else {
                        carry = false;
                    }
                }
            }

            // Check if we've cycled through all combinations
            if carry {
                break;
            }
        }

        settings
    }

    /// Count the total number of parameter combinations
    pub fn count_settings(&self) -> usize {
        self.params.values().map(|v| v.len()).product()
    }
}

/// Check if optimization setting is valid
pub fn check_optimization_setting(setting: &OptimizationSetting) -> Result<(), String> {
    let count = setting.count_settings();

    if count == 0 {
        return Err("优化参数组合为空，请检查".to_string());
    }

    if count > 100000 {
        tracing::warn!(
            "警告：参数组合数量过大（{}组），可能导致内存不足",
            count
        );
        tracing::warn!("建议：1) 减小参数范围 2) 增大步进 3) 使用遗传算法优化");
    }

    if setting.target_name.is_empty() {
        return Err("优化目标未设置，请检查".to_string());
    }

    Ok(())
}

/// Run brute force optimization
pub fn run_bf_optimization<F, K>(
    evaluate_func: F,
    optimization_setting: &OptimizationSetting,
    key_func: K,
) -> Vec<HashMap<String, f64>>
where
    F: Fn(HashMap<String, f64>) -> HashMap<String, f64> + Send + Sync,
    K: Fn(&HashMap<String, f64>) -> f64 + Send + Sync,
{
    let count = optimization_setting.count_settings();
    tracing::info!("开始执行穷举算法优化");
    tracing::info!("参数优化空间：{}", count);

    let start = Instant::now();

    let settings = optimization_setting.generate_settings();
    let mut results: Vec<HashMap<String, f64>> = settings
        .into_par_iter()
        .map(|setting| evaluate_func(setting))
        .collect();

    // Sort by key function
    results.sort_by(|a, b| {
        key_func(b)
            .partial_cmp(&key_func(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let cost = start.elapsed().as_secs();
    tracing::info!("穷举算法优化完成，耗时{}秒", cost);

    results
}

/// Genetic algorithm individual
#[derive(Debug, Clone)]
pub struct Individual {
    pub genes: Vec<(String, f64)>,
    pub fitness: f64,
}

impl Individual {
    pub fn new(genes: Vec<(String, f64)>) -> Self {
        Self {
            genes,
            fitness: 0.0,
        }
    }

    pub fn to_setting(&self) -> HashMap<String, f64> {
        self.genes.iter().cloned().collect()
    }
}

/// Run genetic algorithm optimization
pub fn run_ga_optimization<F, K>(
    evaluate_func: F,
    optimization_setting: &OptimizationSetting,
    key_func: K,
    pop_size: usize,
    ngen: usize,
    cxpb: f64,
    mutpb: f64,
) -> Vec<HashMap<String, f64>>
where
    F: Fn(HashMap<String, f64>) -> HashMap<String, f64> + Send + Sync,
    K: Fn(&HashMap<String, f64>) -> f64 + Send + Sync,
{
    use rand::prelude::*;

    let param_names: Vec<&String> = optimization_setting.params.keys().collect();
    let param_ranges: Vec<&Vec<f64>> = optimization_setting.params.values().collect();

    let total_size = optimization_setting.count_settings();
    let mu = (pop_size as f64 * 0.8) as usize;

    tracing::info!("开始执行遗传算法优化");
    tracing::info!("参数优化空间：{}", total_size);
    tracing::info!("每代族群总数：{}", pop_size);
    tracing::info!("优良筛选个数：{}", mu);
    tracing::info!("迭代次数：{}", ngen);
    tracing::info!("交叉概率：{:.0}%", cxpb * 100.0);
    tracing::info!("突变概率：{:.0}%", mutpb * 100.0);

    let start = Instant::now();
    let mut rng = rand::thread_rng();

    // Initialize population
    let mut population: Vec<Individual> = (0..pop_size)
        .map(|_| {
            let genes: Vec<(String, f64)> = param_names
                .iter()
                .zip(param_ranges.iter())
                .map(|(name, values)| {
                    let value = values[rng.gen_range(0..values.len())];
                    ((*name).clone(), value)
                })
                .collect();
            Individual::new(genes)
        })
        .collect();

    // Cache for evaluated results - use String key since f64 doesn't implement Hash
    fn genes_to_key(genes: &[(String, f64)]) -> String {
        genes
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }
    let mut cache: HashMap<String, HashMap<String, f64>> = HashMap::new();

    // Evolution loop
    for gen in 0..ngen {
        // Evaluate fitness
        for individual in &mut population {
            let cache_key = genes_to_key(&individual.genes);
            let setting = if let Some(result) = cache.get(&cache_key) {
                result.clone()
            } else {
                let result = evaluate_func(individual.to_setting());
                cache.insert(cache_key, result.clone());
                result
            };
            individual.fitness = key_func(&setting);
        }

        // Sort by fitness
        population.sort_by(|a, b| {
            b.fitness
                .partial_cmp(&a.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Select top individuals
        let mut new_population: Vec<Individual> = population[..mu].to_vec();

        // Generate offspring through crossover and mutation
        while new_population.len() < pop_size {
            let parent1 = &population[rng.gen_range(0..mu)];
            let parent2 = &population[rng.gen_range(0..mu)];

            let mut child_genes = parent1.genes.clone();

            // Crossover
            if rng.gen::<f64>() < cxpb {
                let crossover_point = rng.gen_range(0..child_genes.len());
                for i in crossover_point..child_genes.len() {
                    child_genes[i] = parent2.genes[i].clone();
                }
            }

            // Mutation
            if rng.gen::<f64>() < mutpb {
                let mutation_point = rng.gen_range(0..child_genes.len());
                let param_values = param_ranges[mutation_point];
                child_genes[mutation_point].1 = param_values[rng.gen_range(0..param_values.len())];
            }

            new_population.push(Individual::new(child_genes));
        }

        population = new_population;
        tracing::debug!("Generation {} completed", gen + 1);
    }

    // Final evaluation and sorting
    for individual in &mut population {
        let cache_key = genes_to_key(&individual.genes);
        let setting = if let Some(result) = cache.get(&cache_key) {
            result.clone()
        } else {
            evaluate_func(individual.to_setting())
        };
        individual.fitness = key_func(&setting);
    }

    population.sort_by(|a, b| {
        b.fitness
            .partial_cmp(&a.fitness)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let cost = start.elapsed().as_secs();
    tracing::info!("遗传算法优化完成，耗时{}秒", cost);

    // Return cached results
    cache.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimization_setting() {
        let mut setting = OptimizationSetting::new();
        
        setting.add_parameter("param1", 1.0, Some(5.0), Some(1.0)).unwrap();
        setting.add_parameter("param2", 10.0, Some(20.0), Some(5.0)).unwrap();
        
        assert_eq!(setting.count_settings(), 15); // 5 * 3
        
        let settings = setting.generate_settings();
        assert_eq!(settings.len(), 15);
    }

    #[test]
    fn test_optimization_setting_fixed() {
        let mut setting = OptimizationSetting::new();
        
        setting.add_parameter("param1", 5.0, None, None).unwrap();
        
        assert_eq!(setting.count_settings(), 1);
    }

    #[test]
    fn test_optimization_setting_invalid() {
        let mut setting = OptimizationSetting::new();
        
        // Start >= end
        let result = setting.add_parameter("param1", 10.0, Some(5.0), Some(1.0));
        assert!(result.is_err());
        
        // Step <= 0
        let result = setting.add_parameter("param1", 1.0, Some(5.0), Some(-1.0));
        assert!(result.is_err());
    }
}
