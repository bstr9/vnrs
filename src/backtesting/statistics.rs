//! Backtesting Statistics Calculation
//!
//! Calculate performance metrics from backtesting results

use super::base::{BacktestingStatistics, DailyResult};
use chrono::NaiveDate;
use std::collections::HashMap;

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

    let start_date = dates
        .first()
        .expect("dates non-empty checked above")
        .format("%Y-%m-%d")
        .to_string();
    let end_date = dates
        .last()
        .expect("dates non-empty checked above")
        .format("%Y-%m-%d")
        .to_string();

    // Accumulate metrics
    let mut total_net_pnl = 0.0;
    let mut total_commission = 0.0;
    let mut total_slippage = 0.0;
    let mut total_turnover = 0.0;
    let mut total_trade_count = 0;
    let mut profit_days = 0;
    let mut loss_days = 0;

    // GAP 3 additions: trade-level metrics
    let mut winning_trades_pnl: Vec<f64> = Vec::new();
    let mut losing_trades_pnl: Vec<f64> = Vec::new();
    let mut consecutive_wins = 0u32;
    let mut consecutive_losses = 0u32;
    let mut max_consecutive_wins = 0u32;
    let mut max_consecutive_losses = 0u32;

    let mut balance = start_capital;
    let mut max_balance = start_capital;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_percent = 0.0;

    let mut daily_returns: Vec<f64> = Vec::new();
    let mut negative_returns: Vec<f64> = Vec::new();  // For Sortino ratio

    for date in &dates {
        if let Some(result) = daily_results.get(date) {
            // Accumulate totals
            total_net_pnl += result.net_pnl;
            total_commission += result.commission;
            total_slippage += result.slippage;
            total_turnover += result.turnover;
            total_trade_count += result.trade_count;

            // Track trade-level PnL for win rate, profit factor, etc.
            for _trade in &result.trades {
                // Trade-level PnL tracking is approximated from daily net_pnl below
            }

            // Update balance
            balance += result.net_pnl;

            // Calculate return
            let prev_balance = balance - result.net_pnl;
            let daily_return = if prev_balance.abs() > 1e-10 {
                result.net_pnl / prev_balance
            } else {
                0.0
            };
            daily_returns.push(daily_return);
            
            // Track negative returns for Sortino
            if daily_return < 0.0 {
                negative_returns.push(daily_return);
            }

            // Count profit/loss days and track consecutive wins/losses
            if result.net_pnl > 0.0 {
                profit_days += 1;
                consecutive_wins += 1;
                consecutive_losses = 0;
                max_consecutive_wins = max_consecutive_wins.max(consecutive_wins);
                // Approximate winning trade PnL (distributed evenly)
                if result.trade_count > 0 {
                    let avg_trade = result.net_pnl / result.trade_count as f64;
                    winning_trades_pnl.push(avg_trade);
                }
            } else if result.net_pnl < 0.0 {
                loss_days += 1;
                consecutive_losses += 1;
                consecutive_wins = 0;
                max_consecutive_losses = max_consecutive_losses.max(consecutive_losses);
                // Approximate losing trade PnL (distributed evenly)
                if result.trade_count > 0 {
                    let avg_trade = result.net_pnl / result.trade_count as f64;
                    losing_trades_pnl.push(avg_trade);
                }
            }

            // Update max balance and drawdown
            if balance > max_balance {
                max_balance = balance;
            }

            let drawdown = max_balance - balance;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                max_drawdown_percent = if max_balance > 0.0 {
                    (drawdown / max_balance) * 100.0
                } else {
                    0.0
                };
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
            .sum::<f64>()
            / (daily_returns.len() - 1) as f64;
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

    // GAP 3: Calculate additional metrics
    let total_trades = winning_trades_pnl.len() + losing_trades_pnl.len();
    let win_rate = if total_trades > 0 {
        winning_trades_pnl.len() as f64 / total_trades as f64
    } else {
        0.0
    };

    let gross_profit: f64 = winning_trades_pnl.iter().sum();
    let gross_loss: f64 = losing_trades_pnl.iter().map(|x| x.abs()).sum();
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let avg_trade_pnl = if total_trades > 0 {
        (gross_profit - gross_loss) / total_trades as f64
    } else {
        0.0
    };

    let avg_winning_trade = if !winning_trades_pnl.is_empty() {
        gross_profit / winning_trades_pnl.len() as f64
    } else {
        0.0
    };

    let avg_losing_trade = if !losing_trades_pnl.is_empty() {
        gross_loss / losing_trades_pnl.len() as f64
    } else {
        0.0
    };

    let largest_winning_trade = winning_trades_pnl.iter().cloned().fold(0.0, f64::max);
    let largest_losing_trade = losing_trades_pnl.iter().cloned().map(|x| x.abs()).fold(0.0, f64::max);

    // Sortino ratio: uses downside deviation instead of total std
    let downside_std = if negative_returns.len() > 1 {
        let variance: f64 = negative_returns
            .iter()
            .map(|r| {
                let diff = r * r;  // Square of negative returns
                diff
            })
            .sum::<f64>()
            / negative_returns.len() as f64;
        variance.sqrt()
    } else {
        0.0
    };

    let sortino_ratio = if downside_std > 0.0 {
        let excess_return = daily_return_mean - risk_free / annual_days as f64;
        excess_return / downside_std * (annual_days as f64).sqrt()
    } else {
        0.0
    };

    // Calmar ratio: annual return / max drawdown
    let calmar_ratio = if max_drawdown > 0.0 {
        (return_mean * start_capital) / max_drawdown
    } else if return_mean > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

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
        win_rate,
        profit_factor,
        avg_trade_pnl,
        max_consecutive_wins,
        max_consecutive_losses,
        sortino_ratio,
        calmar_ratio,
        avg_winning_trade,
        avg_losing_trade,
        largest_winning_trade,
        largest_losing_trade,
    }
}

/// Calculate maximum drawdown from a series of balance values
pub fn calculate_max_drawdown(balances: &[f64]) -> (f64, f64) {
    if balances.is_empty() {
        return (0.0, 0.0);
    }

    let mut max_balance = balances.first().copied().unwrap_or(0.0);
    let mut max_drawdown = 0.0;
    let mut max_drawdown_percent = 0.0;

    if balances.len() <= 1 {
        return (0.0, 0.0);
    }

    for &balance in &balances[1..] {
        if balance > max_balance {
            max_balance = balance;
        }

        let drawdown = max_balance - balance;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
            max_drawdown_percent = if max_balance > 0.0 {
                (drawdown / max_balance) * 100.0
            } else {
                0.0
            };
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
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free: f64, annual_days: u32) -> f64 {
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
        .sum::<f64>()
        / (returns.len() - 1) as f64;

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

    #[test]
    fn test_calculate_statistics_empty() {
        let daily_results = HashMap::new();
        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);
        assert_eq!(stats.total_days, 0);
        assert_eq!(stats.profit_days, 0);
        assert_eq!(stats.loss_days, 0);
        assert!((stats.end_balance - 0.0).abs() < 1e-10);
        assert!((stats.sharpe_ratio - 0.0).abs() < 1e-10);
        assert!((stats.return_mean - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_statistics_single_day() {
        let mut daily_results = HashMap::new();
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut dr = DailyResult::new(date, 50000.0);
        dr.net_pnl = 1000.0;
        dr.commission = 10.0;
        dr.slippage = 5.0;
        dr.turnover = 50000.0;
        dr.trade_count = 2;
        daily_results.insert(date, dr);

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);

        assert_eq!(stats.total_days, 1);
        assert_eq!(stats.profit_days, 1);
        assert_eq!(stats.loss_days, 0);
        assert!((stats.total_net_pnl - 1000.0).abs() < 1e-10);
        assert!((stats.total_commission - 10.0).abs() < 1e-10);
        assert!((stats.total_slippage - 5.0).abs() < 1e-10);
        assert!((stats.end_balance - 101_000.0).abs() < 1e-10);
        assert_eq!(stats.start_date, "2024-01-15");
        assert_eq!(stats.end_date, "2024-01-15");
    }

    #[test]
    fn test_calculate_statistics_multiple_days() {
        let mut daily_results = HashMap::new();

        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut dr1 = DailyResult::new(d1, 50000.0);
        dr1.net_pnl = 1000.0;
        dr1.commission = 10.0;
        dr1.slippage = 2.0;
        dr1.turnover = 50000.0;
        dr1.trade_count = 2;
        daily_results.insert(d1, dr1);

        let d2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        let mut dr2 = DailyResult::new(d2, 51000.0);
        dr2.net_pnl = -500.0;
        dr2.commission = 8.0;
        dr2.slippage = 1.0;
        dr2.turnover = 40000.0;
        dr2.trade_count = 1;
        daily_results.insert(d2, dr2);

        let d3 = NaiveDate::from_ymd_opt(2024, 1, 17).unwrap();
        let mut dr3 = DailyResult::new(d3, 50500.0);
        dr3.net_pnl = 2000.0;
        dr3.commission = 15.0;
        dr3.slippage = 3.0;
        dr3.turnover = 80000.0;
        dr3.trade_count = 3;
        daily_results.insert(d3, dr3);

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);

        assert_eq!(stats.total_days, 3);
        assert_eq!(stats.profit_days, 2);
        assert_eq!(stats.loss_days, 1);
        assert!((stats.total_net_pnl - 2500.0).abs() < 1e-10);
        assert!((stats.total_commission - 33.0).abs() < 1e-10);
        assert!((stats.total_slippage - 6.0).abs() < 1e-10);
        assert!((stats.total_turnover - 170_000.0).abs() < 1e-10);
        assert_eq!(stats.total_trade_count, 6);
        assert!((stats.end_balance - 102_500.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_statistics_max_drawdown() {
        let mut daily_results = HashMap::new();

        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut dr1 = DailyResult::new(d1, 50000.0);
        dr1.net_pnl = 5000.0;
        daily_results.insert(d1, dr1);

        let d2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        let mut dr2 = DailyResult::new(d2, 55000.0);
        dr2.net_pnl = -8000.0;
        daily_results.insert(d2, dr2);

        let d3 = NaiveDate::from_ymd_opt(2024, 1, 17).unwrap();
        let mut dr3 = DailyResult::new(d3, 47000.0);
        dr3.net_pnl = 3000.0;
        daily_results.insert(d3, dr3);

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);

        // Balance: 100k -> 105k -> 97k -> 100k. Max DD from 105k to 97k = 8000
        assert!((stats.max_drawdown - 8000.0).abs() < 1e-10);
        assert!((stats.max_drawdown_percent - (8000.0 / 105_000.0 * 100.0)).abs() < 0.01);
    }

    #[test]
    fn test_calculate_statistics_annual_return() {
        let mut daily_results = HashMap::new();
        for i in 0..5 {
            let date = NaiveDate::from_ymd_opt(2024, 1, 15 + i as u32).unwrap();
            let mut dr = DailyResult::new(date, 50000.0);
            dr.net_pnl = 100.0;
            daily_results.insert(date, dr);
        }

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);

        // annual_return = daily_return_mean * 252
        // daily_return for each day is ~100/(100000+100*prev_day) ≈ 0.001
        assert!(stats.return_mean.is_finite());
        assert!(stats.return_mean > 0.0);
    }

    #[test]
    fn test_calculate_statistics_sharpe_ratio() {
        let mut daily_results = HashMap::new();
        let returns_data = [0.01, -0.005, 0.02, 0.015, -0.01, 0.005, 0.012];
        let mut balance = 100_000.0_f64;

        for (i, &ret) in returns_data.iter().enumerate() {
            let date = NaiveDate::from_ymd_opt(2024, 1, 15 + i as u32).unwrap();
            let pnl = balance * ret;
            let mut dr = DailyResult::new(date, 50000.0);
            dr.net_pnl = pnl;
            daily_results.insert(date, dr);
            balance += pnl;
        }

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.03, 252);
        assert!(stats.sharpe_ratio.is_finite());
    }

    #[test]
    fn test_calculate_max_drawdown_empty() {
        let (dd, dd_pct) = calculate_max_drawdown(&[]);
        assert!((dd - 0.0).abs() < 1e-10);
        assert!((dd_pct - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_max_drawdown_single() {
        let (dd, dd_pct) = calculate_max_drawdown(&[100.0]);
        assert!((dd - 0.0).abs() < 1e-10);
        assert!((dd_pct - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_max_drawdown_monotonic_increase() {
        let balances = vec![100.0, 110.0, 120.0, 130.0];
        let (dd, dd_pct) = calculate_max_drawdown(&balances);
        assert!((dd - 0.0).abs() < 1e-10);
        assert!((dd_pct - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_returns_empty() {
        let returns = calculate_returns(&[]);
        assert!(returns.is_empty());
    }

    #[test]
    fn test_calculate_returns_zero_balance() {
        let balances = vec![0.0, 100.0];
        let returns = calculate_returns(&balances);
        assert_eq!(returns.len(), 1);
        assert!((returns[0] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_sharpe_ratio_empty() {
        let sharpe = calculate_sharpe_ratio(&[], 0.0, 252);
        assert!((sharpe - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_sharpe_ratio_single() {
        let sharpe = calculate_sharpe_ratio(&[0.01], 0.0, 252);
        assert!((sharpe - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_sharpe_ratio_zero_std() {
        let sharpe = calculate_sharpe_ratio(&[0.01, 0.01, 0.01], 0.0, 252);
        assert!((sharpe - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_statistics_daily_averages() {
        let mut daily_results = HashMap::new();

        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut dr1 = DailyResult::new(d1, 50000.0);
        dr1.net_pnl = 100.0;
        dr1.commission = 5.0;
        dr1.slippage = 1.0;
        dr1.turnover = 10000.0;
        dr1.trade_count = 2;
        daily_results.insert(d1, dr1);

        let d2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        let mut dr2 = DailyResult::new(d2, 50100.0);
        dr2.net_pnl = 200.0;
        dr2.commission = 10.0;
        dr2.slippage = 2.0;
        dr2.turnover = 20000.0;
        dr2.trade_count = 4;
        daily_results.insert(d2, dr2);

        let stats = calculate_statistics(&daily_results, 100_000.0, 0.0, 252);

        assert!((stats.daily_net_pnl - 150.0).abs() < 1e-10);
        assert!((stats.daily_commission - 7.5).abs() < 1e-10);
        assert!((stats.daily_slippage - 1.5).abs() < 1e-10);
        assert!((stats.daily_turnover - 15000.0).abs() < 1e-10);
        assert!((stats.daily_trade_count - 3.0).abs() < 1e-10);
    }
}
