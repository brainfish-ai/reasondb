//! Rate limit state persistence
//!
//! Snapshots rate limit bucket state to redb so it survives restarts.
//! Uses wall-clock timestamps for serialization (Instant is non-serializable).

use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use super::{NodeStore, RATE_LIMITS_TABLE};
use crate::error::{Result, StorageError};

/// Serializable snapshot of a rate limit bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitSnapshot {
    pub tokens: f64,
    pub max_tokens: f64,
    pub refill_rate: f64,
    pub hourly_count: u32,
    /// Seconds since Unix epoch when last accessed
    pub last_access_epoch: i64,
    /// Seconds since Unix epoch when hour started
    pub hour_start_epoch: i64,
}

impl NodeStore {
    /// Save a rate limit snapshot for a client.
    pub fn save_rate_limit(&self, client_key: &str, snapshot: &RateLimitSnapshot) -> Result<()> {
        let data = bincode::serialize(snapshot)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(RATE_LIMITS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(client_key, data.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Save multiple rate limit snapshots in a single transaction.
    pub fn save_rate_limits(&self, snapshots: &[(&str, RateLimitSnapshot)]) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(RATE_LIMITS_TABLE)
                .map_err(StorageError::from)?;

            for (client_key, snapshot) in snapshots {
                let data = bincode::serialize(snapshot)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                table
                    .insert(*client_key, data.as_slice())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Load a rate limit snapshot for a client.
    pub fn load_rate_limit(&self, client_key: &str) -> Result<Option<RateLimitSnapshot>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(RATE_LIMITS_TABLE)
            .map_err(StorageError::from)?;

        match table
            .get(client_key)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => {
                let snapshot: RateLimitSnapshot = bincode::deserialize(value.value())
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    }

    /// Load all rate limit snapshots.
    pub fn load_all_rate_limits(&self) -> Result<Vec<(String, RateLimitSnapshot)>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(RATE_LIMITS_TABLE)
            .map_err(StorageError::from)?;

        let mut results = Vec::new();
        let iter = table
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?;

        for entry in iter {
            let (key, value) = entry.map_err(|e| StorageError::TableError(e.to_string()))?;
            if let Ok(snapshot) = bincode::deserialize::<RateLimitSnapshot>(value.value()) {
                results.push((key.value().to_string(), snapshot));
            }
        }

        Ok(results)
    }

    /// Clear all rate limit snapshots.
    pub fn clear_rate_limits(&self) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(RATE_LIMITS_TABLE)
                .map_err(StorageError::from)?;

            // Collect all keys first
            let keys: Vec<String> = {
                let iter = table
                    .iter()
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
                iter.filter_map(|r| r.ok())
                    .map(|(k, _)| k.value().to_string())
                    .collect()
            };

            for key in keys {
                table
                    .remove(key.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_store() -> (NodeStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_rate_limits.db");
        let store = NodeStore::open(&db_path).unwrap();
        (store, dir)
    }

    fn make_snapshot(tokens: f64) -> RateLimitSnapshot {
        let now = chrono::Utc::now().timestamp();
        RateLimitSnapshot {
            tokens,
            max_tokens: 10.0,
            refill_rate: 1.0,
            hourly_count: 5,
            last_access_epoch: now,
            hour_start_epoch: now - 1800,
        }
    }

    #[test]
    fn test_save_and_load_rate_limit() {
        let (store, _dir) = create_test_store();
        let snapshot = make_snapshot(8.5);

        store.save_rate_limit("client_1", &snapshot).unwrap();
        let loaded = store.load_rate_limit("client_1").unwrap().unwrap();

        assert!((loaded.tokens - 8.5).abs() < f64::EPSILON);
        assert!((loaded.max_tokens - 10.0).abs() < f64::EPSILON);
        assert!((loaded.refill_rate - 1.0).abs() < f64::EPSILON);
        assert_eq!(loaded.hourly_count, 5);
    }

    #[test]
    fn test_load_nonexistent_rate_limit() {
        let (store, _dir) = create_test_store();
        let loaded = store.load_rate_limit("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_save_rate_limits_batch() {
        let (store, _dir) = create_test_store();

        let snapshots = vec![
            ("client_a", make_snapshot(5.0)),
            ("client_b", make_snapshot(3.0)),
            ("client_c", make_snapshot(9.0)),
        ];

        store.save_rate_limits(&snapshots).unwrap();

        let a = store.load_rate_limit("client_a").unwrap().unwrap();
        let b = store.load_rate_limit("client_b").unwrap().unwrap();
        let c = store.load_rate_limit("client_c").unwrap().unwrap();

        assert!((a.tokens - 5.0).abs() < f64::EPSILON);
        assert!((b.tokens - 3.0).abs() < f64::EPSILON);
        assert!((c.tokens - 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_save_rate_limits_empty_batch() {
        let (store, _dir) = create_test_store();
        store.save_rate_limits(&[]).unwrap();
    }

    #[test]
    fn test_load_all_rate_limits() {
        let (store, _dir) = create_test_store();

        store.save_rate_limit("client_x", &make_snapshot(1.0)).unwrap();
        store.save_rate_limit("client_y", &make_snapshot(2.0)).unwrap();

        let all = store.load_all_rate_limits().unwrap();
        assert_eq!(all.len(), 2);

        let keys: Vec<&str> = all.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"client_x"));
        assert!(keys.contains(&"client_y"));
    }

    #[test]
    fn test_clear_rate_limits() {
        let (store, _dir) = create_test_store();

        store.save_rate_limit("client_1", &make_snapshot(5.0)).unwrap();
        store.save_rate_limit("client_2", &make_snapshot(3.0)).unwrap();

        assert_eq!(store.load_all_rate_limits().unwrap().len(), 2);

        store.clear_rate_limits().unwrap();

        assert_eq!(store.load_all_rate_limits().unwrap().len(), 0);
        assert!(store.load_rate_limit("client_1").unwrap().is_none());
    }

    #[test]
    fn test_overwrite_rate_limit() {
        let (store, _dir) = create_test_store();

        store.save_rate_limit("client_1", &make_snapshot(10.0)).unwrap();
        store.save_rate_limit("client_1", &make_snapshot(2.0)).unwrap();

        let loaded = store.load_rate_limit("client_1").unwrap().unwrap();
        assert!((loaded.tokens - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rate_limit_survives_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("persist_rate.db");

        {
            let store = NodeStore::open(&db_path).unwrap();
            store.save_rate_limit("persistent", &make_snapshot(7.7)).unwrap();
        }

        let store = NodeStore::open(&db_path).unwrap();
        let loaded = store.load_rate_limit("persistent").unwrap().unwrap();
        assert!((loaded.tokens - 7.7).abs() < f64::EPSILON);
    }
}
