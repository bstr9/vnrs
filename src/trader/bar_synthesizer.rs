//! Multi-Period Bar Synthesizer
//!
//! Resamples lower-timeframe bars into higher-timeframe bars with gap detection
//! and batch resampling support. Used by the strategy engine for multi-period
//! bar generation and by the backtesting engine for batch resampling.

use chrono::{DateTime, Duration, Utc};
use tracing::{debug, warn};

use super::constant::Interval;
use super::object::BarData;

/// Bar synthesizer for multi-period bar generation from lower-timeframe bars.
///
/// Accumulates source-interval bars and emits a complete target-interval bar
/// when the window is filled. Supports gap detection (resets on missing bars)
/// and time-boundary validation (resets on misaligned bars).
#[derive(Debug, Clone)]
pub struct BarSynthesizer {
    /// Target interval to synthesize (e.g., Minute5, Minute15, Hour)
    interval: Interval,
    /// Source interval (typically Minute)
    source_interval: Interval,
    /// Number of source bars that make up one target bar
    window: u32,
    /// Current count of accumulated source bars
    count: u32,
    /// Currently accumulated bar data
    accumulated: Option<BarData>,
    /// Last source bar's datetime (for gap detection)
    last_bar_time: Option<DateTime<Utc>>,
}

impl BarSynthesizer {
    /// Create a new `BarSynthesizer` that resamples from `source_interval` to `target_interval`.
    ///
    /// The window is calculated as `target_seconds / source_seconds`.
    /// If `target <= source`, window is set to 1 (passthrough mode).
    pub fn new(source_interval: Interval, target_interval: Interval) -> Self {
        let source_secs = Self::interval_to_seconds(source_interval);
        let target_secs = Self::interval_to_seconds(target_interval);

        let window = if target_secs <= source_secs {
            1
        } else {
            let w = target_secs / source_secs;
            if w == 0 { 1 } else { w as u32 }
        };

        Self {
            interval: target_interval,
            source_interval,
            window,
            count: 0,
            accumulated: None,
            last_bar_time: None,
        }
    }

    /// Feed a source bar into the synthesizer.
    ///
    /// Returns `Some(BarData)` when a complete target-interval bar has been formed.
    /// Performs gap detection and time-boundary validation:
    /// - **Gap detection**: If the incoming bar's datetime is not contiguous with
    ///   the previous bar, the accumulator is reset and synthesis restarts from
    ///   the current bar.
    /// - **Time-boundary validation**: If the incoming bar's start time does not
    ///   align with the current window's expected boundary, the accumulator is
    ///   reset.
    pub fn update_bar(&mut self, bar: &BarData) -> Option<BarData> {
        // Passthrough: if window is 1, no synthesis needed
        if self.window <= 1 {
            return None;
        }

        // Gap detection and time-boundary validation
        if let Some(last_time) = self.last_bar_time {
            let source_secs = Self::interval_to_seconds(self.source_interval);
            let expected_next = last_time + Duration::seconds(source_secs);

            // Gap detected: incoming bar is later than expected next bar time
            if bar.datetime > expected_next {
                if self.count > 0 {
                    warn!(
                        "Bar gap detected: expected bar at {:?}, got bar at {:?}. \
                         Resetting synthesizer (discarded {} bars for {:?})",
                        expected_next, bar.datetime, self.count, self.interval
                    );
                }
                self.reset();
            } else if bar.datetime < expected_next {
                // Duplicate or out-of-order bar — skip it
                debug!(
                    "Out-of-order bar received: expected {:?}, got {:?}. Skipping.",
                    expected_next, bar.datetime
                );
                return None;
            }

            // Time-boundary validation: check alignment with target interval
            if self.accumulated.is_some() && !self.is_within_boundary(bar.datetime) {
                warn!(
                    "Bar at {:?} does not align with current {:?} window boundary. Resetting.",
                    bar.datetime, self.interval
                );
                self.reset();
            }
        }

        // Accumulate bar data
        if let Some(ref mut acc) = self.accumulated {
            acc.high_price = acc.high_price.max(bar.high_price);
            acc.low_price = acc.low_price.min(bar.low_price);
            acc.close_price = bar.close_price;
            acc.volume += bar.volume;
            acc.turnover += bar.turnover;
            acc.open_interest = bar.open_interest;
            acc.datetime = bar.datetime;
        } else {
            self.accumulated = Some(BarData {
                gateway_name: bar.gateway_name.clone(),
                symbol: bar.symbol.clone(),
                exchange: bar.exchange,
                datetime: bar.datetime,
                interval: Some(self.interval),
                volume: bar.volume,
                turnover: bar.turnover,
                open_interest: bar.open_interest,
                open_price: bar.open_price,
                high_price: bar.high_price,
                low_price: bar.low_price,
                close_price: bar.close_price,
                extra: None,
            });
        }

        self.count += 1;
        self.last_bar_time = Some(bar.datetime);

        if self.count >= self.window {
            let mut completed = self.accumulated.take();
            if let Some(ref mut completed_bar) = completed {
                completed_bar.interval = Some(self.interval);
            }
            self.count = 0;
            self.last_bar_time = None;
            completed
        } else {
            None
        }
    }

    /// Stateless batch resampling for backtesting use.
    ///
    /// Creates a fresh `BarSynthesizer` internally, feeds all bars, and collects
    /// only complete higher-timeframe bars. Handles gaps: incomplete bars at gaps
    /// are discarded and synthesis restarts.
    pub fn resample(&self, bars: &[BarData]) -> Vec<BarData> {
        let mut synth = Self::new(self.source_interval, self.interval);
        let mut result = Vec::new();

        for bar in bars {
            if let Some(synthesized) = synth.update_bar(bar) {
                result.push(synthesized);
            }
        }

        result
    }

    /// Clear the accumulator, count, and last bar time.
    pub fn reset(&mut self) {
        self.count = 0;
        self.accumulated = None;
        self.last_bar_time = None;
    }

    /// Returns `true` if currently accumulating a bar (i.e., count > 0).
    pub fn is_active(&self) -> bool {
        self.count > 0
    }

    /// Check whether a bar's datetime falls within the current window's
    /// time boundary for the target interval.
    ///
    /// For example, a 5m bar starting at 10:00 should complete at 10:04.
    /// If a bar arrives at 10:07, it doesn't align with the current window.
    fn is_within_boundary(&self, bar_time: DateTime<Utc>) -> bool {
        let target_secs = Self::interval_to_seconds(self.interval);
        let source_secs = Self::interval_to_seconds(self.source_interval);

        if let Some(ref acc) = self.accumulated {
            // Window start is the accumulated bar's datetime
            let window_start = acc.datetime;
            let window_end = window_start + Duration::seconds(target_secs - source_secs);

            // Bar should fall within [window_start, window_end]
            bar_time >= window_start && bar_time <= window_end
        } else {
            true
        }
    }

    /// Convert an `Interval` to its duration in seconds.
    fn interval_to_seconds(interval: Interval) -> i64 {
        match interval {
            Interval::Second => 1,
            Interval::Minute => 60,
            Interval::Minute5 => 300,
            Interval::Minute15 => 900,
            Interval::Minute30 => 1800,
            Interval::Hour => 3600,
            Interval::Hour4 => 14400,
            Interval::Daily => 86400,
            Interval::Weekly => 604800,
            Interval::Tick => 0,
        }
    }

    /// Get the target interval
    pub fn interval(&self) -> Interval {
        self.interval
    }

    /// Get the source interval
    pub fn source_interval(&self) -> Interval {
        self.source_interval
    }

    /// Get the window size
    pub fn window(&self) -> u32 {
        self.window
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::trader::constant::Exchange;

    /// Helper to create a 1-minute BarData at a given minute offset from epoch
    fn make_bar(minute_offset: i64, close: f64, volume: f64) -> BarData {
        let dt = DateTime::UNIX_EPOCH + Duration::minutes(minute_offset);
        BarData {
            gateway_name: "test".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            volume,
            turnover: volume * close,
            open_interest: 0.0,
            open_price: close - 1.0,
            high_price: close + 2.0,
            low_price: close - 2.0,
            close_price: close,
            extra: None,
        }
    }

    #[test]
    fn test_synthesizer_minute_to_5m() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);
        assert_eq!(synth.window(), 5);

        let mut result = None;
        for i in 0..5 {
            result = synth.update_bar(&make_bar(i, 100.0 + i as f64, 10.0));
        }

        // After 5 bars, we should get a completed 5m bar
        let bar = result.expect("Should emit a 5m bar after 5 1m bars");
        assert_eq!(bar.interval, Some(Interval::Minute5));
        assert_eq!(bar.open_price, 99.0);  // First bar's open
        assert_eq!(bar.close_price, 104.0); // Last bar's close
        assert_eq!(bar.high_price, 106.0);  // Max high across bars
        assert_eq!(bar.low_price, 97.0);    // Min low across bars
        assert!((bar.volume - 50.0).abs() < f64::EPSILON); // 5 * 10.0

        // Synthesizer should be reset
        assert!(!synth.is_active());
    }

    #[test]
    fn test_synthesizer_gap_detection() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);

        // Feed 3 bars at minutes 0, 1, 2
        let _ = synth.update_bar(&make_bar(0, 100.0, 10.0));
        let _ = synth.update_bar(&make_bar(1, 101.0, 10.0));
        let _ = synth.update_bar(&make_bar(2, 102.0, 10.0));

        assert!(synth.is_active());

        // Skip minute 3 — feed bar at minute 4 (gap of 1 bar)
        // Expected next is minute 3, but we get minute 4 => gap detected => reset
        let _ = synth.update_bar(&make_bar(4, 104.0, 10.0));

        // After gap, synthesizer restarts with the bar at minute 4
        // So count should be 1 (just the bar at minute 4)
        assert!(synth.is_active());

        // Continue feeding bars 5, 6, 7, 8 — should complete a 5m bar
        // But wait: after reset at minute 4, the window starts at minute 4
        // Window = minutes 4,5,6,7,8 => 5 bars
        let _ = synth.update_bar(&make_bar(5, 105.0, 10.0));
        let _ = synth.update_bar(&make_bar(6, 106.0, 10.0));
        let _ = synth.update_bar(&make_bar(7, 107.0, 10.0));
        let result = synth.update_bar(&make_bar(8, 108.0, 10.0));

        let bar = result.expect("Should emit a 5m bar after gap recovery");
        assert_eq!(bar.interval, Some(Interval::Minute5));
        assert_eq!(bar.open_price, 103.0); // Bar at minute 4: close=104, open=103
    }

    #[test]
    fn test_synthesizer_batch_resample() {
        let synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);

        // Create 15 consecutive 1m bars
        let bars: Vec<BarData> = (0..15).map(|i| make_bar(i, 100.0 + i as f64, 10.0)).collect();

        let result = synth.resample(&bars);
        assert_eq!(result.len(), 3, "Should produce 3 5m bars from 15 1m bars");

        for bar in &result {
            assert_eq!(bar.interval, Some(Interval::Minute5));
        }
    }

    #[test]
    fn test_synthesizer_passthrough() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute);
        assert_eq!(synth.window(), 1);

        // When source == target, update_bar should return None (passthrough)
        let result = synth.update_bar(&make_bar(0, 100.0, 10.0));
        assert!(result.is_none());

        let mut synth2 = BarSynthesizer::new(Interval::Minute5, Interval::Minute);
        assert_eq!(synth2.window(), 1);
        let result2 = synth2.update_bar(&make_bar(0, 100.0, 10.0));
        assert!(result2.is_none());
    }

    #[test]
    fn test_interval_to_seconds() {
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Second), 1);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Minute), 60);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Minute5), 300);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Minute15), 900);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Minute30), 1800);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Hour), 3600);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Hour4), 14400);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Daily), 86400);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Weekly), 604800);
        assert_eq!(BarSynthesizer::interval_to_seconds(Interval::Tick), 0);
    }

    #[test]
    fn test_synthesizer_time_boundary() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);

        // Start at minute 0 (aligned to 5m boundary)
        let _ = synth.update_bar(&make_bar(0, 100.0, 10.0));
        let _ = synth.update_bar(&make_bar(1, 101.0, 10.0));

        // Feed bar at minute 7 — this doesn't align with the 0-4 window
        // Time boundary: window_start=0, window_end=4 (target_secs=300 - source_secs=60 = 240s = 4min)
        // bar_time = 7min > window_end = 4min => boundary violation => reset
        let _ = synth.update_bar(&make_bar(7, 107.0, 10.0));

        // After boundary reset, synthesizer restarts at minute 7
        // Feed 4 more bars: 8, 9, 10, 11 to complete a 5-bar window
        let _ = synth.update_bar(&make_bar(8, 108.0, 10.0));
        let _ = synth.update_bar(&make_bar(9, 109.0, 10.0));
        let _ = synth.update_bar(&make_bar(10, 110.0, 10.0));
        let result = synth.update_bar(&make_bar(11, 111.0, 10.0));

        let bar = result.expect("Should emit a 5m bar after boundary reset recovery");
        assert_eq!(bar.interval, Some(Interval::Minute5));
    }

    #[test]
    fn test_synthesizer_window_calculation() {
        // 1m -> 5m => window = 5
        assert_eq!(BarSynthesizer::new(Interval::Minute, Interval::Minute5).window(), 5);
        // 1m -> 15m => window = 15
        assert_eq!(BarSynthesizer::new(Interval::Minute, Interval::Minute15).window(), 15);
        // 1m -> 1h => window = 60
        assert_eq!(BarSynthesizer::new(Interval::Minute, Interval::Hour).window(), 60);
        // 1m -> 4h => window = 240
        assert_eq!(BarSynthesizer::new(Interval::Minute, Interval::Hour4).window(), 240);
        // 5m -> 15m => window = 3
        assert_eq!(BarSynthesizer::new(Interval::Minute5, Interval::Minute15).window(), 3);
        // 5m -> 1h => window = 12
        assert_eq!(BarSynthesizer::new(Interval::Minute5, Interval::Hour).window(), 12);
        // 1h -> 4h => window = 4
        assert_eq!(BarSynthesizer::new(Interval::Hour, Interval::Hour4).window(), 4);
        // Same interval => passthrough
        assert_eq!(BarSynthesizer::new(Interval::Minute5, Interval::Minute5).window(), 1);
    }

    #[test]
    fn test_synthesizer_is_active() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);
        assert!(!synth.is_active());

        let _ = synth.update_bar(&make_bar(0, 100.0, 10.0));
        assert!(synth.is_active());

        synth.reset();
        assert!(!synth.is_active());
    }

    #[test]
    fn test_synthesizer_out_of_order_bar_skipped() {
        let mut synth = BarSynthesizer::new(Interval::Minute, Interval::Minute5);

        let _ = synth.update_bar(&make_bar(5, 105.0, 10.0));
        let _ = synth.update_bar(&make_bar(6, 106.0, 10.0));

        // Feed a bar at minute 5 again (out of order / duplicate)
        let result = synth.update_bar(&make_bar(5, 105.0, 10.0));
        assert!(result.is_none(), "Out-of-order bar should be skipped");

        // Count should still be 2
        assert!(synth.is_active());
    }
}
