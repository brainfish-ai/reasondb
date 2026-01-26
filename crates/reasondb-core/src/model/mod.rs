//! Data models for ReasonDB
//!
//! This module defines the core data structures:
//!
//! - [`Table`] - A collection of related documents with metadata
//! - [`Document`] - Root-level document metadata  
//! - [`PageNode`] - The fundamental unit of the reasoning tree
//! - [`NodeMetadata`] - Additional node attributes
//! - [`SearchFilter`] - Criteria for filtering documents during search
//!
//! # Module Structure
//!
//! - `table` - Table definition and operations
//! - `document` - Document definition and operations
//! - `node` - PageNode and NodeMetadata definitions
//! - `filter` - SearchFilter for queries

mod document;
mod filter;
mod node;
mod table;

#[cfg(test)]
mod tests;

// Re-export all public types
pub use document::Document;
pub use filter::SearchFilter;
pub use node::{NodeMetadata, PageNode};
pub use table::Table;

use serde::{Deserialize, Deserializer, Serializer};
use serde_json::Value;
use std::collections::HashMap;

// ==================== Type Aliases ====================

/// Unique identifier for nodes
pub type NodeId = String;

/// Unique identifier for documents
pub type DocumentId = String;

/// Unique identifier for tables
pub type TableId = String;

// ==================== Serialization Helpers ====================

/// Custom serialization for HashMap<String, Value> that works with bincode.
/// Bincode doesn't support serde_json::Value directly, so we serialize as JSON string.
pub(crate) mod json_metadata {
    use super::*;

    pub fn serialize<S>(map: &HashMap<String, Value>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let json_str = serde_json::to_string(map).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&json_str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, Value>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let json_str = String::deserialize(deserializer)?;
        if json_str.is_empty() {
            return Ok(HashMap::new());
        }
        serde_json::from_str(&json_str).map_err(serde::de::Error::custom)
    }
}

/// Custom serialization for Option<HashMap<String, Value>> that works with bincode.
pub(crate) mod json_metadata_option {
    use super::*;

    pub fn serialize<S>(
        map: &Option<HashMap<String, Value>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match map {
            Some(m) => {
                let json_str = serde_json::to_string(m).map_err(serde::ser::Error::custom)?;
                serializer.serialize_some(&json_str)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<HashMap<String, Value>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(json_str) if !json_str.is_empty() => {
                let map = serde_json::from_str(&json_str).map_err(serde::de::Error::custom)?;
                Ok(Some(map))
            }
            _ => Ok(None),
        }
    }
}
