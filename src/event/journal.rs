//! Event journal for deterministic replay of event sequences.
//!
//! Provides append-only persistence with CRC32 checksums, enabling
//! deterministic replay for debugging and backtesting.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use crc32fast::Hasher;

use super::engine::{Event, EventEngine};

/// A single entry in the event journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// Timestamp in milliseconds since epoch
    pub timestamp: i64,
    /// Sequence number for strict ordering within same-timestamp events
    pub sequence: u64,
    /// Event type string
    pub event_type: String,
    /// Serialized event payload
    pub payload: Vec<u8>,
    /// CRC32 checksum of (timestamp + sequence + event_type + payload)
    pub checksum: u32,
}

/// Wrapper for ordering journal entries by timestamp, then sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryOrd(pub JournalEntry);

impl PartialOrd for EntryOrd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntryOrd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .timestamp
            .cmp(&other.0.timestamp)
            .then_with(|| self.0.sequence.cmp(&other.0.sequence))
    }
}

/// Computes CRC32 checksum over the journal entry components.
///
/// Checksum covers: timestamp (8 bytes LE) + sequence (8 bytes LE) +
/// event_type length (4 bytes LE) + event_type bytes + payload.
fn compute_checksum(timestamp: i64, sequence: u64, event_type: &str, payload: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(&sequence.to_le_bytes());
    let type_len = event_type.len() as u32;
    hasher.update(&type_len.to_le_bytes());
    hasher.update(event_type.as_bytes());
    hasher.update(payload);
    hasher.finalize()
}

/// Binary format for a journal entry on disk:
///
/// ```text
/// [timestamp: 8 bytes LE]
/// [sequence: 8 bytes LE]
/// [event_type_len: 4 bytes LE]
/// [event_type: event_type_len bytes]
/// [payload_len: 4 bytes LE]
/// [payload: payload_len bytes]
/// [checksum: 4 bytes LE]
/// ```
/// Append-only event journal for deterministic replay.
pub struct EventJournal {
    path: PathBuf,
    file: File,
    /// Monotonically increasing sequence counter
    sequence: u64,
}

impl EventJournal {
    /// Create or open an event journal at the given path.
    ///
    /// The file is opened in append mode so existing data is preserved.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(false)
            .open(&path)
            .map_err(|e| format!("Failed to open journal at {:?}: {}", path, e))?;

        Ok(EventJournal {
            path,
            file,
            sequence: 0,
        })
    }

    /// Get the journal file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Append an event to the journal with the current timestamp and auto-incremented sequence.
    ///
    /// The event is serialized as: event_type string bytes (no Arc<dyn Any> payload
    /// since it's not serializable). The payload field is left empty for events
    /// without serializable data.
    pub fn append(&mut self, event: &Event) -> Result<(), String> {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let sequence = self.sequence;
        self.sequence += 1;

        // Event data is Arc<dyn Any + Send + Sync> which is not serializable.
        // We store only the event_type as payload metadata.
        let payload: Vec<u8> = Vec::new();

        let checksum = compute_checksum(timestamp, sequence, &event.event_type, &payload);

        self.write_entry(timestamp, sequence, &event.event_type, &payload, checksum)
    }

    /// Append a pre-built JournalEntry (for replay/restore scenarios).
    pub fn append_entry(&mut self, entry: &JournalEntry) -> Result<(), String> {
        // Verify checksum before writing
        let computed = compute_checksum(entry.timestamp, entry.sequence, &entry.event_type, &entry.payload);
        if computed != entry.checksum {
            return Err(format!(
                "Checksum mismatch for entry: expected {}, got {}",
                entry.checksum, computed
            ));
        }

        self.write_entry(
            entry.timestamp,
            entry.sequence,
            &entry.event_type,
            &entry.payload,
            entry.checksum,
        )
    }

    /// Low-level write of a journal entry to the file.
    fn write_entry(
        &mut self,
        timestamp: i64,
        sequence: u64,
        event_type: &str,
        payload: &[u8],
        checksum: u32,
    ) -> Result<(), String> {
        let mut writer = BufWriter::new(&mut self.file);
        writer
            .write_all(&timestamp.to_le_bytes())
            .map_err(|e| format!("Failed to write timestamp: {}", e))?;
        writer
            .write_all(&sequence.to_le_bytes())
            .map_err(|e| format!("Failed to write sequence: {}", e))?;
        let type_len = event_type.len() as u32;
        writer
            .write_all(&type_len.to_le_bytes())
            .map_err(|e| format!("Failed to write event_type length: {}", e))?;
        writer
            .write_all(event_type.as_bytes())
            .map_err(|e| format!("Failed to write event_type: {}", e))?;
        let payload_len = payload.len() as u32;
        writer
            .write_all(&payload_len.to_le_bytes())
            .map_err(|e| format!("Failed to write payload length: {}", e))?;
        writer
            .write_all(payload)
            .map_err(|e| format!("Failed to write payload: {}", e))?;
        writer
            .write_all(&checksum.to_le_bytes())
            .map_err(|e| format!("Failed to write checksum: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush journal: {}", e))?;

        Ok(())
    }

    /// Read all entries from the journal file with checksum verification.
    ///
    /// Returns entries in the order they appear on disk.
    /// Use `read_all_sorted()` for timestamp-ordered entries.
    pub fn read_all(&self) -> Result<Vec<JournalEntry>, String> {
        let file = File::open(&self.path)
            .map_err(|e| format!("Failed to open journal for reading: {}", e))?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();

        loop {
            match read_one_entry(&mut reader) {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => break, // EOF
                Err(e) => return Err(e),
            }
        }

        Ok(entries)
    }

    /// Read all entries sorted by timestamp, then sequence number.
    pub fn read_all_sorted(&self) -> Result<Vec<JournalEntry>, String> {
        let mut entries = self.read_all()?;
        entries.sort_by_key(|e| EntryOrd(e.clone()));
        Ok(entries)
    }

    /// Replay all journal entries into an EventEngine in timestamp order.
    ///
    /// Returns the number of events replayed.
    pub fn replay(&self, engine: &EventEngine) -> Result<usize, String> {
        let entries = self.read_all_sorted()?;
        let count = entries.len();

        for entry in &entries {
            let event = Event::new(entry.event_type.clone(), None);
            engine.put(event);
        }

        Ok(count)
    }
}

/// Read a single journal entry from the reader, returning None at EOF.
fn read_one_entry<R: Read>(reader: &mut R) -> Result<Option<JournalEntry>, String> {
    let mut buf8 = [0u8; 8];
    let mut buf4 = [0u8; 4];

    // Read timestamp
    match reader.read_exact(&mut buf8) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("Failed to read timestamp: {}", e)),
    }
    let timestamp = i64::from_le_bytes(buf8);

    // Read sequence
    reader
        .read_exact(&mut buf8)
        .map_err(|e| format!("Failed to read sequence: {}", e))?;
    let sequence = u64::from_le_bytes(buf8);

    // Read event_type length
    reader
        .read_exact(&mut buf4)
        .map_err(|e| format!("Failed to read event_type length: {}", e))?;
    let type_len = u32::from_le_bytes(buf4) as usize;

    // Read event_type
    let mut event_type_buf = vec![0u8; type_len];
    reader
        .read_exact(&mut event_type_buf)
        .map_err(|e| format!("Failed to read event_type: {}", e))?;
    let event_type = String::from_utf8(event_type_buf)
        .map_err(|e| format!("Invalid UTF-8 in event_type: {}", e))?;

    // Read payload length
    reader
        .read_exact(&mut buf4)
        .map_err(|e| format!("Failed to read payload length: {}", e))?;
    let payload_len = u32::from_le_bytes(buf4) as usize;

    // Read payload
    let mut payload = vec![0u8; payload_len];
    reader
        .read_exact(&mut payload)
        .map_err(|e| format!("Failed to read payload: {}", e))?;

    // Read checksum
    reader
        .read_exact(&mut buf4)
        .map_err(|e| format!("Failed to read checksum: {}", e))?;
    let checksum = u32::from_le_bytes(buf4);

    // Verify checksum
    let computed = compute_checksum(timestamp, sequence, &event_type, &payload);
    if computed != checksum {
        return Err(format!(
            "CRC32 checksum mismatch at timestamp={}, sequence={}: expected {}, computed {}",
            timestamp, sequence, checksum, computed
        ));
    }

    Ok(Some(JournalEntry {
        timestamp,
        sequence,
        event_type,
        payload,
        checksum,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn temp_journal_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("vnrs_journal_tests");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn test_append_read_roundtrip() {
        let path = temp_journal_path("test_roundtrip.bin");
        let _ = std::fs::remove_file(&path); // Clean slate

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();
            let e1 = Event::new("order_created".to_string(), None);
            let e2 = Event::new("trade_executed".to_string(), None);
            journal.append(&e1).unwrap();
            journal.append(&e2).unwrap();
        }

        // Reopen read-only by creating a new journal (append mode) and reading
        let journal = EventJournal::open(path.clone()).unwrap();
        let entries = journal.read_all().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].event_type, "order_created");
        assert_eq!(entries[1].event_type, "trade_executed");
        assert_eq!(entries[0].sequence, 0);
        assert_eq!(entries[1].sequence, 1);
        // Timestamps should be non-decreasing
        assert!(entries[0].timestamp <= entries[1].timestamp);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_crc32_mismatch_detection() {
        let path = temp_journal_path("test_crc_mismatch.bin");
        let _ = std::fs::remove_file(&path);

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();
            let event = Event::new("test_event".to_string(), None);
            journal.append(&event).unwrap();
        }

        // Corrupt the checksum (last 4 bytes) in the file
        let data = std::fs::read(&path).unwrap();
        assert!(data.len() >= 4);
        let mut corrupted = data.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF; // Flip bits in last byte of checksum
        std::fs::write(&path, &corrupted).unwrap();

        let journal = EventJournal::open(path.clone()).unwrap();
        let result = journal.read_all();
        assert!(result.is_err(), "Should detect CRC32 mismatch");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("checksum mismatch"),
            "Error should mention checksum mismatch, got: {}",
            err_msg
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_timestamp_ordering() {
        let path = temp_journal_path("test_ordering.bin");
        let _ = std::fs::remove_file(&path);

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();

            // Manually write entries with specific timestamps to test ordering
            // Entry with later timestamp first
            let entry1 = JournalEntry {
                timestamp: 3000,
                sequence: 0,
                event_type: "event_c".to_string(),
                payload: Vec::new(),
                checksum: 0, // Will be computed
            };
            let checksum1 = compute_checksum(
                entry1.timestamp,
                entry1.sequence,
                &entry1.event_type,
                &entry1.payload,
            );

            let entry2 = JournalEntry {
                timestamp: 1000,
                sequence: 0,
                event_type: "event_a".to_string(),
                payload: Vec::new(),
                checksum: 0,
            };
            let checksum2 = compute_checksum(
                entry2.timestamp,
                entry2.sequence,
                &entry2.event_type,
                &entry2.payload,
            );

            let entry3 = JournalEntry {
                timestamp: 2000,
                sequence: 0,
                event_type: "event_b".to_string(),
                payload: Vec::new(),
                checksum: 0,
            };
            let checksum3 = compute_checksum(
                entry3.timestamp,
                entry3.sequence,
                &entry3.event_type,
                &entry3.payload,
            );

            // Write in non-sorted order: c, a, b
            journal
                .append_entry(&JournalEntry {
                    checksum: checksum1,
                    ..entry1
                })
                .unwrap();
            journal
                .append_entry(&JournalEntry {
                    checksum: checksum2,
                    ..entry2
                })
                .unwrap();
            journal
                .append_entry(&JournalEntry {
                    checksum: checksum3,
                    ..entry3
                })
                .unwrap();
        }

        let journal = EventJournal::open(path.clone()).unwrap();

        // read_all returns in disk order
        let disk_order = journal.read_all().unwrap();
        assert_eq!(disk_order[0].event_type, "event_c");
        assert_eq!(disk_order[1].event_type, "event_a");
        assert_eq!(disk_order[2].event_type, "event_b");

        // read_all_sorted returns in timestamp order
        let sorted = journal.read_all_sorted().unwrap();
        assert_eq!(sorted[0].event_type, "event_a");
        assert_eq!(sorted[1].event_type, "event_b");
        assert_eq!(sorted[2].event_type, "event_c");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_same_timestamp_ordered_by_sequence() {
        let path = temp_journal_path("test_seq_ordering.bin");
        let _ = std::fs::remove_file(&path);

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();

            // Two events at same timestamp, different sequences
            let entry1 = JournalEntry {
                timestamp: 1000,
                sequence: 5,
                event_type: "later_seq".to_string(),
                payload: Vec::new(),
                checksum: 0,
            };
            let c1 = compute_checksum(entry1.timestamp, entry1.sequence, &entry1.event_type, &entry1.payload);

            let entry2 = JournalEntry {
                timestamp: 1000,
                sequence: 2,
                event_type: "earlier_seq".to_string(),
                payload: Vec::new(),
                checksum: 0,
            };
            let c2 = compute_checksum(entry2.timestamp, entry2.sequence, &entry2.event_type, &entry2.payload);

            // Write higher sequence first
            journal
                .append_entry(&JournalEntry {
                    checksum: c1,
                    ..entry1
                })
                .unwrap();
            journal
                .append_entry(&JournalEntry {
                    checksum: c2,
                    ..entry2
                })
                .unwrap();
        }

        let journal = EventJournal::open(path.clone()).unwrap();
        let sorted = journal.read_all_sorted().unwrap();

        // Should be sorted by sequence within same timestamp
        assert_eq!(sorted[0].event_type, "earlier_seq");
        assert_eq!(sorted[0].sequence, 2);
        assert_eq!(sorted[1].event_type, "later_seq");
        assert_eq!(sorted[1].sequence, 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_replay_into_event_engine() {
        let path = temp_journal_path("test_replay.bin");
        let _ = std::fs::remove_file(&path);

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();
            let e1 = Event::new("order_created".to_string(), None);
            let e2 = Event::new("trade_executed".to_string(), None);
            let e3 = Event::new("order_created".to_string(), None);
            journal.append(&e1).unwrap();
            journal.append(&e2).unwrap();
            journal.append(&e3).unwrap();
        }

        let mut engine = EventEngine::new(1);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&counter);
        engine.register(
            "order_created",
            Arc::new(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let c2 = Arc::clone(&counter);
        engine.register(
            "trade_executed",
            Arc::new(move |_| {
                c2.fetch_add(10, Ordering::SeqCst);
            }),
        );

        engine.start();

        let journal = EventJournal::open(path.clone()).unwrap();
        let count = journal.replay(&engine).unwrap();
        assert_eq!(count, 3);

        // Wait for events to be processed
        std::thread::sleep(Duration::from_millis(200));
        engine.stop();

        // 2 order_created (1 each) + 1 trade_executed (10) = 12
        assert_eq!(counter.load(Ordering::SeqCst), 12);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_append_entry_checksum_validation() {
        let path = temp_journal_path("test_entry_checksum.bin");
        let _ = std::fs::remove_file(&path);

        let mut journal = EventJournal::open(path.clone()).unwrap();

        let bad_entry = JournalEntry {
            timestamp: 1000,
            sequence: 0,
            event_type: "test".to_string(),
            payload: Vec::new(),
            checksum: 99999, // Wrong checksum
        };

        let result = journal.append_entry(&bad_entry);
        assert!(result.is_err(), "Should reject entry with bad checksum");
        assert!(result.unwrap_err().contains("Checksum mismatch"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_empty_journal() {
        let path = temp_journal_path("test_empty.bin");
        let _ = std::fs::remove_file(&path);

        let journal = EventJournal::open(path.clone()).unwrap();
        let entries = journal.read_all().unwrap();
        assert!(entries.is_empty());

        let sorted = journal.read_all_sorted().unwrap();
        assert!(sorted.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_payload_roundtrip() {
        let path = temp_journal_path("test_payload.bin");
        let _ = std::fs::remove_file(&path);

        {
            let mut journal = EventJournal::open(path.clone()).unwrap();
            let payload = b"hello world payload".to_vec();
            let checksum = compute_checksum(5000, 0, "data_event", &payload);
            let entry = JournalEntry {
                timestamp: 5000,
                sequence: 0,
                event_type: "data_event".to_string(),
                payload: payload.clone(),
                checksum,
            };
            journal.append_entry(&entry).unwrap();
        }

        let journal = EventJournal::open(path.clone()).unwrap();
        let entries = journal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].payload, b"hello world payload");
        assert_eq!(entries[0].timestamp, 5000);
        assert_eq!(entries[0].sequence, 0);

        let _ = std::fs::remove_file(&path);
    }
}
