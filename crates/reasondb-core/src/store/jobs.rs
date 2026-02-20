//! Job persistence for durable ingestion queue
//!
//! Stores ingestion jobs in redb so they survive server restarts.

use redb::ReadableTable;

use super::{NodeStore, JOBS_TABLE, JOBS_ORDER_TABLE};
use crate::error::{Result, StorageError};

impl NodeStore {
    /// Insert a new job into the database.
    pub fn insert_job(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(JOBS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(id, data)
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Append to order index using timestamp-prefixed key for ordering
            let mut order_table = write_txn
                .open_table(JOBS_ORDER_TABLE)
                .map_err(StorageError::from)?;
            let seq_key = format!("{}_{}", chrono::Utc::now().timestamp_millis(), id);
            order_table
                .insert(seq_key.as_str(), id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Update an existing job in the database.
    pub fn update_job(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(JOBS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(id, data)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Get a job by its ID.
    pub fn get_job(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(JOBS_TABLE)
            .map_err(StorageError::from)?;

        match table
            .get(id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => Ok(Some(value.value().to_vec())),
            None => Ok(None),
        }
    }

    /// List recent jobs (newest first), up to `limit`.
    pub fn list_jobs(&self, limit: usize) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let jobs_table = read_txn
            .open_table(JOBS_TABLE)
            .map_err(StorageError::from)?;
        let order_table = read_txn
            .open_table(JOBS_ORDER_TABLE)
            .map_err(StorageError::from)?;

        let mut results = Vec::new();
        // Iterate in reverse (newest first) using range over all keys
        let iter = order_table
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?;

        let entries: Vec<_> = iter
            .filter_map(|r| r.ok())
            .map(|(_, v)| v.value().to_string())
            .collect();

        for job_id in entries.iter().rev().take(limit) {
            if let Ok(Some(value)) = jobs_table.get(job_id.as_str()).map(|o| o.map(|v| v.value().to_vec())) {
                results.push(value);
            }
        }

        Ok(results)
    }

    /// Get all jobs with a specific status byte prefix.
    /// The caller is responsible for deserializing and filtering.
    pub fn get_all_jobs(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(JOBS_TABLE)
            .map_err(StorageError::from)?;

        let mut results = Vec::new();
        let iter = table
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?;

        for entry in iter {
            let (key, value) = entry.map_err(|e| StorageError::TableError(e.to_string()))?;
            results.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(results)
    }

    /// Delete a job by ID.
    pub fn delete_job(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        let deleted;
        {
            let mut table = write_txn
                .open_table(JOBS_TABLE)
                .map_err(StorageError::from)?;
            let removed = table
                .remove(id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
            deleted = removed.is_some();
        }

        if deleted {
            let mut order_table = write_txn
                .open_table(JOBS_ORDER_TABLE)
                .map_err(StorageError::from)?;
            let keys_to_remove: Vec<String> = {
                let iter = order_table
                    .iter()
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
                iter.filter_map(|r| r.ok())
                    .filter(|(_, v)| v.value() == id)
                    .map(|(k, _)| k.value().to_string())
                    .collect()
            };
            for key in keys_to_remove {
                order_table
                    .remove(key.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }

        write_txn.commit().map_err(StorageError::from)?;
        Ok(deleted)
    }

    /// Delete multiple jobs by ID in a single transaction.
    pub fn delete_jobs(&self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        let mut deleted_count = 0;
        {
            let mut table = write_txn
                .open_table(JOBS_TABLE)
                .map_err(StorageError::from)?;
            let mut order_table = write_txn
                .open_table(JOBS_ORDER_TABLE)
                .map_err(StorageError::from)?;

            for id in ids {
                if table
                    .remove(id.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?
                    .is_some()
                {
                    deleted_count += 1;
                }
            }

            // Batch clean order table
            let keys_to_remove: Vec<String> = {
                let iter = order_table
                    .iter()
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
                iter.filter_map(|r| r.ok())
                    .filter(|(_, v)| ids.iter().any(|id| id.as_str() == v.value()))
                    .map(|(k, _)| k.value().to_string())
                    .collect()
            };
            for key in keys_to_remove {
                order_table
                    .remove(key.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(deleted_count)
    }

    /// Atomically claim the next queued job by setting its data to `new_data`.
    /// Returns the job ID and old data if a job was claimed, None otherwise.
    /// The caller provides a predicate function to identify queued jobs.
    pub fn claim_next_job<F>(&self, is_queued: F, new_data: &[u8]) -> Result<Option<(String, Vec<u8>)>>
    where
        F: Fn(&[u8]) -> bool,
    {
        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        let result = {
            let order_table = write_txn
                .open_table(JOBS_ORDER_TABLE)
                .map_err(StorageError::from)?;

            // Collect ordered job IDs first
            let entries: Vec<String> = {
                let iter = order_table
                    .iter()
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
                iter.filter_map(|r| r.ok())
                    .map(|(_, v)| v.value().to_string())
                    .collect()
            };

            // Read phase: find the first queued job
            let jobs_table_ro = write_txn
                .open_table(JOBS_TABLE)
                .map_err(StorageError::from)?;
            let mut candidate: Option<(String, Vec<u8>)> = None;
            for job_id in &entries {
                if let Some(guard) = jobs_table_ro
                    .get(job_id.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?
                {
                    let old_data = guard.value().to_vec();
                    drop(guard);
                    if is_queued(&old_data) {
                        candidate = Some((job_id.clone(), old_data));
                        break;
                    }
                }
            }
            drop(jobs_table_ro);

            // Write phase: update the claimed job
            if let Some((ref job_id, _)) = candidate {
                let mut jobs_table_rw = write_txn
                    .open_table(JOBS_TABLE)
                    .map_err(StorageError::from)?;
                jobs_table_rw
                    .insert(job_id.as_str(), new_data)
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }

            candidate
        };
        write_txn.commit().map_err(StorageError::from)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_store() -> (NodeStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_jobs.db");
        let store = NodeStore::open(&db_path).unwrap();
        (store, dir)
    }

    #[test]
    fn test_insert_and_get_job() {
        let (store, _dir) = create_test_store();
        let data = b"hello world";

        store.insert_job("job_1", data).unwrap();

        let retrieved = store.get_job("job_1").unwrap();
        assert_eq!(retrieved, Some(data.to_vec()));
    }

    #[test]
    fn test_get_nonexistent_job() {
        let (store, _dir) = create_test_store();
        let retrieved = store.get_job("nonexistent").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_update_job() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"original").unwrap();
        store.update_job("job_1", b"updated").unwrap();

        let retrieved = store.get_job("job_1").unwrap().unwrap();
        assert_eq!(retrieved, b"updated");
    }

    #[test]
    fn test_delete_job() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"data").unwrap();
        assert!(store.get_job("job_1").unwrap().is_some());

        let deleted = store.delete_job("job_1").unwrap();
        assert!(deleted);
        assert!(store.get_job("job_1").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent_job() {
        let (store, _dir) = create_test_store();
        let deleted = store.delete_job("nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_delete_jobs_batch() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"data1").unwrap();
        store.insert_job("job_2", b"data2").unwrap();
        store.insert_job("job_3", b"data3").unwrap();

        let deleted = store
            .delete_jobs(&["job_1".to_string(), "job_3".to_string()])
            .unwrap();
        assert_eq!(deleted, 2);

        assert!(store.get_job("job_1").unwrap().is_none());
        assert!(store.get_job("job_2").unwrap().is_some());
        assert!(store.get_job("job_3").unwrap().is_none());
    }

    #[test]
    fn test_delete_jobs_empty_list() {
        let (store, _dir) = create_test_store();
        let deleted = store.delete_jobs(&[]).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_get_all_jobs() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_a", b"data_a").unwrap();
        store.insert_job("job_b", b"data_b").unwrap();

        let all = store.get_all_jobs().unwrap();
        assert_eq!(all.len(), 2);

        let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"job_a"));
        assert!(ids.contains(&"job_b"));
    }

    #[test]
    fn test_list_jobs_ordering() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_first", b"first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.insert_job("job_second", b"second").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.insert_job("job_third", b"third").unwrap();

        let listed = store.list_jobs(2).unwrap();
        assert_eq!(listed.len(), 2);
        // Newest first
        assert_eq!(listed[0], b"third");
        assert_eq!(listed[1], b"second");
    }

    #[test]
    fn test_list_jobs_with_limit() {
        let (store, _dir) = create_test_store();

        for i in 0..5 {
            store
                .insert_job(&format!("job_{}", i), format!("data_{}", i).as_bytes())
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let listed = store.list_jobs(3).unwrap();
        assert_eq!(listed.len(), 3);
    }

    #[test]
    fn test_claim_next_job() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"queued").unwrap();
        store.insert_job("job_2", b"queued").unwrap();

        let claimed = store
            .claim_next_job(|data| data == b"queued", b"processing")
            .unwrap();

        assert!(claimed.is_some());
        let (id, old_data) = claimed.unwrap();
        assert_eq!(id, "job_1");
        assert_eq!(old_data, b"queued");

        // Verify the data was updated in storage
        let updated = store.get_job("job_1").unwrap().unwrap();
        assert_eq!(updated, b"processing");
    }

    #[test]
    fn test_claim_skips_non_queued() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"processing").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.insert_job("job_2", b"queued").unwrap();

        let claimed = store
            .claim_next_job(|data| data == b"queued", b"processing")
            .unwrap();

        assert!(claimed.is_some());
        let (id, _) = claimed.unwrap();
        assert_eq!(id, "job_2");
    }

    #[test]
    fn test_claim_returns_none_when_empty() {
        let (store, _dir) = create_test_store();

        let claimed = store
            .claim_next_job(|data| data == b"queued", b"processing")
            .unwrap();
        assert!(claimed.is_none());
    }

    #[test]
    fn test_claim_returns_none_when_all_taken() {
        let (store, _dir) = create_test_store();

        store.insert_job("job_1", b"processing").unwrap();
        store.insert_job("job_2", b"completed").unwrap();

        let claimed = store
            .claim_next_job(|data| data == b"queued", b"processing")
            .unwrap();
        assert!(claimed.is_none());
    }

    #[test]
    fn test_job_survives_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("persist_test.db");

        {
            let store = NodeStore::open(&db_path).unwrap();
            store.insert_job("persistent_job", b"important_data").unwrap();
        }

        // Reopen the store and verify persistence
        let store = NodeStore::open(&db_path).unwrap();
        let data = store.get_job("persistent_job").unwrap();
        assert_eq!(data, Some(b"important_data".to_vec()));
    }
}
