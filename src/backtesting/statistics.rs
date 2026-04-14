//! Backtesting Statistics Calculation
//! 
//! Calculate performance metrics from backtesting results

use std::collections::HashMap;
use chrono::NaiveDate;
use super::base::{DailyResult, BacktestingStatistics};

/// Calculate comprehensive backtesting statistics
pub fn calculate_statistics(
    daily_results: &HashMap<NaiveDate, DailyResult>,
    start_capital: f64,
    risk_free: f64,
    annual_days: u32,
) -> BacktestingStatistics {
    if daily_results.is_empty() {
        return BacktestingStatistics::default();
    }

    // Get sorted dates
    let mut dates: Vec<&NaiveDate> = daily_results.keys().collect();
    dates.sort();
    
    let start_date = dates.first().unwrap().format("%Y-%m-%d").to_string();
    let end_date = dates.last().unwrap().format("%Y-%m-%d").to_string();

    // Accumulate metrics
    let mut total_net_pnl = 0.0;
    let mut total_commission = 0.0;
    let mut total_slippage = 0.0;
    let mut total_turnover = 0.0;
    let mut total_trade_count = 0;
    let mut profit_days = 0;
    let mut loss_days = 0;

    let mut balance = start_capital;
    let mut max_balance = start_capital;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_percent = 0.0;

    let mut daily_returns: Vec<f64> = Vec::new();

    for date in &dates {
        if let Some(result) = daily_results.get(date) {
            // Accumulate totals
            total_net_pnl += result.net_pnl;
            total_commission += result.commission;
            total_slippage += result.slippage;
            total_turnover += result.turnover;
            total_trade_count += result.trade_count;

            // Update balance
            balance += result.net_pnl;

            // Calculate return
            let daily_return = if balance > 0.0 {
                result.net_pnl / (balance - result.net_pnl)
            } else {
                0.0
            };
            daily_returns.push(daily_return);

            // Count profit/loss days
            if result.net_pnl > 0.0 {
                profit_days += 1;
            } else if result.net_pnl < 0.0 {
                loss_days += 1;
            }

            // Update max balance and drawdown
            if balance > max_balance {
                max_balance = balance;
            }

            let drawdown = max_balance - balance;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                max_drawdown_percent = (drawdown / max_balance) * 100.0;
            }
        }
    }

    let total_days = dates.len() as u32;
    let end_balance = balance;

    // Calculate daily averages
    let daily_net_pnl = if total_days > 0 {
        total_net_pnl / total_days as f64
    } else {
        0.0
    };

    let daily_commission = if total_days > 0 {
        total_commission / total_days as f64
    } else {
        0.0
    };

    let daily_slippage = if total_days > 0 {
        total_slippage / total_days as f64
    } else {
        0.0
    };

    let daily_turnover = if total_days > 0 {
        total_turnover / total_days as f64
    } else {
        0.0
    };

    let daily_trade_count = if total_days > 0 {
        total_trade_count as f64 / total_days as f64
    } else {
        0.0
    };

    // Calculate return statistics
    let daily_return_mean = if !daily_returns.is_empty() {
        daily_returns.iter().sum::<f64>() / daily_returns.len() as f64
    } else {
        0.0
    };

    let return_std = if daily_returns.len() > 1 {
        let variance: f64 = daily_returns
            .iter()
            .map(|r| {
                let diff = r - daily_return_mean;
                diff * diff
            })
            .sum::<f64>() / (daily_returns.len() - 1) as f64;
        variance.sqrt()
    } else {
        0.0
    };

    // Calculate Sharpe ratio
    let sharpe_ratio = if return_std > 0.0 {
        let excess_return = daily_return_mean - risk_free / annual_days as f64;
        excess_return / return_std * (annual_days as f64).sqrt()
    } else {
        0.0
    };

    // Annual return
    let return_mean = daily_return_mean * annual_days as f64;

    BacktestingStatistics {
        start_date,
        end_date,
        total_days,
        profit_days,
        loss_days,
        end_balance,
        max_drawdown,
        max_drawdown_percent,
        total_net_pnl,
        total_commission,
        total_slippage,
        total_turnover,
        total_trade_count,
        daily_net_pnl,
        daily_commission,
        daily_slippage,
        daily_turnover,
        daily_trade_count,
        daily_return: daily_return_mean,
        return_std,
        sharpe_ratio,
        return_mean,
    }
}

/// Calculate maximum drawdown from a series of balance values
pub fn calculate_max_drawdown(balances: &[f64]) -> (f64, f64) {
    let mut max_balance = 0.0;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_percent = 0.0;

    for &balance in balances {
        if balance > max_balance {
            max_balance = balance;
        }

        let drawdown = max_balance - balance;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
            if max_balance > 0.0 {
                max_drawdown_percent = (drawdown / max_balance) * 100.0;
            }
        }
    }

    (max_drawdown, max_drawdown_percent)
}

/// Calculate daily returns from balance series
pub fn calculate_returns(balances: &[f64]) -> Vec<f64> {
    let mut returns = Vec::new();

    for i in 1..balances.len() {
        if balances[i - 1] > 0.0 {
            let ret = (balances[i] - balances[i - 1]) / balances[i - 1];
            returns.push(ret);
        } else {
            returns.push(0.0);
        }
    }

    returns
}

/// Calculate Sharpe ratio from returns
pub fn calculate_sharpe_ratio(
    returns: &[f64],
    risk_free: f64,
    annual_days: u32,
) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;

    if returns.len() < 2 {
        return 0.0;
    }

    let variance: f64 = returns
        .iter()
        .map(|r| {
            let diff = r - mean_return;
            diff * diff
        })
        .sum::<f64>() / (returns.len() - 1) as f64;

    let std_return = variance.sqrt();

    if std_return == 0.0 {
        return 0.0;
    }

    let daily_risk_free = risk_free / annual_days as f64;
    let excess_return = mean_return - daily_risk_free;

    excess_return / std_return * (annual_days as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_max_drawdown() {
        let balances = vec![100.0, 110.0, 105.0, 120.0, 90.0, 95.0];
        let (max_dd, max_dd_pct) = calculate_max_drawdown(&balances);
        assert_eq!(max_dd, 30.0); // From 120 to 90
        assert!((max_dd_pct - 25.0).abs() < 0.01); // 30/120 = 25%
    }

    #[test]
    fn test_calculate_returns() {
        let balances = vec![100.0, 110.0, 105.0, 120.0];
        let returns = calculate_returns(&balances);
        assert_eq!(returns.len(), 3);
        assert!((returns[0] - 0.1).abs() < 0.001); // 10% return
        assert!((returns[1] + 0.0454545).abs() < 0.001); // -4.545% return
        assert!((returns[2] - 0.142857).abs() < 0.001); // 14.286% return
    }

    #[test]
    fn test_calculate_sharpe_ratio() {
        let returns = vec![0.01, -0.005, 0.02, 0.015, -0.01];
        let sharpe = calculate_sharpe_ratio(&returns, 0.03, 252);
        assert!(sharpe.is_finite());
    }
}
