//! CRDT-backed intent graph using Automerge.
//!
//! Provides conflict-free concurrent modification of the intent graph
//! across multiple agents. Each agent can independently create/update
//! intents, and changes merge automatically without coordination.
//!
//! The CRDT document stores intents as a map of maps:
//! ```text
//! {
//!   "intents": {
//!     "hief-a1b2c3d4": { "title": "...", "status": "draft", ... },
//!     "hief-e5f6g7h8": { "title": "...", "status": "in_progress", ... },
//!   },
//!   "edges": [
//!     { "from": "hief-e5f6g7h8", "to": "hief-a1b2c3d4", "kind": "depends_on" },
//!   ]
//! }
//! ```
//!
//! The SQL database (libsql) serves as a materialized view of the CRDT
//! state, enabling efficient queries while the CRDT handles merging.

use std::path::Path;

use automerge::transaction::Transactable;
use automerge::{ObjType, ReadDoc, ROOT};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::errors::{HiefError, Result};

/// A CRDT-backed intent document using Automerge.
///
/// This wraps an Automerge document that mirrors the intent graph.
/// Multiple agents can fork, modify, and merge their documents
/// without conflicts.
pub struct IntentCrdt {
    doc: automerge::AutoCommit,
}

/// Snapshot of an intent from the CRDT document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtIntent {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: String,
}

/// Snapshot of an edge from the CRDT document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

/// Summary of changes after a merge operation.
#[derive(Debug, Clone, Serialize)]
pub struct MergeSummary {
    pub changes_applied: usize,
    pub intents_after: usize,
    pub edges_after: usize,
}

impl IntentCrdt {
    /// Create a new empty CRDT document.
    pub fn new() -> Self {
        let mut doc = automerge::AutoCommit::new();

        // Initialize top-level structure
        doc.put_object(ROOT, "intents", ObjType::Map)
            .expect("Failed to create intents map");
        doc.put_object(ROOT, "edges", ObjType::List)
            .expect("Failed to create edges list");

        Self { doc }
    }

    /// Load a CRDT document from binary data.
    pub fn load(data: &[u8]) -> Result<Self> {
        let doc = automerge::AutoCommit::load(data)
            .map_err(|e| HiefError::Other(format!("Failed to load CRDT document: {}", e)))?;
        Ok(Self { doc })
    }

    /// Load from a file path.
    pub fn load_file(path: &Path) -> Result<Self> {
        if path.exists() {
            let data = std::fs::read(path)?;
            Self::load(&data)
        } else {
            Ok(Self::new())
        }
    }

    /// Save the CRDT document to binary data.
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    /// Save to a file path.
    pub fn save_file(&mut self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = self.save();
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Add an intent to the CRDT document.
    pub fn put_intent(&mut self, intent: &CrdtIntent) -> Result<()> {
        let intents_id = self.doc.get(ROOT, "intents")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'intents' in CRDT doc".to_string()))?
            .1;

        let intent_obj = self.doc
            .put_object(&intents_id, &intent.id, ObjType::Map)
            .map_err(|e| HiefError::Other(e.to_string()))?;

        self.doc.put(&intent_obj, "kind", intent.kind.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&intent_obj, "title", intent.title.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&intent_obj, "description", intent.description.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&intent_obj, "status", intent.status.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&intent_obj, "priority", intent.priority.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&intent_obj, "assigned_to", intent.assigned_to.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;

        debug!("CRDT: added intent {}", intent.id);
        Ok(())
    }

    /// Update a single field of an intent in the CRDT.
    pub fn update_intent_field(
        &mut self,
        intent_id: &str,
        field: &str,
        value: &str,
    ) -> Result<()> {
        let intents_id = self.doc.get(ROOT, "intents")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'intents' in CRDT doc".to_string()))?
            .1;

        let intent_obj_id = self.doc.get(&intents_id, intent_id)
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::IntentNotFound(intent_id.to_string()))?
            .1;

        self.doc.put(&intent_obj_id, field, value)
            .map_err(|e| HiefError::Other(e.to_string()))?;

        debug!("CRDT: updated {}.{} = {}", intent_id, field, value);
        Ok(())
    }

    /// Get an intent from the CRDT document.
    pub fn get_intent(&self, intent_id: &str) -> Result<Option<CrdtIntent>> {
        let intents_id = self.doc.get(ROOT, "intents")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'intents' in CRDT doc".to_string()))?
            .1;

        let intent_entry = self.doc.get(&intents_id, intent_id)
            .map_err(|e| HiefError::Other(e.to_string()))?;

        let Some((_, intent_obj_id)) = intent_entry else {
            return Ok(None);
        };

        let get_str = |field: &str| -> String {
            self.doc.get(&intent_obj_id, field)
                .ok()
                .flatten()
                .and_then(|(val, _)| val.into_string().ok())
                .unwrap_or_default()
        };

        Ok(Some(CrdtIntent {
            id: intent_id.to_string(),
            kind: get_str("kind"),
            title: get_str("title"),
            description: get_str("description"),
            status: get_str("status"),
            priority: get_str("priority"),
            assigned_to: get_str("assigned_to"),
        }))
    }

    /// List all intents in the CRDT document.
    pub fn list_intents(&self) -> Result<Vec<CrdtIntent>> {
        let intents_id = self.doc.get(ROOT, "intents")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'intents' in CRDT doc".to_string()))?
            .1;

        let keys = self.doc.keys(&intents_id);
        let mut intents = Vec::new();

        for key in keys {
            if let Some(intent) = self.get_intent(&key)? {
                intents.push(intent);
            }
        }

        Ok(intents)
    }

    /// Add an edge to the CRDT document.
    pub fn add_edge(&mut self, edge: &CrdtEdge) -> Result<()> {
        let edges_id = self.doc.get(ROOT, "edges")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'edges' in CRDT doc".to_string()))?
            .1;

        let len = self.doc.length(&edges_id);
        let edge_obj = self.doc.insert_object(&edges_id, len, ObjType::Map)
            .map_err(|e| HiefError::Other(e.to_string()))?;

        self.doc.put(&edge_obj, "from", edge.from.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&edge_obj, "to", edge.to.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;
        self.doc.put(&edge_obj, "kind", edge.kind.as_str())
            .map_err(|e| HiefError::Other(e.to_string()))?;

        debug!("CRDT: added edge {} --[{}]--> {}", edge.from, edge.kind, edge.to);
        Ok(())
    }

    /// List all edges in the CRDT document.
    pub fn list_edges(&self) -> Result<Vec<CrdtEdge>> {
        let edges_id = self.doc.get(ROOT, "edges")
            .map_err(|e| HiefError::Other(e.to_string()))?
            .ok_or_else(|| HiefError::Other("Missing 'edges' in CRDT doc".to_string()))?
            .1;

        let len = self.doc.length(&edges_id);
        let mut edges = Vec::new();

        for i in 0..len {
            let Some((_, edge_obj_id)) = self.doc.get(&edges_id, i)
                .map_err(|e| HiefError::Other(e.to_string()))?
            else {
                continue;
            };

            let get_str = |field: &str| -> String {
                self.doc.get(&edge_obj_id, field)
                    .ok()
                    .flatten()
                    .and_then(|(val, _)| val.into_string().ok())
                    .unwrap_or_default()
            };

            edges.push(CrdtEdge {
                from: get_str("from"),
                to: get_str("to"),
                kind: get_str("kind"),
            });
        }

        Ok(edges)
    }

    /// Merge another CRDT document into this one.
    ///
    /// This is the core multi-agent capability: two agents can independently
    /// modify their copies of the intent graph, then merge without conflicts.
    /// Automerge handles concurrent edits automatically using CRDTs.
    pub fn merge(&mut self, other: &mut IntentCrdt) -> Result<MergeSummary> {
        let changes_before = self.doc.get_changes(&[]).len();

        self.doc.merge(&mut other.doc)
            .map_err(|e| HiefError::Other(format!("CRDT merge failed: {}", e)))?;

        let changes_after = self.doc.get_changes(&[]).len();
        let changes_applied = changes_after.saturating_sub(changes_before);

        let intents_after = self.list_intents()?.len();
        let edges_after = self.list_edges()?.len();

        info!(
            "CRDT merge: {} changes applied, {} intents, {} edges",
            changes_applied, intents_after, edges_after
        );

        Ok(MergeSummary {
            changes_applied,
            intents_after,
            edges_after,
        })
    }

    /// Fork this document for a new agent.
    ///
    /// Creates a deep copy that can be independently modified and later
    /// merged back. This is how multiple agents work on the same graph.
    pub fn fork(&mut self) -> Result<IntentCrdt> {
        let doc = self.doc.fork();
        Ok(IntentCrdt { doc })
    }

    /// Get the number of changes in the document history.
    pub fn change_count(&mut self) -> usize {
        self.doc.get_changes(&[]).len()
    }

    /// Get the actor ID for this document instance.
    pub fn actor_id(&self) -> String {
        self.doc.get_actor().to_hex_string()
    }

    /// Sync the CRDT state to the SQL database.
    ///
    /// This materializes the CRDT intents/edges into libsql tables,
    /// enabling efficient SQL queries while the CRDT handles merging.
    pub async fn sync_to_db(&self, db: &crate::db::Database) -> Result<usize> {
        let intents = self.list_intents()?;
        let mut synced = 0;

        for ci in &intents {
            // Check if intent already exists in DB
            let exists = crate::graph::intent::get(db, &ci.id).await.is_ok();

            if !exists {
                let intent = crate::graph::intent::Intent::new(
                    &ci.kind,
                    &ci.title,
                    if ci.description.is_empty() { None } else { Some(ci.description.clone()) },
                    if ci.priority.is_empty() { None } else { Some(ci.priority.clone()) },
                );
                // Override the generated ID with the CRDT's ID
                let mut intent = intent;
                intent.id = ci.id.clone();
                intent.status = ci.status.clone();

                if let Err(e) = crate::graph::intent::insert(db, &intent).await {
                    debug!("CRDT sync: skipping {} ({})", ci.id, e);
                    continue;
                }
                synced += 1;
            } else {
                // Update status if changed
                let db_intent = crate::graph::intent::get(db, &ci.id).await?;
                if db_intent.status != ci.status && !ci.status.is_empty() {
                    // Direct status update (bypass transition validation for CRDT sync)
                    if let Err(e) = crate::graph::intent::update_status(db, &ci.id, &ci.status).await {
                        debug!("CRDT sync: status update failed for {} ({})", ci.id, e);
                    }
                }
                if !ci.assigned_to.is_empty() && db_intent.assigned_to.as_deref() != Some(&ci.assigned_to) {
                    let _ = crate::graph::intent::assign(db, &ci.id, &ci.assigned_to).await;
                }
            }
        }

        // Sync edges
        let edges = self.list_edges()?;
        for ce in &edges {
            let edge = crate::graph::edges::IntentEdge::new(&ce.from, &ce.to, &ce.kind);
            let _ = crate::graph::edges::insert(db, &edge).await;
        }

        info!("CRDT sync: {} new intents synced to DB", synced);
        Ok(synced)
    }
}

impl Default for IntentCrdt {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic CRDT operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_empty_crdt() {
        let crdt = IntentCrdt::new();
        let intents = crdt.list_intents().unwrap();
        assert!(intents.is_empty());
        let edges = crdt.list_edges().unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_put_and_get_intent() {
        let mut crdt = IntentCrdt::new();
        let intent = CrdtIntent {
            id: "hief-aabbccdd".to_string(),
            kind: "feature".to_string(),
            title: "Test intent".to_string(),
            description: "A test".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        };
        crdt.put_intent(&intent).unwrap();

        let fetched = crdt.get_intent("hief-aabbccdd").unwrap().unwrap();
        assert_eq!(fetched.id, "hief-aabbccdd");
        assert_eq!(fetched.title, "Test intent");
        assert_eq!(fetched.status, "draft");
        assert_eq!(fetched.kind, "feature");
    }

    #[test]
    fn test_list_intents() {
        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-11111111".to_string(),
            kind: "feature".to_string(),
            title: "Intent A".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "high".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();
        crdt.put_intent(&CrdtIntent {
            id: "hief-22222222".to_string(),
            kind: "bug".to_string(),
            title: "Intent B".to_string(),
            description: "".to_string(),
            status: "in_progress".to_string(),
            priority: "critical".to_string(),
            assigned_to: "agent-1".to_string(),
        }).unwrap();

        let intents = crdt.list_intents().unwrap();
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn test_update_intent_field() {
        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-test0001".to_string(),
            kind: "feature".to_string(),
            title: "Original title".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        crdt.update_intent_field("hief-test0001", "status", "approved").unwrap();
        crdt.update_intent_field("hief-test0001", "assigned_to", "agent-claude").unwrap();

        let fetched = crdt.get_intent("hief-test0001").unwrap().unwrap();
        assert_eq!(fetched.status, "approved");
        assert_eq!(fetched.assigned_to, "agent-claude");
    }

    #[test]
    fn test_update_nonexistent_intent_fails() {
        let mut crdt = IntentCrdt::new();
        let result = crdt.update_intent_field("nonexistent", "status", "approved");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent_intent_returns_none() {
        let crdt = IntentCrdt::new();
        let result = crdt.get_intent("nonexistent").unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Edge operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_and_list_edges() {
        let mut crdt = IntentCrdt::new();
        crdt.add_edge(&CrdtEdge {
            from: "hief-aaaa".to_string(),
            to: "hief-bbbb".to_string(),
            kind: "depends_on".to_string(),
        }).unwrap();
        crdt.add_edge(&CrdtEdge {
            from: "hief-cccc".to_string(),
            to: "hief-aaaa".to_string(),
            kind: "blocks".to_string(),
        }).unwrap();

        let edges = crdt.list_edges().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].from, "hief-aaaa");
        assert_eq!(edges[0].kind, "depends_on");
    }

    // -----------------------------------------------------------------------
    // Merge / Fork tests (the core multi-agent capability)
    // -----------------------------------------------------------------------

    #[test]
    fn test_fork_and_merge() {
        let mut crdt1 = IntentCrdt::new();
        crdt1.put_intent(&CrdtIntent {
            id: "hief-shared01".to_string(),
            kind: "feature".to_string(),
            title: "Shared intent".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        // Fork for agent 2
        let mut crdt2 = crdt1.fork().unwrap();

        // Agent 1 adds a new intent
        crdt1.put_intent(&CrdtIntent {
            id: "hief-agent1-01".to_string(),
            kind: "bug".to_string(),
            title: "Agent 1's bug".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "high".to_string(),
            assigned_to: "agent-1".to_string(),
        }).unwrap();

        // Agent 2 adds a different intent
        crdt2.put_intent(&CrdtIntent {
            id: "hief-agent2-01".to_string(),
            kind: "refactor".to_string(),
            title: "Agent 2's refactor".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "low".to_string(),
            assigned_to: "agent-2".to_string(),
        }).unwrap();

        // Merge agent 2's changes into agent 1's doc
        let summary = crdt1.merge(&mut crdt2).unwrap();
        assert_eq!(summary.intents_after, 3, "Should have 3 intents after merge");

        // All three intents should be present
        let intents = crdt1.list_intents().unwrap();
        let ids: Vec<&str> = intents.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"hief-shared01"));
        assert!(ids.contains(&"hief-agent1-01"));
        assert!(ids.contains(&"hief-agent2-01"));
    }

    #[test]
    fn test_concurrent_status_update_merge() {
        let mut crdt1 = IntentCrdt::new();
        crdt1.put_intent(&CrdtIntent {
            id: "hief-conflict1".to_string(),
            kind: "feature".to_string(),
            title: "Contested intent".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        let mut crdt2 = crdt1.fork().unwrap();

        // Agent 1 updates status to approved
        crdt1.update_intent_field("hief-conflict1", "status", "approved").unwrap();

        // Agent 2 updates status to rejected (concurrent conflicting edit)
        crdt2.update_intent_field("hief-conflict1", "status", "rejected").unwrap();

        // Merge — Automerge resolves the conflict deterministically
        // (last-writer-wins per field, based on actor ID ordering)
        let summary = crdt1.merge(&mut crdt2).unwrap();
        assert_eq!(summary.intents_after, 1);

        let intent = crdt1.get_intent("hief-conflict1").unwrap().unwrap();
        // The status will be one of "approved" or "rejected" — Automerge
        // picks a deterministic winner. The key invariant is that both
        // agents converge to the same value after merging.
        assert!(
            intent.status == "approved" || intent.status == "rejected",
            "Status should be one of the two values, got: {}",
            intent.status
        );
    }

    #[test]
    fn test_concurrent_different_fields_merge() {
        let mut crdt1 = IntentCrdt::new();
        crdt1.put_intent(&CrdtIntent {
            id: "hief-noconflict".to_string(),
            kind: "feature".to_string(),
            title: "No conflict".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        let mut crdt2 = crdt1.fork().unwrap();

        // Agent 1 updates status
        crdt1.update_intent_field("hief-noconflict", "status", "approved").unwrap();

        // Agent 2 updates assigned_to (different field — no conflict)
        crdt2.update_intent_field("hief-noconflict", "assigned_to", "agent-2").unwrap();

        crdt1.merge(&mut crdt2).unwrap();

        let intent = crdt1.get_intent("hief-noconflict").unwrap().unwrap();
        assert_eq!(intent.status, "approved", "Status should be updated by agent 1");
        assert_eq!(intent.assigned_to, "agent-2", "assigned_to should be updated by agent 2");
    }

    // -----------------------------------------------------------------------
    // Save / Load persistence
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_load() {
        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-persist01".to_string(),
            kind: "feature".to_string(),
            title: "Persistent intent".to_string(),
            description: "desc".to_string(),
            status: "draft".to_string(),
            priority: "high".to_string(),
            assigned_to: "agent-x".to_string(),
        }).unwrap();
        crdt.add_edge(&CrdtEdge {
            from: "hief-persist01".to_string(),
            to: "hief-persist02".to_string(),
            kind: "depends_on".to_string(),
        }).unwrap();

        let data = crdt.save();
        assert!(!data.is_empty(), "Saved data should be non-empty");

        let loaded = IntentCrdt::load(&data).unwrap();
        let intents = loaded.list_intents().unwrap();
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].title, "Persistent intent");

        let edges = loaded.list_edges().unwrap();
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_save_and_load_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.crdt");

        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-file01".to_string(),
            kind: "feature".to_string(),
            title: "File intent".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        crdt.save_file(&path).unwrap();
        assert!(path.exists());

        let loaded = IntentCrdt::load_file(&path).unwrap();
        let intents = loaded.list_intents().unwrap();
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].id, "hief-file01");
    }

    #[test]
    fn test_load_nonexistent_file_creates_new() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.crdt");
        let crdt = IntentCrdt::load_file(&path).unwrap();
        assert!(crdt.list_intents().unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Actor identity
    // -----------------------------------------------------------------------

    #[test]
    fn test_unique_actor_ids() {
        let crdt1 = IntentCrdt::new();
        let crdt2 = IntentCrdt::new();
        assert_ne!(
            crdt1.actor_id(),
            crdt2.actor_id(),
            "Different documents should have different actor IDs"
        );
    }

    #[test]
    fn test_actor_id_format() {
        let crdt = IntentCrdt::new();
        let actor = crdt.actor_id();
        assert!(!actor.is_empty());
        assert!(actor.len() > 8, "Actor ID should be a hex string");
    }

    // -----------------------------------------------------------------------
    // Change tracking
    // -----------------------------------------------------------------------

    #[test]
    fn test_change_count() {
        let mut crdt = IntentCrdt::new();
        let initial = crdt.change_count();

        crdt.put_intent(&CrdtIntent {
            id: "hief-changes01".to_string(),
            kind: "feature".to_string(),
            title: "Track changes".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        assert!(crdt.change_count() > initial, "Change count should increase after put");
    }

    // -----------------------------------------------------------------------
    // DB sync tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sync_to_db() {
        let db = crate::db::Database::open_memory().await.unwrap();

        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-sync0001".to_string(),
            kind: "feature".to_string(),
            title: "Synced intent".to_string(),
            description: "From CRDT".to_string(),
            status: "draft".to_string(),
            priority: "high".to_string(),
            assigned_to: "agent-crdt".to_string(),
        }).unwrap();

        let synced = crdt.sync_to_db(&db).await.unwrap();
        assert_eq!(synced, 1, "Should sync 1 intent to DB");

        // Verify in DB
        let db_intent = crate::graph::intent::get(&db, "hief-sync0001").await.unwrap();
        assert_eq!(db_intent.title, "Synced intent");
        assert_eq!(db_intent.priority, "high");
    }

    #[tokio::test]
    async fn test_sync_to_db_idempotent() {
        let db = crate::db::Database::open_memory().await.unwrap();

        let mut crdt = IntentCrdt::new();
        crdt.put_intent(&CrdtIntent {
            id: "hief-idempot01".to_string(),
            kind: "feature".to_string(),
            title: "Idempotent".to_string(),
            description: "".to_string(),
            status: "draft".to_string(),
            priority: "medium".to_string(),
            assigned_to: "".to_string(),
        }).unwrap();

        let first = crdt.sync_to_db(&db).await.unwrap();
        assert_eq!(first, 1);

        // Sync again — should be idempotent
        let second = crdt.sync_to_db(&db).await.unwrap();
        assert_eq!(second, 0, "Second sync should not create duplicates");
    }
}
