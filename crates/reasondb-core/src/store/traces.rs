//! Query trace storage operations

use redb::ReadableTable;

use super::{NodeStore, TRACES_TABLE};
use crate::error::{Result, StorageError};
use crate::trace::{QueryTrace, QueryTraceSummary};

impl NodeStore {
    /// Persist a completed query trace.
    pub fn save_trace(&self, trace: &QueryTrace) -> Result<()> {
        let key = trace.trace_id.as_str();
        tracing::info!(trace_id = %key, table_id = %trace.table_id, "Persisting query trace to store");
        let value = bincode::serialize(trace).map_err(|e| {
            tracing::error!(trace_id = %key, "Failed to serialize trace: {}", e);
            e
        })?;

        tracing::debug!(trace_id = %key, bytes = value.len(), "Trace serialized, opening write transaction");
        let write_txn = self.db.begin_write().map_err(|e| {
            tracing::error!(trace_id = %key, "Failed to begin write transaction for trace: {}", e);
            StorageError::from(e)
        })?;
        {
            let mut t = write_txn
                .open_table(TRACES_TABLE)
                .map_err(StorageError::from)?;
            t.insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(|e| {
            tracing::error!(trace_id = %key, "Failed to commit trace write transaction: {}", e);
            StorageError::from(e)
        })?;
        tracing::info!(trace_id = %key, "Query trace persisted successfully");
        Ok(())
    }

    /// Retrieve a single trace by ID.
    pub fn get_trace(&self, trace_id: &str) -> Result<Option<QueryTrace>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let t = read_txn
            .open_table(TRACES_TABLE)
            .map_err(StorageError::from)?;

        let Some(val) = t
            .get(trace_id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        else {
            return Ok(None);
        };

        let trace = bincode::deserialize(val.value())?;
        Ok(Some(trace))
    }

    /// List recent traces for a table, newest first.
    pub fn list_traces(&self, table_id: &str, limit: usize) -> Result<Vec<QueryTraceSummary>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let t = read_txn
            .open_table(TRACES_TABLE)
            .map_err(StorageError::from)?;

        let mut summaries = Vec::new();
        for result in t
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            let (_, val) = result.map_err(|e| StorageError::TableError(e.to_string()))?;
            if let Ok(trace) = bincode::deserialize::<QueryTrace>(val.value()) {
                if trace.table_id == table_id {
                    summaries.push(QueryTraceSummary::from(&trace));
                }
            }
        }

        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        summaries.truncate(limit);
        Ok(summaries)
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use tempfile::tempdir;

    use super::*;
    use crate::trace::{
        BeamReasoningTrace, Bm25SelectionTrace, FinalResultTrace, LlmRankingTrace,
        StructuralFilterTrace,
    };

    fn create_test_store() -> (NodeStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = NodeStore::open(&db_path).unwrap();
        (store, dir)
    }

    fn make_trace(trace_id: &str, table_id: &str, ts_secs: i64) -> QueryTrace {
        QueryTrace {
            trace_id: trace_id.to_string(),
            query: "what is covered?".to_string(),
            table_id: table_id.to_string(),
            created_at: DateTime::from_timestamp(ts_secs, 0).unwrap(),
            duration_ms: 100,
            decomposition: None,
            bm25_selection: Bm25SelectionTrace {
                total_candidates: 5,
                hits: vec![],
            },
            structural_filter: StructuralFilterTrace {
                terms: vec![],
                filtered_count: 3,
                scores: vec![],
            },
            llm_ranking: LlmRankingTrace {
                input_count: 3,
                selected_count: 2,
                skipped_llm: false,
                rankings: vec![],
            },
            beam_reasoning: BeamReasoningTrace {
                documents_processed: 2,
                total_llm_calls: 4,
                documents: vec![],
            },
            final_results: vec![FinalResultTrace {
                document_id: "doc_1".to_string(),
                document_title: "Policy Doc".to_string(),
                node_id: "node_1".to_string(),
                node_title: "Coverage".to_string(),
                confidence: 0.92,
                path: vec!["root".to_string(), "Coverage".to_string()],
            }],
        }
    }

    #[test]
    fn test_save_and_get_trace() {
        let (store, _dir) = create_test_store();
        let trace = make_trace("tr-abc", "tbl-1", 1_700_000_000);

        store.save_trace(&trace).unwrap();

        let retrieved = store.get_trace("tr-abc").unwrap().unwrap();
        assert_eq!(retrieved.trace_id, "tr-abc");
        assert_eq!(retrieved.table_id, "tbl-1");
        assert_eq!(retrieved.query, "what is covered?");
        assert_eq!(retrieved.final_results.len(), 1);
        assert_eq!(retrieved.final_results[0].node_title, "Coverage");
        assert_eq!(retrieved.bm25_selection.total_candidates, 5);
        assert_eq!(retrieved.beam_reasoning.total_llm_calls, 4);
    }

    #[test]
    fn test_get_nonexistent_trace() {
        let (store, _dir) = create_test_store();
        let result = store.get_trace("does-not-exist").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_traces_filters_by_table() {
        let (store, _dir) = create_test_store();

        store
            .save_trace(&make_trace("tr-1", "tbl-A", 1_700_000_001))
            .unwrap();
        store
            .save_trace(&make_trace("tr-2", "tbl-A", 1_700_000_002))
            .unwrap();
        store
            .save_trace(&make_trace("tr-3", "tbl-A", 1_700_000_003))
            .unwrap();
        store
            .save_trace(&make_trace("tr-4", "tbl-B", 1_700_000_004))
            .unwrap();

        let tbl_a = store.list_traces("tbl-A", 10).unwrap();
        assert_eq!(tbl_a.len(), 3);
        assert!(tbl_a.iter().all(|s| s.table_id == "tbl-A"));

        let tbl_b = store.list_traces("tbl-B", 10).unwrap();
        assert_eq!(tbl_b.len(), 1);
        assert_eq!(tbl_b[0].trace_id, "tr-4");

        let missing = store.list_traces("tbl-C", 10).unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_list_traces_newest_first() {
        let (store, _dir) = create_test_store();

        store
            .save_trace(&make_trace("tr-old", "tbl-1", 1_700_000_000))
            .unwrap();
        store
            .save_trace(&make_trace("tr-new", "tbl-1", 1_700_000_999))
            .unwrap();

        let results = store.list_traces("tbl-1", 10).unwrap();
        assert_eq!(results[0].trace_id, "tr-new");
        assert_eq!(results[1].trace_id, "tr-old");
    }

    #[test]
    fn test_list_traces_respects_limit() {
        let (store, _dir) = create_test_store();

        for i in 0..5 {
            store
                .save_trace(&make_trace(&format!("tr-{i}"), "tbl-1", 1_700_000_000 + i))
                .unwrap();
        }

        let results = store.list_traces("tbl-1", 2).unwrap();
        assert_eq!(results.len(), 2);
    }
}
