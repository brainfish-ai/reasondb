//! Database-backed configuration storage
//!
//! Stores LLM settings (and future config) in the same ReDB database
//! as documents and nodes. Uses JSON serialization for human-debuggability.

use redb::TableDefinition;

use crate::error::{Result, StorageError};
use crate::llm::config::LlmSettings;

use super::NodeStore;

/// Key-value config table (config_key -> JSON bytes)
pub(crate) const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("config");

const LLM_SETTINGS_KEY: &str = "llm_settings";

impl NodeStore {
    /// Retrieve the persisted LLM settings, or `None` if never stored.
    pub fn get_llm_settings(&self) -> Result<Option<LlmSettings>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(CONFIG_TABLE)
            .map_err(StorageError::from)?;

        match table.get(LLM_SETTINGS_KEY) {
            Ok(Some(guard)) => {
                let bytes = guard.value();
                let settings: LlmSettings = serde_json::from_slice(bytes)
                    .map_err(|e| StorageError::Deserialization(format!("LLM settings: {}", e)))?;
                Ok(Some(settings))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::TableError(e.to_string()).into()),
        }
    }

    /// Persist LLM settings to the database.
    pub fn set_llm_settings(&self, settings: &LlmSettings) -> Result<()> {
        let bytes = serde_json::to_vec(settings)
            .map_err(|e| StorageError::Serialization(format!("LLM settings: {}", e)))?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(CONFIG_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(LLM_SETTINGS_KEY, bytes.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::config::{LlmModelConfig, LlmOptions};

    #[test]
    fn test_round_trip_llm_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NodeStore::open(tmp.path().join("test.db")).unwrap();

        assert!(store.get_llm_settings().unwrap().is_none());

        let settings = LlmSettings {
            ingestion: LlmModelConfig {
                provider: "openai".into(),
                api_key: Some("sk-test".into()),
                model: Some("gpt-4o-mini".into()),
                base_url: None,
                options: LlmOptions {
                    temperature: Some(0.2),
                    disable_thinking: true,
                    ..Default::default()
                },
            },
            retrieval: LlmModelConfig {
                provider: "anthropic".into(),
                api_key: Some("sk-ant-test".into()),
                model: Some("claude-sonnet-4-5-20250929".into()),
                base_url: None,
                options: LlmOptions::default(),
            },
        };

        store.set_llm_settings(&settings).unwrap();
        let loaded = store.get_llm_settings().unwrap().unwrap();

        assert_eq!(loaded.ingestion.provider, "openai");
        assert_eq!(loaded.retrieval.provider, "anthropic");
        assert_eq!(loaded.ingestion.options.temperature, Some(0.2));
        assert!(loaded.ingestion.options.disable_thinking);
    }
}
