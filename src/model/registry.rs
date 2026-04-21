//! Model Registry — SQLite-backed (or in-memory) metadata store for model lifecycle.
//!
//! The registry tracks model entries (id, version, stage, metrics, features, timestamps)
//! and enforces the `ModelStage` state machine on transitions.

use chrono::Utc;
#[cfg(feature = "sqlite")]
use chrono::DateTime;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

use super::types::{ModelEntry, ModelMetrics, ModelStage};

// ---------------------------------------------------------------------------
// Backend abstraction
// ---------------------------------------------------------------------------

/// Internal storage backend trait. Implemented by both in-memory and SQLite.
trait RegistryBackend: Send + Sync {
    fn register(&self, entry: ModelEntry) -> Result<(), String>;
    fn get(&self, model_id: &str, version: &str) -> Option<ModelEntry>;
    fn list(&self, stage: Option<ModelStage>) -> Vec<ModelEntry>;
    fn update_stage(&self, model_id: &str, version: &str, new_stage: ModelStage) -> Result<(), String>;
    fn update_metrics(&self, model_id: &str, version: &str, metrics: ModelMetrics) -> Result<(), String>;
    fn exists(&self, model_id: &str, version: &str) -> bool;
}

// ---------------------------------------------------------------------------
// In-memory backend
// ---------------------------------------------------------------------------

struct InMemoryBackend {
    entries: Mutex<HashMap<String, ModelEntry>>,
}

impl InMemoryBackend {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl RegistryBackend for InMemoryBackend {
    fn register(&self, entry: ModelEntry) -> Result<(), String> {
        let key = entry.key();
        let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
        if entries.contains_key(&key) {
            return Err(format!("Model already registered: {}", key));
        }
        debug!(key = %key, "Registered model (in-memory)");
        entries.insert(key, entry);
        Ok(())
    }

    fn get(&self, model_id: &str, version: &str) -> Option<ModelEntry> {
        let key = format!("{}:{}", model_id, version);
        let entries = self.entries.lock().ok()?;
        entries.get(&key).cloned()
    }

    fn list(&self, stage: Option<ModelStage>) -> Vec<ModelEntry> {
        let entries = match self.entries.lock() {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        entries
            .values()
            .filter(|e| stage.map_or(true, |s| e.stage == s))
            .cloned()
            .collect()
    }

    fn update_stage(&self, model_id: &str, version: &str, new_stage: ModelStage) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
        let entry = entries
            .get_mut(&key)
            .ok_or_else(|| format!("Model not found: {}", key))?;
        entry.stage.validate_transition(&new_stage)?;
        entry.stage = new_stage;
        entry.updated_at = Utc::now();
        debug!(key = %key, stage = %new_stage, "Transitioned model stage");
        Ok(())
    }

    fn update_metrics(&self, model_id: &str, version: &str, metrics: ModelMetrics) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
        let entry = entries
            .get_mut(&key)
            .ok_or_else(|| format!("Model not found: {}", key))?;
        entry.metrics = metrics;
        entry.updated_at = Utc::now();
        debug!(key = %key, "Updated model metrics");
        Ok(())
    }

    fn exists(&self, model_id: &str, version: &str) -> bool {
        let key = format!("{}:{}", model_id, version);
        self.entries.lock().map(|e| e.contains_key(&key)).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// SQLite backend (behind feature gate)
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlite")]
struct SqliteBackend {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

#[cfg(feature = "sqlite")]
impl SqliteBackend {
    fn new(path: &str) -> Result<Self, String> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| format!("Failed to open model registry DB at {}: {}", path, e))?;
        Self::create_tables(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn new_in_memory() -> Result<Self, String> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory model registry DB: {}", e))?;
        Self::create_tables(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn create_tables(conn: &rusqlite::Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_entries (
                model_id      TEXT NOT NULL,
                version       TEXT NOT NULL,
                stage         TEXT NOT NULL,
                artifact_path TEXT NOT NULL DEFAULT '',
                metrics_json  TEXT NOT NULL DEFAULT '{}',
                feature_ids   TEXT NOT NULL DEFAULT '[]',
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL,
                PRIMARY KEY (model_id, version)
            );"
        ).map_err(|e| format!("Failed to create model_entries table: {}", e))?;
        Ok(())
    }

    fn entry_from_row(row: &rusqlite::Row<'_>) -> Result<ModelEntry, rusqlite::Error> {
        let model_id: String = row.get(0)?;
        let version: String = row.get(1)?;
        let stage_str: String = row.get(2)?;
        let artifact_path: String = row.get(3)?;
        let metrics_json: String = row.get(4)?;
        let feature_ids_json: String = row.get(5)?;
        let created_at_str: String = row.get(6)?;
        let updated_at_str: String = row.get(7)?;

        let stage = serde_json::from_str::<ModelStage>(&format!("\"{}\"", stage_str))
            .unwrap_or(ModelStage::Development);
        let metrics: ModelMetrics = serde_json::from_str(&metrics_json).unwrap_or_default();
        let feature_ids: Vec<String> = serde_json::from_str(&feature_ids_json).unwrap_or_default();
        let created_at: DateTime<Utc> = created_at_str.parse().unwrap_or_else(|_| Utc::now());
        let updated_at: DateTime<Utc> = updated_at_str.parse().unwrap_or_else(|_| Utc::now());

        Ok(ModelEntry {
            model_id,
            version,
            stage,
            artifact_path,
            metrics,
            feature_ids,
            created_at,
            updated_at,
        })
    }
}

#[cfg(feature = "sqlite")]
impl RegistryBackend for SqliteBackend {
    fn register(&self, entry: ModelEntry) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let metrics_json = serde_json::to_string(&entry.metrics).unwrap_or_else(|_| "{}".into());
        let feature_ids_json = serde_json::to_string(&entry.feature_ids).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT OR REPLACE INTO model_entries (model_id, version, stage, artifact_path, metrics_json, feature_ids, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                entry.model_id,
                entry.version,
                entry.stage.to_string(),
                entry.artifact_path,
                metrics_json,
                feature_ids_json,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("Failed to register model: {}", e))?;
        info!(model_id = %entry.model_id, version = %entry.version, "Registered model (sqlite)");
        Ok(())
    }

    fn get(&self, model_id: &str, version: &str) -> Option<ModelEntry> {
        let conn = self.conn.lock().ok()?;
        let mut stmt = conn.prepare(
            "SELECT model_id, version, stage, artifact_path, metrics_json, feature_ids, created_at, updated_at
             FROM model_entries WHERE model_id = ?1 AND version = ?2"
        ).ok()?;
        stmt.query_row(rusqlite::params![model_id, version], Self::entry_from_row).ok()
    }

    fn list(&self, stage: Option<ModelStage>) -> Vec<ModelEntry> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = match stage {
            Some(_) => "SELECT model_id, version, stage, artifact_path, metrics_json, feature_ids, created_at, updated_at FROM model_entries WHERE stage = ?1",
            None => "SELECT model_id, version, stage, artifact_path, metrics_json, feature_ids, created_at, updated_at FROM model_entries",
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stage {
            Some(ref s) => stmt.query(rusqlite::params![s.to_string()]),
            None => stmt.query([]),
        };
        let mut rows = match rows {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let mut result = Vec::new();
        while let Ok(Some(row)) = rows.next() {
            if let Ok(entry) = Self::entry_from_row(row) {
                result.push(entry);
            }
        }
        result
    }

    fn update_stage(&self, model_id: &str, version: &str, new_stage: ModelStage) -> Result<(), String> {
        // Fetch current entry to validate transition
        let current = self.get(model_id, version)
            .ok_or_else(|| format!("Model not found: {}:{}", model_id, version))?;
        current.stage.validate_transition(&new_stage)?;

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE model_entries SET stage = ?1, updated_at = ?2 WHERE model_id = ?3 AND version = ?4",
            rusqlite::params![new_stage.to_string(), Utc::now().to_rfc3339(), model_id, version],
        ).map_err(|e| format!("Failed to update model stage: {}", e))?;
        info!(model_id, version, stage = %new_stage, "Transitioned model stage (sqlite)");
        Ok(())
    }

    fn update_metrics(&self, model_id: &str, version: &str, metrics: ModelMetrics) -> Result<(), String> {
        let metrics_json = serde_json::to_string(&metrics).unwrap_or_else(|_| "{}".into());
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE model_entries SET metrics_json = ?1, updated_at = ?2 WHERE model_id = ?3 AND version = ?4",
            rusqlite::params![metrics_json, Utc::now().to_rfc3339(), model_id, version],
        ).map_err(|e| format!("Failed to update model metrics: {}", e))?;
        Ok(())
    }

    fn exists(&self, model_id: &str, version: &str) -> bool {
        self.get(model_id, version).is_some()
    }
}

// ---------------------------------------------------------------------------
// ModelRegistry — public facade
// ---------------------------------------------------------------------------

/// Registry for model metadata with stage lifecycle management.
///
/// When the `sqlite` feature is enabled, the registry can be backed by a SQLite
/// database. Otherwise it uses an in-memory `HashMap`.
pub struct ModelRegistry {
    backend: Arc<dyn RegistryBackend>,
}

impl ModelRegistry {
    /// Create a new in-memory registry.
    pub fn new_in_memory() -> Self {
        info!("Created in-memory ModelRegistry");
        Self {
            backend: Arc::new(InMemoryBackend::new()),
        }
    }

    /// Create a SQLite-backed registry at the given path.
    #[cfg(feature = "sqlite")]
    pub fn new_sqlite(path: &str) -> Result<Self, String> {
        info!(path, "Creating SQLite-backed ModelRegistry");
        Ok(Self {
            backend: Arc::new(SqliteBackend::new(path)?),
        })
    }

    /// Create a SQLite-backed in-memory registry (useful for testing).
    #[cfg(feature = "sqlite")]
    pub fn new_sqlite_in_memory() -> Result<Self, String> {
        Ok(Self {
            backend: Arc::new(SqliteBackend::new_in_memory()?),
        })
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Register a new model entry.
    ///
    /// Returns an error if a model with the same `model_id:version` already exists.
    pub fn register(&self, entry: ModelEntry) -> Result<(), String> {
        self.backend.register(entry)
    }

    /// Get a model entry by its ID and version.
    pub fn get(&self, model_id: &str, version: &str) -> Option<ModelEntry> {
        self.backend.get(model_id, version)
    }

    /// List model entries, optionally filtered by stage.
    pub fn list(&self, stage: Option<ModelStage>) -> Vec<ModelEntry> {
        self.backend.list(stage)
    }

    /// Transition a model to a new stage.
    ///
    /// Validates the transition against the `ModelStage` state machine.
    /// Returns an error for invalid transitions.
    pub fn transition(&self, model_id: &str, version: &str, new_stage: ModelStage) -> Result<(), String> {
        self.backend.update_stage(model_id, version, new_stage)
    }

    /// Update the metrics for a model.
    pub fn update_metrics(&self, model_id: &str, version: &str, metrics: ModelMetrics) -> Result<(), String> {
        self.backend.update_metrics(model_id, version, metrics)
    }

    /// Archive a model (shorthand for `transition(model_id, version, ModelStage::Archived)`).
    pub fn archive(&self, model_id: &str, version: &str) -> Result<(), String> {
        self.transition(model_id, version, ModelStage::Archived)
    }

    /// Check whether a model exists.
    pub fn exists(&self, model_id: &str, version: &str) -> bool {
        self.backend.exists(model_id, version)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, ver: &str) -> ModelEntry {
        ModelEntry::new(id, ver, format!("/models/{}_v{}.onnx", id, ver))
    }

    // ---- In-memory tests ----

    #[test]
    fn test_register_and_get() {
        let reg = ModelRegistry::new_in_memory();
        let entry = make_entry("btc_pred", "1.0");
        reg.register(entry.clone()).unwrap();
        let got = reg.get("btc_pred", "1.0").unwrap();
        assert_eq!(got.model_id, "btc_pred");
        assert_eq!(got.version, "1.0");
    }

    #[test]
    fn test_register_duplicate_fails() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1.0")).unwrap();
        let err = reg.register(make_entry("m", "1.0")).unwrap_err();
        assert!(err.contains("already registered"));
    }

    #[test]
    fn test_get_missing() {
        let reg = ModelRegistry::new_in_memory();
        assert!(reg.get("nope", "0.0").is_none());
    }

    #[test]
    fn test_list_all() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("a", "1")).unwrap();
        reg.register(make_entry("b", "2")).unwrap();
        assert_eq!(reg.list(None).len(), 2);
    }

    #[test]
    fn test_list_by_stage() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("a", "1")).unwrap(); // Development
        let mut e2 = make_entry("b", "2");
        e2.stage = ModelStage::Production;
        reg.register(e2).unwrap();
        assert_eq!(reg.list(Some(ModelStage::Development)).len(), 1);
        assert_eq!(reg.list(Some(ModelStage::Production)).len(), 1);
        assert_eq!(reg.list(Some(ModelStage::Staging)).len(), 0);
    }

    #[test]
    fn test_transition_valid() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        reg.transition("m", "1", ModelStage::Staging).unwrap();
        assert_eq!(reg.get("m", "1").unwrap().stage, ModelStage::Staging);
    }

    #[test]
    fn test_transition_invalid() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        let err = reg.transition("m", "1", ModelStage::Production).unwrap_err();
        assert!(err.contains("Invalid stage transition"));
    }

    #[test]
    fn test_transition_missing_model() {
        let reg = ModelRegistry::new_in_memory();
        let err = reg.transition("x", "1", ModelStage::Staging).unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_update_metrics() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        let metrics = ModelMetrics {
            accuracy: 0.95,
            sharpe_ratio: 2.1,
            max_drawdown: -0.12,
            latency_ms: 5.3,
        };
        reg.update_metrics("m", "1", metrics).unwrap();
        let got = reg.get("m", "1").unwrap();
        assert!((got.metrics.accuracy - 0.95).abs() < 1e-9);
        assert!((got.metrics.sharpe_ratio - 2.1).abs() < 1e-9);
    }

    #[test]
    fn test_archive() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        reg.archive("m", "1").unwrap();
        assert_eq!(reg.get("m", "1").unwrap().stage, ModelStage::Archived);
    }

    #[test]
    fn test_archive_from_production() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        reg.transition("m", "1", ModelStage::Staging).unwrap();
        reg.transition("m", "1", ModelStage::Shadow).unwrap();
        reg.transition("m", "1", ModelStage::Canary).unwrap();
        reg.transition("m", "1", ModelStage::Production).unwrap();
        reg.archive("m", "1").unwrap();
        assert_eq!(reg.get("m", "1").unwrap().stage, ModelStage::Archived);
    }

    #[test]
    fn test_archived_cannot_transition() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("m", "1")).unwrap();
        reg.archive("m", "1").unwrap();
        let err = reg.transition("m", "1", ModelStage::Development).unwrap_err();
        assert!(err.contains("Invalid stage transition"));
    }

    #[test]
    fn test_full_lifecycle() {
        let reg = ModelRegistry::new_in_memory();
        reg.register(make_entry("lifecycle", "1.0")).unwrap();
        reg.transition("lifecycle", "1.0", ModelStage::Staging).unwrap();
        reg.transition("lifecycle", "1.0", ModelStage::Shadow).unwrap();
        reg.transition("lifecycle", "1.0", ModelStage::Canary).unwrap();
        reg.transition("lifecycle", "1.0", ModelStage::Production).unwrap();
        assert_eq!(reg.get("lifecycle", "1.0").unwrap().stage, ModelStage::Production);
    }

    #[test]
    fn test_exists() {
        let reg = ModelRegistry::new_in_memory();
        assert!(!reg.exists("m", "1"));
        reg.register(make_entry("m", "1")).unwrap();
        assert!(reg.exists("m", "1"));
    }

    // ---- SQLite-backed tests (only when sqlite feature is enabled) ----

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_sqlite_register_and_get() {
        let reg = ModelRegistry::new_sqlite_in_memory().unwrap();
        reg.register(make_entry("sql_m", "1")).unwrap();
        let got = reg.get("sql_m", "1").unwrap();
        assert_eq!(got.model_id, "sql_m");
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_sqlite_transition() {
        let reg = ModelRegistry::new_sqlite_in_memory().unwrap();
        reg.register(make_entry("sql_m", "1")).unwrap();
        reg.transition("sql_m", "1", ModelStage::Staging).unwrap();
        assert_eq!(reg.get("sql_m", "1").unwrap().stage, ModelStage::Staging);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_sqlite_list_by_stage() {
        let reg = ModelRegistry::new_sqlite_in_memory().unwrap();
        reg.register(make_entry("a", "1")).unwrap();
        let mut e = make_entry("b", "2");
        e.stage = ModelStage::Production;
        reg.register(e).unwrap();
        assert_eq!(reg.list(Some(ModelStage::Development)).len(), 1);
        assert_eq!(reg.list(Some(ModelStage::Production)).len(), 1);
    }
}
