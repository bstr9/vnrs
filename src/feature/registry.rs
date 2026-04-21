//! Feature registry for definition, version, and lineage tracking.
//!
//! The `FeatureRegistry` maintains feature definitions with version history
//! and dependency lineage. This enables:
//! - Feature versioning for reproducibility
//! - Lineage tracking (which features depend on which)
//! - Training/serving consistency guarantees

use std::collections::HashMap;

use super::types::{FeatureDefinition, FeatureId};

/// Registry for feature definitions with version and lineage tracking.
///
/// Maintains a mapping from `FeatureId` to one or more `FeatureDefinition`
/// entries (one per version). Supports:
/// - Registering new feature definitions
/// - Looking up definitions by id
/// - Listing all registered features
/// - Tracing dependency lineage
/// - Listing versions of a feature
pub struct FeatureRegistry {
    /// Feature definitions indexed by id -> list of versions
    definitions: HashMap<FeatureId, Vec<FeatureDefinition>>,
}

impl FeatureRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    /// Register a new feature definition.
    ///
    /// If a definition with the same id and version already exists,
    /// it will be replaced.
    pub fn register(&mut self, definition: FeatureDefinition) {
        let id = definition.id.clone();
        let version = definition.version;

        let versions = self.definitions.entry(id).or_default();

        // Check if this version already exists
        if let Some(existing) = versions.iter_mut().find(|d| d.version == version) {
            *existing = definition;
        } else {
            versions.push(definition);
            // Keep versions sorted
            versions.sort_by_key(|d| d.version);
        }
    }

    /// Get the latest version of a feature definition by id.
    pub fn get(&self, id: &FeatureId) -> Option<&FeatureDefinition> {
        self.definitions
            .get(id)
            .and_then(|versions| versions.last())
    }

    /// Get a specific version of a feature definition.
    pub fn get_version(&self, id: &FeatureId, version: u32) -> Option<&FeatureDefinition> {
        self.definitions
            .get(id)
            .and_then(|versions| versions.iter().find(|d| d.version == version))
    }

    /// List all registered feature definitions (latest version of each).
    pub fn list(&self) -> Vec<&FeatureDefinition> {
        self.definitions
            .iter()
            .filter_map(|(_, versions)| versions.last())
            .collect()
    }

    /// List all versions of a feature.
    pub fn versions(&self, id: &FeatureId) -> Vec<u32> {
        self.definitions
            .get(id)
            .map(|versions| versions.iter().map(|d| d.version).collect())
            .unwrap_or_default()
    }

    /// Get the dependency lineage for a feature.
    ///
    /// Returns the full dependency chain starting from the given feature,
    /// recursively resolving all transitive dependencies. The result
    /// is ordered from the feature itself, then its direct dependencies,
    /// then their dependencies, etc.
    pub fn lineage(&self, id: &FeatureId) -> Vec<FeatureId> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.lineage_recursive(id, &mut result, &mut visited);
        result
    }

    fn lineage_recursive(
        &self,
        id: &FeatureId,
        result: &mut Vec<FeatureId>,
        visited: &mut std::collections::HashSet<FeatureId>,
    ) {
        if visited.contains(id) {
            return;
        }
        visited.insert(id.clone());

        if let Some(def) = self.get(id) {
            result.push(id.clone());
            for dep in &def.dependencies {
                self.lineage_recursive(dep, result, visited);
            }
        }
    }

    /// Get the number of registered features (unique ids).
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

impl Default for FeatureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::FeatureType;

    #[test]
    fn test_register_and_get() {
        let mut registry = FeatureRegistry::new();
        let def = FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64);
        registry.register(def);

        let retrieved = registry.get(&FeatureId::new("btcusdt_close"));
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().version, 1);
    }

    #[test]
    fn test_register_multiple_versions() {
        let mut registry = FeatureRegistry::new();

        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close", 2, FeatureType::Float64));

        // get() returns latest version
        let latest = registry.get(&FeatureId::new("btcusdt_close")).unwrap();
        assert_eq!(latest.version, 2);
        assert_eq!(latest.expression, "bar.close");

        // get_version() returns specific version
        let v1 = registry.get_version(&FeatureId::new("btcusdt_close"), 1).unwrap();
        assert_eq!(v1.expression, "bar.close_price");
    }

    #[test]
    fn test_list() {
        let mut registry = FeatureRegistry::new();
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));
        registry.register(FeatureDefinition::new("btcusdt_volume", "bar.volume", 1, FeatureType::Float64));

        let list = registry.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_versions() {
        let mut registry = FeatureRegistry::new();
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close", 2, FeatureType::Float64));
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price_v3", 3, FeatureType::Float64));

        let versions = registry.versions(&FeatureId::new("btcusdt_close"));
        assert_eq!(versions, vec![1, 2, 3]);
    }

    #[test]
    fn test_lineage_no_deps() {
        let mut registry = FeatureRegistry::new();
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));

        let lineage = registry.lineage(&FeatureId::new("btcusdt_close"));
        assert_eq!(lineage.len(), 1);
        assert_eq!(lineage[0], FeatureId::new("btcusdt_close"));
    }

    #[test]
    fn test_lineage_with_deps() {
        let mut registry = FeatureRegistry::new();

        // close depends on nothing
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));

        // sma_20 depends on close
        registry.register(
            FeatureDefinition::new("btcusdt_sma_20", "sma(close, 20)", 1, FeatureType::Float64)
                .with_dependency(FeatureId::new("btcusdt_close")),
        );

        // momentum depends on close and sma_20
        registry.register(
            FeatureDefinition::new("btcusdt_momentum", "close - sma_20", 1, FeatureType::Float64)
                .with_dependency(FeatureId::new("btcusdt_close"))
                .with_dependency(FeatureId::new("btcusdt_sma_20")),
        );

        let lineage = registry.lineage(&FeatureId::new("btcusdt_momentum"));
        assert_eq!(lineage.len(), 3);
        assert_eq!(lineage[0], FeatureId::new("btcusdt_momentum"));
        // The exact order of deps may vary, but close and sma_20 should both appear
        assert!(lineage.contains(&FeatureId::new("btcusdt_close")));
        assert!(lineage.contains(&FeatureId::new("btcusdt_sma_20")));
    }

    #[test]
    fn test_lineage_circular_safe() {
        let mut registry = FeatureRegistry::new();
        // Create a circular dependency (shouldn't happen in practice but must not infinite loop)
        let mut def_a = FeatureDefinition::new("feature_a", "feature_b", 1, FeatureType::Float64);
        def_a.dependencies.push(FeatureId::new("feature_b"));

        let mut def_b = FeatureDefinition::new("feature_b", "feature_a", 1, FeatureType::Float64);
        def_b.dependencies.push(FeatureId::new("feature_a"));

        registry.register(def_a);
        registry.register(def_b);

        let lineage = registry.lineage(&FeatureId::new("feature_a"));
        // Should not infinite loop, and should contain both
        assert!(lineage.contains(&FeatureId::new("feature_a")));
        assert!(lineage.contains(&FeatureId::new("feature_b")));
    }

    #[test]
    fn test_replace_existing_version() {
        let mut registry = FeatureRegistry::new();
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64));
        registry.register(FeatureDefinition::new("btcusdt_close", "bar.close_v1_updated", 1, FeatureType::Float64));

        let def = registry.get(&FeatureId::new("btcusdt_close")).unwrap();
        assert_eq!(def.expression, "bar.close_v1_updated");
        assert_eq!(registry.versions(&FeatureId::new("btcusdt_close")), vec![1]);
    }

    #[test]
    fn test_get_missing() {
        let registry = FeatureRegistry::new();
        assert!(registry.get(&FeatureId::new("nonexistent")).is_none());
    }

    #[test]
    fn test_versions_missing() {
        let registry = FeatureRegistry::new();
        assert!(registry.versions(&FeatureId::new("nonexistent")).is_empty());
    }

    #[test]
    fn test_len_and_empty() {
        let mut registry = FeatureRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.register(FeatureDefinition::new("test", "expr", 1, FeatureType::Float64));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }
}
