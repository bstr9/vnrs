//! Trading Report & Analytics Module
//!
//! Computes and exports professional-grade trading performance metrics
//! from both live and backtested strategies.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::constant::{Direction, Offset};
use super::engine::OmsEngine;
use crate::backtesting::base::{BacktestingResult, DailyResult};
use crate::backtesting::statistics::{
    calculate_max_drawdown, calculate_returns, calculate_sharpe_ratio,
};

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

/// A single trade record for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: String,
    pub strategy_name: String,
    pub vt_symbol: String,
    pub direction: Direction,
    pub offset: Offset,
    pub price: f64,
    pub volume: f64,
    pub trade_time: DateTime<Utc>,
    pub commission: f64,
    pub slippage: f64,
    pub realized_pnl: f64,
}

/// Daily summary for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: NaiveDate,
    pub start_balance: f64,
    pub end_balance: f64,
    pub net_pnl: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub commission: f64,
    pub slippage: f64,
    pub trade_count: u32,
    pub turnover: f64,
    pub max_drawdown: f64,
    pub daily_return: f64,
}

/// Equity curve data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub datetime: DateTime<Utc>,
    pub equity: f64,
    pub benchmark_equity: f64,
}

/// Per-strategy report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyReport {
    pub strategy_name: String,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_pnl: f64,
    pub trade_count: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    /// Per-symbol breakdown: vt_symbol -> SymbolPnl
    pub symbol_breakdown: HashMap<String, SymbolPnl>,
}

/// Per-symbol PnL breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolPnl {
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub trade_count: u32,
    pub avg_entry_price: f64,
}

/// Comprehensive trading report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingReport {
    // Time range
    pub start_date: String,
    pub end_date: String,
    pub total_days: u32,

    // Capital
    pub start_capital: f64,
    pub end_capital: f64,

    // Returns
    pub total_return: f64,
    pub annual_return: f64,
    pub daily_return_mean: f64,
    pub daily_return_std: f64,

    // Risk metrics
    pub max_drawdown: f64,
    pub max_drawdown_percent: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,

    // Trade metrics
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_trade_pnl: f64,
    pub avg_winning_trade: f64,
    pub avg_losing_trade: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
    pub max_consecutive_wins: u32,
    pub max_consecutive_losses: u32,

    /// Per-strategy breakdown
    pub strategy_reports: HashMap<String, StrategyReport>,

    // Time series
    pub daily_summaries: Vec<DailySummary>,
    pub equity_curve: Vec<EquityPoint>,
}

/// Strategy PnL data passed from StrategyEngine without tight coupling
#[derive(Debug, Clone, Default)]
pub struct StrategyPnlData {
    /// Realized PnL per strategy
    pub strategy_pnl: HashMap<String, f64>,
    /// Unrealized PnL per (strategy, symbol)
    pub strategy_unrealized_pnl: HashMap<(String, String), f64>,
    /// Realized PnL per (strategy, symbol)
    pub strategy_pnl_by_symbol: HashMap<(String, String), f64>,
    /// Trade count per strategy
    pub strategy_trade_count: HashMap<String, usize>,
    /// Average entry price per (strategy, symbol)
    pub strategy_avg_price: HashMap<(String, String), f64>,
}
// ---------------------------------------------------------------------------
// ReportEngine
// ---------------------------------------------------------------------------

/// Engine for generating trading reports from live or backtested data
pub struct ReportEngine {
    /// Reference to OmsEngine for live data
    #[allow(dead_code)]
    oms_engine: Option<Arc<OmsEngine>>,
    /// Start capital for computing returns
    start_capital: f64,
    /// Risk-free rate for Sharpe/Sortino
    risk_free_rate: f64,
    /// Annual trading days
    annual_days: u32,
    /// Daily balance snapshots: date -> balance
    daily_balances: Arc<RwLock<HashMap<NaiveDate, f64>>>,
    /// Trade log
    trade_log: Arc<RwLock<Vec<TradeRecord>>>,
    /// Equity curve
    equity_curve: Arc<RwLock<Vec<EquityPoint>>>,
    /// Benchmark initial price (BTC close at start)
    benchmark_start_price: Arc<RwLock<Option<f64>>>,
}

impl ReportEngine {
    /// Create a new ReportEngine with the given start capital
    pub fn new(start_capital: f64) -> Self {
        Self {
            oms_engine: None,
            start_capital,
            risk_free_rate: 0.0,
            annual_days: 252,
            daily_balances: Arc::new(RwLock::new(HashMap::new())),
            trade_log: Arc::new(RwLock::new(Vec::new())),
            equity_curve: Arc::new(RwLock::new(Vec::new())),
            benchmark_start_price: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a ReportEngine connected to an OmsEngine for live data
    pub fn with_oms(oms_engine: Arc<OmsEngine>, start_capital: f64) -> Self {
        Self {
            oms_engine: Some(oms_engine),
            start_capital,
            risk_free_rate: 0.0,
            annual_days: 252,
            daily_balances: Arc::new(RwLock::new(HashMap::new())),
            trade_log: Arc::new(RwLock::new(Vec::new())),
            equity_curve: Arc::new(RwLock::new(Vec::new())),
            benchmark_start_price: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the risk-free rate (builder pattern)
    pub fn risk_free_rate(mut self, rate: f64) -> Self {
        self.risk_free_rate = rate;
        self
    }

    /// Set the annual trading days (builder pattern)
    pub fn annual_days(mut self, days: u32) -> Self {
        self.annual_days = days;
        self
    }

    /// Record a trade for reporting
    pub fn record_trade(&self, trade: TradeRecord) {
        let mut log = self
            .trade_log
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        log.push(trade);
    }

    /// Record daily balance snapshot
    pub fn record_daily_balance(&self, date: NaiveDate, balance: f64) {
        let mut balances = self
            .daily_balances
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        balances.insert(date, balance);
    }

    /// Record equity curve point with optional benchmark price
    pub fn record_equity(
        &self,
        datetime: DateTime<Utc>,
        equity: f64,
        benchmark_price: Option<f64>,
    ) {
        // Set benchmark start price on first call, then compute benchmark equity
        let benchmark_equity = if let Some(price) = benchmark_price {
            let mut start_price = self
                .benchmark_start_price
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if start_price.is_none() {
                *start_price = Some(price);
            }
            let sp = start_price.unwrap_or(price);
            if sp > 0.0 {
                self.start_capital * (price / sp)
            } else {
                equity
            }
        } else {
            equity
        };

        let mut curve = self
            .equity_curve
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        curve.push(EquityPoint {
            datetime,
            equity,
            benchmark_equity,
        });
    }
    /// Generate full trading report from accumulated data + strategy PnL
    #[allow(clippy::too_many_lines)]
    pub fn generate_report(&self, strategy_pnl_data: &StrategyPnlData) -> TradingReport {
        let trades = self
            .trade_log
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let daily_balances = self
            .daily_balances
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let equity_curve = self
            .equity_curve
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Sort dates
        let mut dates: Vec<NaiveDate> = daily_balances.keys().copied().collect();
        dates.sort();

        let start_date = dates
            .first()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let end_date = dates
            .last()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let total_days = dates.len() as u32;

        // Build balance series
        let balances: Vec<f64> = dates
            .iter()
            .map(|d| *daily_balances.get(d).unwrap_or(&self.start_capital))
            .collect();

        let end_capital = balances.last().copied().unwrap_or(self.start_capital);
        let total_return = if self.start_capital > 0.0 {
            (end_capital - self.start_capital) / self.start_capital
        } else {
            0.0
        };

        let daily_returns = calculate_returns(&balances);
        let daily_return_mean = mean(&daily_returns);
        let daily_return_std = std_dev(&daily_returns);
        let annual_return = daily_return_mean * self.annual_days as f64;

        let (max_drawdown, max_drawdown_percent) = calculate_max_drawdown(&balances);
        let sharpe_ratio =
            calculate_sharpe_ratio(&daily_returns, self.risk_free_rate, self.annual_days);
        let sortino_ratio = compute_sortino(
            &daily_returns,
            daily_return_mean,
            self.risk_free_rate,
            self.annual_days,
        );
        let calmar_ratio = compute_calmar(annual_return, max_drawdown, self.start_capital);

        // Trade-level metrics
        let (winning_pnls, losing_pnls) = partition_trades(&trades);
        let (max_consecutive_wins, max_consecutive_losses) = consecutive_wins_losses(&trades);

        let total_trades = trades.len() as u32;
        let winning_trades = winning_pnls.len() as u32;
        let losing_trades = losing_pnls.len() as u32;
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let gross_profit: f64 = winning_pnls.iter().sum();
        let gross_loss: f64 = losing_pnls.iter().map(|x| x.abs()).sum();
        let pf = profit_factor(gross_profit, gross_loss);
        let avg_trade_pnl = if total_trades > 0 {
            (gross_profit - gross_loss) / total_trades as f64
        } else {
            0.0
        };
        let avg_winning_trade = avg_of(&winning_pnls);
        let avg_losing_trade = avg_abs_of(&losing_pnls);
        let largest_win = winning_pnls.iter().cloned().fold(0.0, f64::max);
        let largest_loss = losing_pnls.iter().map(|x| x.abs()).fold(0.0, f64::max);

        let strategy_reports = self.build_strategy_reports(strategy_pnl_data);
        let daily_summaries =
            self.build_daily_summaries(&dates, &daily_balances, &daily_returns, &balances);

        TradingReport {
            start_date,
            end_date,
            total_days,
            start_capital: self.start_capital,
            end_capital,
            total_return,
            annual_return,
            daily_return_mean,
            daily_return_std,
            max_drawdown,
            max_drawdown_percent,
            sharpe_ratio,
            sortino_ratio,
            calmar_ratio,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            profit_factor: pf,
            avg_trade_pnl,
            avg_winning_trade,
            avg_losing_trade,
            largest_win,
            largest_loss,
            max_consecutive_wins,
            max_consecutive_losses,
            strategy_reports,
            daily_summaries,
            equity_curve: equity_curve.clone(),
        }
    }
    /// Generate report from backtesting results
    #[allow(clippy::too_many_lines)]
    pub fn generate_report_from_backtest(&self, result: &BacktestingResult) -> TradingReport {
        let mut dates: Vec<&NaiveDate> = result.daily_results.keys().collect();
        dates.sort();

        let start_date = dates
            .first()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let end_date = dates
            .last()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let total_days = dates.len() as u32;

        // Build balance series from daily results
        let mut balance = result.start_capital;
        let mut balances: Vec<f64> = Vec::with_capacity(dates.len());
        for date in &dates {
            if let Some(dr) = result.daily_results.get(date) {
                balance += dr.net_pnl;
            }
            balances.push(balance);
        }

        let end_capital = balance;
        let total_return = if result.start_capital > 0.0 {
            (end_capital - result.start_capital) / result.start_capital
        } else {
            0.0
        };

        let daily_returns = calculate_returns(&balances);
        let daily_return_mean = mean(&daily_returns);
        let daily_return_std = std_dev(&daily_returns);
        let annual_return = daily_return_mean * self.annual_days as f64;

        let (max_drawdown, max_drawdown_percent) = calculate_max_drawdown(&balances);
        let sharpe_ratio =
            calculate_sharpe_ratio(&daily_returns, self.risk_free_rate, self.annual_days);
        let sortino_ratio = compute_sortino(
            &daily_returns,
            daily_return_mean,
            self.risk_free_rate,
            self.annual_days,
        );
        let calmar_ratio = compute_calmar(annual_return, max_drawdown, result.start_capital);

        // Trade-level metrics from daily results
        let mut winning_pnls: Vec<f64> = Vec::new();
        let mut losing_pnls: Vec<f64> = Vec::new();
        let mut cw = 0u32;
        let mut cl = 0u32;
        let mut mcw = 0u32;
        let mut mcl = 0u32;
        let mut total_trades = 0u32;

        for date in &dates {
            if let Some(dr) = result.daily_results.get(date) {
                total_trades += dr.trade_count;

                if dr.net_pnl > 0.0 {
                    cw += 1;
                    cl = 0;
                    mcw = mcw.max(cw);
                    if dr.trade_count > 0 {
                        winning_pnls.push(dr.net_pnl / dr.trade_count as f64);
                    }
                } else if dr.net_pnl < 0.0 {
                    cl += 1;
                    cw = 0;
                    mcl = mcl.max(cl);
                    if dr.trade_count > 0 {
                        losing_pnls.push(dr.net_pnl / dr.trade_count as f64);
                    }
                }
            }
        }

        let winning_trades = winning_pnls.len() as u32;
        let losing_trades = losing_pnls.len() as u32;
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        let gross_profit: f64 = winning_pnls.iter().sum();
        let gross_loss: f64 = losing_pnls.iter().map(|x| x.abs()).sum();
        let pf = profit_factor(gross_profit, gross_loss);
        let avg_trade_pnl = if total_trades > 0 {
            (gross_profit - gross_loss) / total_trades as f64
        } else {
            0.0
        };
        let avg_winning_trade = avg_of(&winning_pnls);
        let avg_losing_trade = avg_abs_of(&losing_pnls);
        let largest_win = winning_pnls.iter().cloned().fold(0.0, f64::max);
        let largest_loss = losing_pnls.iter().map(|x| x.abs()).fold(0.0, f64::max);

        let daily_summaries = self.build_daily_summaries_from_backtest(
            &dates,
            &result.daily_results,
            &balances,
            &daily_returns,
        );

        // Build equity curve from daily balances (no benchmark for backtest)
        let equity_curve: Vec<EquityPoint> = dates
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let dt = d
                    .and_hms_opt(23, 59, 59)
                    .map_or_else(Utc::now, |nd| {
                        DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc)
                    });
                let eq = balances.get(i).copied().unwrap_or(result.start_capital);
                EquityPoint {
                    datetime: dt,
                    equity: eq,
                    benchmark_equity: eq,
                }
            })
            .collect();

        TradingReport {
            start_date,
            end_date,
            total_days,
            start_capital: result.start_capital,
            end_capital,
            total_return,
            annual_return,
            daily_return_mean,
            daily_return_std,
            max_drawdown,
            max_drawdown_percent,
            sharpe_ratio,
            sortino_ratio,
            calmar_ratio,
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            profit_factor: pf,
            avg_trade_pnl,
            avg_winning_trade,
            avg_losing_trade,
            largest_win,
            largest_loss,
            max_consecutive_wins: mcw,
            max_consecutive_losses: mcl,
            strategy_reports: HashMap::new(),
            daily_summaries,
            equity_curve,
        }
    }
    /// Export trade log to CSV
    pub fn export_trades_csv(&self, path: &Path) -> Result<(), String> {
        let trades = self
            .trade_log
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut file =
            std::fs::File::create(path).map_err(|e| format!("Failed to create trades CSV: {e}"))?;

        writeln!(
            file,
            "trade_id,strategy,symbol,direction,offset,price,volume,time,commission,slippage,realized_pnl"
        )
        .map_err(|e| format!("Failed to write CSV header: {e}"))?;

        for trade in trades.iter() {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&trade.trade_id),
                csv_escape(&trade.strategy_name),
                csv_escape(&trade.vt_symbol),
                trade.direction,
                trade.offset,
                trade.price,
                trade.volume,
                trade.trade_time.to_rfc3339(),
                trade.commission,
                trade.slippage,
                trade.realized_pnl,
            )
            .map_err(|e| format!("Failed to write trade row: {e}"))?;
        }

        Ok(())
    }

    /// Export daily summary to CSV
    pub fn export_daily_csv(
        &self,
        report: &TradingReport,
        path: &Path,
    ) -> Result<(), String> {
        let mut file =
            std::fs::File::create(path).map_err(|e| format!("Failed to create daily CSV: {e}"))?;

        writeln!(
            file,
            "date,start_balance,end_balance,net_pnl,realized_pnl,unrealized_pnl,commission,trade_count,turnover,daily_return"
        )
        .map_err(|e| format!("Failed to write CSV header: {e}"))?;

        for summary in &report.daily_summaries {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{}",
                summary.date,
                summary.start_balance,
                summary.end_balance,
                summary.net_pnl,
                summary.realized_pnl,
                summary.unrealized_pnl,
                summary.commission,
                summary.trade_count,
                summary.turnover,
                summary.daily_return,
            )
            .map_err(|e| format!("Failed to write daily row: {e}"))?;
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Build per-strategy reports from StrategyPnlData
    fn build_strategy_reports(&self, data: &StrategyPnlData) -> HashMap<String, StrategyReport> {
        let mut reports = HashMap::new();

        // Collect all strategy names
        let mut all_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in data.strategy_pnl.keys() {
            all_names.insert(name.clone());
        }
        for (name, _sym) in data.strategy_unrealized_pnl.keys() {
            all_names.insert(name.clone());
        }

        for name in all_names {
            let realized = data.strategy_pnl.get(&name).copied().unwrap_or(0.0);
            // Aggregate unrealized PnL across all symbols for this strategy
            let unrealized: f64 = data
                .strategy_unrealized_pnl
                .iter()
                .filter(|((s, _), _)| s == &name)
                .map(|(_, &v)| v)
                .sum();
            let trade_count = data.strategy_trade_count.get(&name).copied().unwrap_or(0) as u32;
            let total_pnl = realized + unrealized;

            // Per-symbol breakdown
            let mut symbol_breakdown = HashMap::new();
            for ((s, sym), &pnl) in &data.strategy_pnl_by_symbol {
                if s == &name {
                    let key = (name.clone(), sym.clone());
                    let unrealized_sym = data
                        .strategy_unrealized_pnl
                        .get(&key)
                        .copied()
                        .unwrap_or(0.0);
                    let avg_price = data
                        .strategy_avg_price
                        .get(&key)
                        .copied()
                        .unwrap_or(0.0);
                    symbol_breakdown.insert(
                        sym.clone(),
                        SymbolPnl {
                            realized_pnl: pnl,
                            unrealized_pnl: unrealized_sym,
                            trade_count,
                            avg_entry_price: avg_price,
                        },
                    );
                }
            }

            reports.insert(
                name.clone(),
                StrategyReport {
                    strategy_name: name,
                    realized_pnl: realized,
                    unrealized_pnl: unrealized,
                    total_pnl,
                    trade_count,
                    win_rate: 0.0,
                    profit_factor: 0.0,
                    max_drawdown: 0.0,
                    symbol_breakdown,
                },
            );
        }

        reports
    }
    /// Build daily summaries from accumulated balance snapshots
    fn build_daily_summaries(
        &self,
        dates: &[NaiveDate],
        daily_balances: &HashMap<NaiveDate, f64>,
        daily_returns: &[f64],
        balances: &[f64],
    ) -> Vec<DailySummary> {
        let mut summaries = Vec::with_capacity(dates.len());

        for (i, &date) in dates.iter().enumerate() {
            let end_balance = daily_balances.get(&date).copied().unwrap_or(self.start_capital);
            let start_balance = if i > 0 {
                balances.get(i - 1).copied().unwrap_or(self.start_capital)
            } else {
                self.start_capital
            };
            let net_pnl = end_balance - start_balance;
            let daily_return = daily_returns.get(i).copied().unwrap_or(0.0);

            let running_balances = &balances[..=i];
            let (dd, _) = calculate_max_drawdown(running_balances);

            summaries.push(DailySummary {
                date,
                start_balance,
                end_balance,
                net_pnl,
                realized_pnl: net_pnl,
                unrealized_pnl: 0.0,
                commission: 0.0,
                slippage: 0.0,
                trade_count: 0,
                turnover: 0.0,
                max_drawdown: dd,
                daily_return,
            });
        }

        summaries
    }

    /// Build daily summaries from backtesting daily results
    fn build_daily_summaries_from_backtest(
        &self,
        dates: &[&NaiveDate],
        daily_results: &HashMap<NaiveDate, DailyResult>,
        balances: &[f64],
        daily_returns: &[f64],
    ) -> Vec<DailySummary> {
        let mut summaries = Vec::with_capacity(dates.len());

        for (i, date) in dates.iter().enumerate() {
            let dr = daily_results.get(date);
            let end_balance = balances.get(i).copied().unwrap_or(self.start_capital);
            let start_balance = if i > 0 {
                balances.get(i - 1).copied().unwrap_or(self.start_capital)
            } else {
                self.start_capital
            };
            let net_pnl = dr.map_or(0.0, |d| d.net_pnl);
            let daily_return = daily_returns.get(i).copied().unwrap_or(0.0);

            let running_balances = &balances[..=i];
            let (dd, _) = calculate_max_drawdown(running_balances);

            summaries.push(DailySummary {
                date: **date,
                start_balance,
                end_balance,
                net_pnl,
                realized_pnl: dr.map_or(0.0, |d| d.trading_pnl),
                unrealized_pnl: dr.map_or(0.0, |d| d.holding_pnl),
                commission: dr.map_or(0.0, |d| d.commission),
                slippage: dr.map_or(0.0, |d| d.slippage),
                trade_count: dr.map_or(0, |d| d.trade_count),
                turnover: dr.map_or(0.0, |d| d.turnover),
                max_drawdown: dd,
                daily_return,
            });
        }

        summaries
    }
}

// ---------------------------------------------------------------------------
// Free helper functions
// ---------------------------------------------------------------------------

/// Compute mean of a slice of f64
fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

/// Compute sample standard deviation
fn std_dev(data: &[f64]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let m = mean(data);
    let variance = data.iter().map(|r| (r - m) * (r - m)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

/// Compute Sortino ratio
fn compute_sortino(returns: &[f64], mean_ret: f64, risk_free: f64, annual_days: u32) -> f64 {
    let neg: Vec<f64> = returns.iter().copied().filter(|r| *r < 0.0).collect();
    if neg.len() < 2 {
        return 0.0;
    }
    let downside_var = neg.iter().map(|r| r * r).sum::<f64>() / neg.len() as f64;
    let downside_std = downside_var.sqrt();
    if downside_std <= 0.0 {
        return 0.0;
    }
    let excess = mean_ret - risk_free / annual_days as f64;
    excess / downside_std * (annual_days as f64).sqrt()
}

/// Compute Calmar ratio
fn compute_calmar(annual_return: f64, max_drawdown: f64, start_capital: f64) -> f64 {
    if max_drawdown > 0.0 {
        annual_return * start_capital / max_drawdown
    } else if annual_return > 0.0 {
        f64::INFINITY
    } else {
        0.0
    }
}

/// Partition trades into winning and losing PnL vectors
fn partition_trades(trades: &[TradeRecord]) -> (Vec<f64>, Vec<f64>) {
    let mut winning = Vec::new();
    let mut losing = Vec::new();
    for t in trades {
        if t.realized_pnl > 0.0 {
            winning.push(t.realized_pnl);
        } else if t.realized_pnl < 0.0 {
            losing.push(t.realized_pnl);
        }
    }
    (winning, losing)
}

/// Track max consecutive wins/losses
fn consecutive_wins_losses(trades: &[TradeRecord]) -> (u32, u32) {
    let mut cw = 0u32;
    let mut cl = 0u32;
    let mut mcw = 0u32;
    let mut mcl = 0u32;
    for t in trades {
        if t.realized_pnl > 0.0 {
            cw += 1;
            cl = 0;
            mcw = mcw.max(cw);
        } else if t.realized_pnl < 0.0 {
            cl += 1;
            cw = 0;
            mcl = mcl.max(cl);
        }
    }
    (mcw, mcl)
}

/// Profit factor helper
fn profit_factor(gross_profit: f64, gross_loss: f64) -> f64 {
    if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    }
}

/// Average of a slice
fn avg_of(data: &[f64]) -> f64 {
    if data.is_empty() {
        0.0
    } else {
        data.iter().sum::<f64>() / data.len() as f64
    }
}

/// Average of absolute values
fn avg_abs_of(data: &[f64]) -> f64 {
    if data.is_empty() {
        0.0
    } else {
        data.iter().map(|x| x.abs()).sum::<f64>() / data.len() as f64
    }
}

/// Escape a string for CSV output (quote if contains comma, quote, or newline)
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_engine_new() {
        let engine = ReportEngine::new(100_000.0);
        assert!((engine.start_capital - 100_000.0).abs() < f64::EPSILON);
        assert_eq!(engine.annual_days, 252);
    }

    #[test]
    fn test_report_engine_builder() {
        let engine = ReportEngine::new(50_000.0)
            .risk_free_rate(0.03)
            .annual_days(365);
        assert!((engine.start_capital - 50_000.0).abs() < f64::EPSILON);
        assert!((engine.risk_free_rate - 0.03).abs() < f64::EPSILON);
        assert_eq!(engine.annual_days, 365);
    }

    #[test]
    fn test_record_trade() {
        let engine = ReportEngine::new(100_000.0);
        let trade = TradeRecord {
            trade_id: "T001".to_string(),
            strategy_name: "test_strat".to_string(),
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            direction: Direction::Long,
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            trade_time: Utc::now(),
            commission: 5.0,
            slippage: 1.0,
            realized_pnl: 0.0,
        };
        engine.record_trade(trade);
        let trades = engine
            .trade_log
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade_id, "T001");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_record_daily_balance() {
        let engine = ReportEngine::new(100_000.0);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        engine.record_daily_balance(date, 101_000.0);
        let balances = engine
            .daily_balances
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!((balances[&date] - 101_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_record_equity() {
        let engine = ReportEngine::new(100_000.0);
        engine.record_equity(Utc::now(), 101_000.0, Some(50_000.0));
        let curve = engine
            .equity_curve
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(curve.len(), 1);
        assert!((curve[0].equity - 101_000.0).abs() < f64::EPSILON);
        // benchmark_equity = 100_000 * (50000/50000) = 100_000 on first record
        assert!((curve[0].benchmark_equity - 100_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_record_equity_benchmark_growth() {
        let engine = ReportEngine::new(100_000.0);
        engine.record_equity(Utc::now(), 101_000.0, Some(50_000.0));
        // benchmark at 55000 => 100_000 * (55000/50000) = 110_000
        engine.record_equity(Utc::now(), 102_000.0, Some(55_000.0));
        let curve = engine
            .equity_curve
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(curve.len(), 2);
        assert!((curve[1].benchmark_equity - 110_000.0).abs() < 0.01, "expected 110000, got {}", curve[1].benchmark_equity);
    }

    #[test]
    fn test_generate_report_empty() {
        let engine = ReportEngine::new(100_000.0);
        let pnl_data = StrategyPnlData::default();
        let report = engine.generate_report(&pnl_data);
        assert!(report.start_date.is_empty());
        assert!(report.end_date.is_empty());
        assert_eq!(report.total_days, 0);
        assert!((report.start_capital - 100_000.0).abs() < f64::EPSILON);
        assert!((report.end_capital - 100_000.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_generate_report_with_data() {
        let engine = ReportEngine::new(100_000.0);

        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        engine.record_daily_balance(d1, 101_000.0);
        engine.record_daily_balance(d2, 103_000.0);

        let pnl_data = StrategyPnlData::default();
        let report = engine.generate_report(&pnl_data);

        assert_eq!(report.total_days, 2);
        assert_eq!(report.start_date, "2024-01-15");
        assert_eq!(report.end_date, "2024-01-16");
        assert!((report.end_capital - 103_000.0).abs() < f64::EPSILON);
        assert!(report.total_return > 0.0);
        assert_eq!(report.daily_summaries.len(), 2);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_generate_report_from_backtest() {
        let engine = ReportEngine::new(100_000.0);

        let mut result = BacktestingResult::new(100_000.0);
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut dr1 = crate::backtesting::base::DailyResult::new(d1, 50000.0);
        dr1.net_pnl = 1000.0;
        dr1.trade_count = 2;
        result.daily_results.insert(d1, dr1);

        let d2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();
        let mut dr2 = crate::backtesting::base::DailyResult::new(d2, 51000.0);
        dr2.net_pnl = -500.0;
        dr2.trade_count = 1;
        result.daily_results.insert(d2, dr2);

        let report = engine.generate_report_from_backtest(&result);

        assert_eq!(report.total_days, 2);
        assert!((report.end_capital - 100_500.0).abs() < f64::EPSILON);
        assert_eq!(report.total_trades, 3);
        assert_eq!(report.daily_summaries.len(), 2);
        assert_eq!(report.equity_curve.len(), 2);
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_mean_std_dev() {
        let data = vec![0.01, -0.005, 0.02, 0.015, -0.01];
        let m = mean(&data);
        assert!(m.is_finite());
        let s = std_dev(&data);
        assert!(s.is_finite());
        assert!(s > 0.0);
    }

    #[test]
    fn test_profit_factor_fn() {
        assert!((profit_factor(0.0, 0.0) - 0.0).abs() < f64::EPSILON);
        assert!(profit_factor(100.0, 0.0).is_infinite());
        assert!((profit_factor(100.0, 50.0) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_strategy_pnl_data() {
        let mut data = StrategyPnlData::default();
        data.strategy_pnl.insert("strat_a".to_string(), 500.0);
        data.strategy_unrealized_pnl.insert(("strat_a".to_string(), "BTCUSDT.BINANCE".to_string()), 100.0);
        data.strategy_trade_count.insert("strat_a".to_string(), 10);
        data.strategy_pnl_by_symbol
            .insert(("strat_a".to_string(), "BTCUSDT.BINANCE".to_string()), 500.0);
        data.strategy_avg_price
            .insert(("strat_a".to_string(), "BTCUSDT.BINANCE".to_string()), 45000.0);

        let engine = ReportEngine::new(100_000.0);
        let report = engine.generate_report(&data);

        assert!(report.strategy_reports.contains_key("strat_a"));
        let sr = &report.strategy_reports["strat_a"];
        assert!((sr.realized_pnl - 500.0).abs() < f64::EPSILON);
        assert!((sr.unrealized_pnl - 100.0).abs() < f64::EPSILON);
        assert!((sr.total_pnl - 600.0).abs() < f64::EPSILON);
        assert_eq!(sr.trade_count, 10);
        assert!(sr.symbol_breakdown.contains_key("BTCUSDT.BINANCE"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_export_trades_csv() {
        let engine = ReportEngine::new(100_000.0);
        engine.record_trade(TradeRecord {
            trade_id: "T001".to_string(),
            strategy_name: "test".to_string(),
            vt_symbol: "BTCUSDT.BINANCE".to_string(),
            direction: Direction::Long,
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            trade_time: Utc::now(),
            commission: 5.0,
            slippage: 1.0,
            realized_pnl: 100.0,
        });

        let dir = std::env::temp_dir().join("vnrs_test_trades.csv");
        let result = engine.export_trades_csv(&dir);
        assert!(result.is_ok());
        assert!(dir.exists());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_export_daily_csv() {
        let engine = ReportEngine::new(100_000.0);
        let d1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        engine.record_daily_balance(d1, 101_000.0);

        let pnl_data = StrategyPnlData::default();
        let report = engine.generate_report(&pnl_data);

        let dir = std::env::temp_dir().join("vnrs_test_daily.csv");
        let result = engine.export_daily_csv(&report, &dir);
        assert!(result.is_ok());
        assert!(dir.exists());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn test_consecutive_wins_losses() {
        let trades = vec![
            TradeRecord { trade_id: "1".into(), strategy_name: "s".into(), vt_symbol: "x".into(), direction: Direction::Long, offset: Offset::Open, price: 1.0, volume: 1.0, trade_time: Utc::now(), commission: 0.0, slippage: 0.0, realized_pnl: 10.0 },
            TradeRecord { trade_id: "2".into(), strategy_name: "s".into(), vt_symbol: "x".into(), direction: Direction::Long, offset: Offset::Open, price: 1.0, volume: 1.0, trade_time: Utc::now(), commission: 0.0, slippage: 0.0, realized_pnl: 20.0 },
            TradeRecord { trade_id: "3".into(), strategy_name: "s".into(), vt_symbol: "x".into(), direction: Direction::Long, offset: Offset::Open, price: 1.0, volume: 1.0, trade_time: Utc::now(), commission: 0.0, slippage: 0.0, realized_pnl: -5.0 },
            TradeRecord { trade_id: "4".into(), strategy_name: "s".into(), vt_symbol: "x".into(), direction: Direction::Long, offset: Offset::Open, price: 1.0, volume: 1.0, trade_time: Utc::now(), commission: 0.0, slippage: 0.0, realized_pnl: -3.0 },
            TradeRecord { trade_id: "5".into(), strategy_name: "s".into(), vt_symbol: "x".into(), direction: Direction::Long, offset: Offset::Open, price: 1.0, volume: 1.0, trade_time: Utc::now(), commission: 0.0, slippage: 0.0, realized_pnl: 15.0 },
        ];
        let (mcw, mcl) = consecutive_wins_losses(&trades);
        assert_eq!(mcw, 2);
        assert_eq!(mcl, 2);
    }
}