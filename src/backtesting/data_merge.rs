//! Data Merge Utilities
//!
//! Efficient K-way merge of sorted market data streams using a min-heap.
//! Provides O(N log K) merging when input streams are already sorted,
//! where K is the number of symbols and N is total data points.
//! This avoids cloning all data and the O(N log N) full sort.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use chrono::{DateTime, Utc};

use crate::trader::{BarData, TickData};

/// Converts a `DateTime<Utc>` to a comparable i64 timestamp (nanoseconds).
/// Falls back to 0 if the timestamp is out of range.
#[inline]
fn datetime_to_nanos(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_nanos_opt().unwrap_or(0)
}

/// Heap-based K-way merge iterator for bar data.
///
/// Takes multiple sorted slices of `BarData` and yields references
/// in ascending datetime order without cloning any data.
pub struct BarMergeIterator<'a> {
    /// Min-heap of (timestamp_nanos, source_index, position_in_source)
    heap: BinaryHeap<Reverse<(i64, usize, usize)>>,
    /// Source data slices
    sources: Vec<&'a [BarData]>,
}

impl<'a> BarMergeIterator<'a> {
    /// Create a new merge iterator from multiple sorted bar streams.
    ///
    /// Each slice must be sorted by `datetime` in ascending order.
    pub fn new(sources: Vec<&'a [BarData]>) -> Self {
        let mut heap = BinaryHeap::with_capacity(sources.len());

        for (idx, slice) in sources.iter().enumerate() {
            if let Some(first) = slice.first() {
                let ts = datetime_to_nanos(first.datetime);
                heap.push(Reverse((ts, idx, 0)));
            }
        }

        Self { heap, sources }
    }
}

impl<'a> Iterator for BarMergeIterator<'a> {
    type Item = &'a BarData;

    fn next(&mut self) -> Option<Self::Item> {
        let Reverse((_, source_idx, pos)) = self.heap.pop()?;
        let slice = self.sources.get(source_idx)?;
        let bar = slice.get(pos)?;

        // Push next item from the same source
        if let Some(next_bar) = slice.get(pos + 1) {
            let next_ts = datetime_to_nanos(next_bar.datetime);
            self.heap.push(Reverse((next_ts, source_idx, pos + 1)));
        }

        Some(bar)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining: usize = self
            .sources
            .iter()
            .map(|s| s.len())
            .sum::<usize>()
            .saturating_sub(
                self.heap
                    .len()
                    .saturating_sub(self.sources.iter().filter(|s| s.is_empty()).count()),
            );
        // Lower bound: items still in heap (each represents at least one unyielded item)
        // Upper bound: all remaining items across all sources
        (self.heap.len(), Some(remaining))
    }
}

/// Heap-based K-way merge iterator for tick data.
///
/// Takes multiple sorted slices of `TickData` and yields references
/// in ascending datetime order without cloning any data.
pub struct TickMergeIterator<'a> {
    /// Min-heap of (timestamp_nanos, source_index, position_in_source)
    heap: BinaryHeap<Reverse<(i64, usize, usize)>>,
    /// Source data slices
    sources: Vec<&'a [TickData]>,
}

impl<'a> TickMergeIterator<'a> {
    /// Create a new merge iterator from multiple sorted tick streams.
    ///
    /// Each slice must be sorted by `datetime` in ascending order.
    pub fn new(sources: Vec<&'a [TickData]>) -> Self {
        let mut heap = BinaryHeap::with_capacity(sources.len());

        for (idx, slice) in sources.iter().enumerate() {
            if let Some(first) = slice.first() {
                let ts = datetime_to_nanos(first.datetime);
                heap.push(Reverse((ts, idx, 0)));
            }
        }

        Self { heap, sources }
    }
}

impl<'a> Iterator for TickMergeIterator<'a> {
    type Item = &'a TickData;

    fn next(&mut self) -> Option<Self::Item> {
        let Reverse((_, source_idx, pos)) = self.heap.pop()?;
        let slice = self.sources.get(source_idx)?;
        let tick = slice.get(pos)?;

        // Push next item from the same source
        if let Some(next_tick) = slice.get(pos + 1) {
            let next_ts = datetime_to_nanos(next_tick.datetime);
            self.heap.push(Reverse((next_ts, source_idx, pos + 1)));
        }

        Some(tick)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining: usize = self
            .sources
            .iter()
            .map(|s| s.len())
            .sum::<usize>()
            .saturating_sub(
                self.heap
                    .len()
                    .saturating_sub(self.sources.iter().filter(|s| s.is_empty()).count()),
            );
        (self.heap.len(), Some(remaining))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Exchange, Interval};
    use chrono::TimeZone;

    fn make_bar(symbol: &str, dt: DateTime<Utc>) -> BarData {
        BarData {
            gateway_name: "test".to_string(),
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            volume: 0.0,
            turnover: 0.0,
            open_interest: 0.0,
            open_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            close_price: 0.0,
            extra: None,
        }
    }

    #[test]
    fn test_bar_merge_single_source() {
        let bars = vec![
            make_bar("A", Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap()),
            make_bar("A", Utc.with_ymd_and_hms(2024, 1, 1, 10, 1, 0).unwrap()),
            make_bar("A", Utc.with_ymd_and_hms(2024, 1, 1, 10, 2, 0).unwrap()),
        ];
        let merged: Vec<_> = BarMergeIterator::new(vec![&bars]).collect();
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].datetime, bars[0].datetime);
        assert_eq!(merged[2].datetime, bars[2].datetime);
    }

    #[test]
    fn test_bar_merge_interleaved() {
        let bars_a = vec![
            make_bar("A", Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap()),
            make_bar("A", Utc.with_ymd_and_hms(2024, 1, 1, 10, 2, 0).unwrap()),
        ];
        let bars_b = vec![
            make_bar("B", Utc.with_ymd_and_hms(2024, 1, 1, 10, 1, 0).unwrap()),
            make_bar("B", Utc.with_ymd_and_hms(2024, 1, 1, 10, 3, 0).unwrap()),
        ];
        let merged: Vec<_> = BarMergeIterator::new(vec![&bars_a, &bars_b]).collect();
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].symbol, "A");
        assert_eq!(merged[1].symbol, "B");
        assert_eq!(merged[2].symbol, "A");
        assert_eq!(merged[3].symbol, "B");
    }

    #[test]
    fn test_bar_merge_empty_source() {
        let bars_a = vec![make_bar(
            "A",
            Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
        )];
        let bars_b: Vec<BarData> = vec![];
        let merged: Vec<_> = BarMergeIterator::new(vec![&bars_a, &bars_b]).collect();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_bar_merge_all_empty() {
        let merged: Vec<_> = BarMergeIterator::new(vec![&[], &[]]).collect();
        assert!(merged.is_empty());
    }
}
