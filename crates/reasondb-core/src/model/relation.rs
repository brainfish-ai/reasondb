//! Document relationships
//!
//! Links between documents such as references, citations, and follow-ups.
//!
//! # Example
//!
//! ```rust
//! use reasondb_core::model::DocumentRelation;
//!
//! // Create a reference relationship
//! let relation = DocumentRelation::new(
//!     "doc_contract_v1",
//!     "doc_contract_v2",
//!     RelationType::Supersedes,
//! );
//!
//! // With metadata
//! let relation = DocumentRelation::builder()
//!     .from("doc_contract")
//!     .to("doc_amendment")
//!     .relation_type(RelationType::References)
//!     .note("Section 5 amendment")
//!     .build();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of relationship between documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// Document A references Document B (e.g., citation, link)
    References,
    /// Document A is referenced by Document B (inverse of References)
    ReferencedBy,
    /// Document A is a follow-up to Document B
    FollowsUp,
    /// Document A is followed up by Document B (inverse of FollowsUp)
    FollowedUpBy,
    /// Document A supersedes/replaces Document B
    Supersedes,
    /// Document A is superseded by Document B (inverse of Supersedes)
    SupersededBy,
    /// Document A is related to Document B (general relationship)
    RelatedTo,
    /// Document A is a parent of Document B (hierarchical)
    ParentOf,
    /// Document A is a child of Document B (inverse of ParentOf)
    ChildOf,
    /// Custom relationship type
    Custom(String),
}

impl RelationType {
    /// Get the inverse relationship type.
    pub fn inverse(&self) -> Self {
        match self {
            RelationType::References => RelationType::ReferencedBy,
            RelationType::ReferencedBy => RelationType::References,
            RelationType::FollowsUp => RelationType::FollowedUpBy,
            RelationType::FollowedUpBy => RelationType::FollowsUp,
            RelationType::Supersedes => RelationType::SupersededBy,
            RelationType::SupersededBy => RelationType::Supersedes,
            RelationType::RelatedTo => RelationType::RelatedTo, // Symmetric
            RelationType::ParentOf => RelationType::ChildOf,
            RelationType::ChildOf => RelationType::ParentOf,
            RelationType::Custom(s) => RelationType::Custom(format!("inverse_{}", s)),
        }
    }

    /// Check if this relationship type is symmetric.
    pub fn is_symmetric(&self) -> bool {
        matches!(self, RelationType::RelatedTo)
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationType::References => write!(f, "references"),
            RelationType::ReferencedBy => write!(f, "referenced_by"),
            RelationType::FollowsUp => write!(f, "follows_up"),
            RelationType::FollowedUpBy => write!(f, "followed_up_by"),
            RelationType::Supersedes => write!(f, "supersedes"),
            RelationType::SupersededBy => write!(f, "superseded_by"),
            RelationType::RelatedTo => write!(f, "related_to"),
            RelationType::ParentOf => write!(f, "parent_of"),
            RelationType::ChildOf => write!(f, "child_of"),
            RelationType::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// A relationship between two documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRelation {
    /// Unique identifier for this relationship
    pub id: String,
    /// Source document ID
    pub from_document_id: String,
    /// Target document ID
    pub to_document_id: String,
    /// Type of relationship
    pub relation_type: RelationType,
    /// Optional note or description
    pub note: Option<String>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    /// When the relationship was created
    pub created_at: DateTime<Utc>,
}

impl DocumentRelation {
    /// Create a new document relationship.
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        relation_type: RelationType,
    ) -> Self {
        Self {
            id: generate_relation_id(),
            from_document_id: from.into(),
            to_document_id: to.into(),
            relation_type,
            note: None,
            metadata: std::collections::HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a builder for constructing a relationship.
    pub fn builder() -> RelationBuilder {
        RelationBuilder::default()
    }

    /// Create the inverse relationship.
    pub fn inverse(&self) -> Self {
        Self {
            id: generate_relation_id(),
            from_document_id: self.to_document_id.clone(),
            to_document_id: self.from_document_id.clone(),
            relation_type: self.relation_type.inverse(),
            note: self.note.clone(),
            metadata: self.metadata.clone(),
            created_at: Utc::now(),
        }
    }
}

/// Builder for creating document relationships.
#[derive(Default)]
pub struct RelationBuilder {
    from: Option<String>,
    to: Option<String>,
    relation_type: Option<RelationType>,
    note: Option<String>,
    metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl RelationBuilder {
    /// Set the source document ID.
    pub fn from(mut self, doc_id: impl Into<String>) -> Self {
        self.from = Some(doc_id.into());
        self
    }

    /// Set the target document ID.
    pub fn to(mut self, doc_id: impl Into<String>) -> Self {
        self.to = Some(doc_id.into());
        self
    }

    /// Set the relationship type.
    pub fn relation_type(mut self, rt: RelationType) -> Self {
        self.relation_type = Some(rt);
        self
    }

    /// Set a note/description.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Add metadata.
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Build the relationship.
    pub fn build(self) -> DocumentRelation {
        DocumentRelation {
            id: generate_relation_id(),
            from_document_id: self.from.expect("from document ID required"),
            to_document_id: self.to.expect("to document ID required"),
            relation_type: self.relation_type.unwrap_or(RelationType::RelatedTo),
            note: self.note,
            metadata: self.metadata,
            created_at: Utc::now(),
        }
    }
}

/// Generate a unique relation ID.
fn generate_relation_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("rel_{:016x}", timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_relation() {
        let rel = DocumentRelation::new("doc_a", "doc_b", RelationType::References);
        assert!(rel.id.starts_with("rel_"));
        assert_eq!(rel.from_document_id, "doc_a");
        assert_eq!(rel.to_document_id, "doc_b");
        assert_eq!(rel.relation_type, RelationType::References);
    }

    #[test]
    fn test_relation_builder() {
        let rel = DocumentRelation::builder()
            .from("doc_contract")
            .to("doc_amendment")
            .relation_type(RelationType::References)
            .note("Section 5 amendment")
            .build();

        assert_eq!(rel.from_document_id, "doc_contract");
        assert_eq!(rel.to_document_id, "doc_amendment");
        assert_eq!(rel.note, Some("Section 5 amendment".to_string()));
    }

    #[test]
    fn test_inverse_relation() {
        let rel = DocumentRelation::new("doc_a", "doc_b", RelationType::References);
        let inv = rel.inverse();

        assert_eq!(inv.from_document_id, "doc_b");
        assert_eq!(inv.to_document_id, "doc_a");
        assert_eq!(inv.relation_type, RelationType::ReferencedBy);
    }

    #[test]
    fn test_relation_type_inverse() {
        assert_eq!(
            RelationType::References.inverse(),
            RelationType::ReferencedBy
        );
        assert_eq!(
            RelationType::Supersedes.inverse(),
            RelationType::SupersededBy
        );
        assert_eq!(RelationType::RelatedTo.inverse(), RelationType::RelatedTo);
    }

    #[test]
    fn test_symmetric_relation() {
        assert!(RelationType::RelatedTo.is_symmetric());
        assert!(!RelationType::References.is_symmetric());
    }
}
