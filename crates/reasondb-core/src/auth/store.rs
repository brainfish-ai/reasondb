//! API key storage using redb

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::Arc;

use super::{hash_api_key, ApiKey, ApiKeyMetadata};
use crate::error::ReasonDBError;

/// Table definition for API keys
const API_KEYS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("api_keys");

/// Index: key_hash -> key_id for fast lookups
const API_KEYS_HASH_INDEX: TableDefinition<&str, &str> = TableDefinition::new("api_keys_hash_idx");

/// Storage for API keys
pub struct ApiKeyStore {
    db: Arc<Database>,
}

impl ApiKeyStore {
    /// Create a new API key store
    pub fn new(db: Arc<Database>) -> Result<Self, ReasonDBError> {
        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(API_KEYS_TABLE)?;
            let _ = write_txn.open_table(API_KEYS_HASH_INDEX)?;
        }
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store a new API key
    pub fn insert(&self, key: &ApiKey) -> Result<(), ReasonDBError> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(API_KEYS_TABLE)?;
            let mut hash_idx = write_txn.open_table(API_KEYS_HASH_INDEX)?;

            // Serialize and store the key
            let data = bincode::serialize(key)?;
            table.insert(key.id.as_str(), data.as_slice())?;

            // Add hash index for fast lookup
            hash_idx.insert(key.key_hash.as_str(), key.id.as_str())?;
        }
        write_txn.commit()?;

        Ok(())
    }

    /// Get an API key by ID
    pub fn get(&self, id: &str) -> Result<Option<ApiKey>, ReasonDBError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(API_KEYS_TABLE)?;

        if let Some(data) = table.get(id)? {
            let key: ApiKey = bincode::deserialize(data.value())?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    /// Find an API key by its hash (used during authentication)
    pub fn find_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, ReasonDBError> {
        let read_txn = self.db.begin_read()?;
        let hash_idx = read_txn.open_table(API_KEYS_HASH_INDEX)?;

        if let Some(key_id) = hash_idx.get(key_hash)? {
            let table = read_txn.open_table(API_KEYS_TABLE)?;
            if let Some(data) = table.get(key_id.value())? {
                let key: ApiKey = bincode::deserialize(data.value())?;
                return Ok(Some(key));
            }
        }

        Ok(None)
    }

    /// Authenticate a raw API key and return the key if valid
    pub fn authenticate(&self, raw_key: &str) -> Result<Option<ApiKey>, ReasonDBError> {
        let key_hash = hash_api_key(raw_key);

        if let Some(key) = self.find_by_hash(&key_hash)? {
            if key.is_valid() {
                return Ok(Some(key));
            }
        }

        Ok(None)
    }

    /// Update an API key (e.g., mark as used, update permissions)
    pub fn update(&self, key: &ApiKey) -> Result<(), ReasonDBError> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(API_KEYS_TABLE)?;

            // Ensure key exists
            if table.get(key.id.as_str())?.is_none() {
                return Err(ReasonDBError::NotFound(format!(
                    "API key not found: {}",
                    key.id
                )));
            }

            let data = bincode::serialize(key)?;
            table.insert(key.id.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;

        Ok(())
    }

    /// Revoke (deactivate) an API key
    pub fn revoke(&self, id: &str) -> Result<(), ReasonDBError> {
        if let Some(mut key) = self.get(id)? {
            key.is_active = false;
            self.update(&key)?;
            Ok(())
        } else {
            Err(ReasonDBError::NotFound(format!(
                "API key not found: {}",
                id
            )))
        }
    }

    /// Delete an API key permanently
    pub fn delete(&self, id: &str) -> Result<(), ReasonDBError> {
        // First, get the key hash using a read transaction
        let key_hash = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(API_KEYS_TABLE)?;

            if let Some(data) = table.get(id)? {
                let key: ApiKey = bincode::deserialize(data.value())?;
                key.key_hash
            } else {
                return Err(ReasonDBError::NotFound(format!(
                    "API key not found: {}",
                    id
                )));
            }
        };

        // Now delete using a write transaction
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(API_KEYS_TABLE)?;
            let mut hash_idx = write_txn.open_table(API_KEYS_HASH_INDEX)?;

            hash_idx.remove(key_hash.as_str())?;
            table.remove(id)?;
        }
        write_txn.commit()?;

        Ok(())
    }

    /// List all API keys (returns metadata only, no hashes)
    pub fn list(&self) -> Result<Vec<ApiKeyMetadata>, ReasonDBError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(API_KEYS_TABLE)?;

        let mut keys = Vec::new();
        for item in table.iter()? {
            let (_, data) = item?;
            let key: ApiKey = bincode::deserialize(data.value())?;
            keys.push(ApiKeyMetadata::from(&key));
        }

        // Sort by created_at descending (newest first)
        keys.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(keys)
    }

    /// Count total API keys
    pub fn count(&self) -> Result<usize, ReasonDBError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(API_KEYS_TABLE)?;
        Ok(table.len()? as usize)
    }

    /// Check if any API keys exist (used to determine if auth is configured)
    pub fn has_keys(&self) -> Result<bool, ReasonDBError> {
        Ok(self.count()? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{KeyPrefix, Permission, Permissions};
    use tempfile::tempdir;

    fn create_test_store() -> ApiKeyStore {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let db = Arc::new(Database::create(db_path).unwrap());
        ApiKeyStore::new(db).unwrap()
    }

    #[test]
    fn test_insert_and_get() {
        let store = create_test_store();

        let perms = Permissions::new(vec![Permission::Read]);
        let (key, _raw) = ApiKey::generate("Test".into(), KeyPrefix::Test, perms);

        store.insert(&key).unwrap();

        let retrieved = store.get(&key.id).unwrap().unwrap();
        assert_eq!(retrieved.id, key.id);
        assert_eq!(retrieved.name, key.name);
    }

    #[test]
    fn test_authenticate() {
        let store = create_test_store();

        let perms = Permissions::default_user();
        let (key, raw) = ApiKey::generate("Auth Test".into(), KeyPrefix::Live, perms);

        store.insert(&key).unwrap();

        // Valid key should authenticate
        let result = store.authenticate(&raw).unwrap();
        assert!(result.is_some());

        // Invalid key should not authenticate
        let result = store
            .authenticate("rdb_live_invalidinvalidinvalidinval123")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_revoke() {
        let store = create_test_store();

        let perms = Permissions::default_user();
        let (key, raw) = ApiKey::generate("Revoke Test".into(), KeyPrefix::Test, perms);

        store.insert(&key).unwrap();

        // Key should authenticate before revoke
        assert!(store.authenticate(&raw).unwrap().is_some());

        // Revoke the key
        store.revoke(&key.id).unwrap();

        // Key should not authenticate after revoke
        assert!(store.authenticate(&raw).unwrap().is_none());
    }

    #[test]
    fn test_list() {
        let store = create_test_store();

        let perms = Permissions::default_user();
        for i in 0..3 {
            let (key, _) = ApiKey::generate(format!("Key {}", i), KeyPrefix::Test, perms.clone());
            store.insert(&key).unwrap();
        }

        let keys = store.list().unwrap();
        assert_eq!(keys.len(), 3);
    }
}
