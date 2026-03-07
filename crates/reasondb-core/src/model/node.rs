//! PageNode - The fundamental unit of the reasoning tree
//!
//! This module defines the tree structure used for hierarchical document representation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::{DocumentId, NodeId};

/// The fundamental unit of the reasoning tree.
///
/// A `PageNode` represents a section of a document at any level of the hierarchy.
/// Leaf nodes contain actual content, while internal nodes contain summaries
/// of their children.
///
/// # Tree Structure
///
/// ```text
/// Document Root (depth=0)
/// ├── Chapter 1 (depth=1)
/// │   ├── Section 1.1 (depth=2, leaf with content)
/// │   └── Section 1.2 (depth=2, leaf with content)
/// └── Chapter 2 (depth=1)
///     └── Section 2.1 (depth=2, leaf with content)
/// ```
///
/// # Example
///
/// ```rust
/// use reasondb_core::PageNode;
///
/// let node = PageNode::new(
///     "doc_123".to_string(),
///     "Introduction".to_string(),
///     Some("This chapter introduces the main concepts...".to_string()),
///     0,
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageNode {
    /// Unique identifier for this node
    pub id: NodeId,

    /// Reference to the parent document
    pub document_id: DocumentId,

    /// Human-readable title (e.g., "Chapter 1", "Section 2.1")
    pub title: String,

    /// LLM-generated summary describing what this node contains.
    /// This is what the LLM reads during tree traversal.
    pub summary: String,

    /// Depth level in the tree (0 = root)
    pub depth: u8,

    /// Character offset where this section starts in the source document
    pub start_index: usize,

    /// Character offset where this section ends in the source document
    pub end_index: usize,

    /// Parent node ID (None for root nodes)
    pub parent_id: Option<NodeId>,

    /// IDs of child nodes
    pub children_ids: Vec<NodeId>,

    /// Actual content (only present for leaf nodes)
    pub content: Option<String>,

    /// Path to associated image (for vision-enabled reasoning)
    pub image_path: Option<String>,

    /// Additional metadata
    pub metadata: NodeMetadata,

    /// When this node was created
    pub created_at: DateTime<Utc>,

    /// When this node was last updated
    pub updated_at: DateTime<Utc>,
}

impl PageNode {
    /// Create a new PageNode with a generated ID.
    pub fn new(document_id: DocumentId, title: String, summary: Option<String>, depth: u8) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            document_id,
            title,
            summary: summary.unwrap_or_default(),
            depth,
            start_index: 0,
            end_index: 0,
            parent_id: None,
            children_ids: Vec::new(),
            content: None,
            image_path: None,
            metadata: NodeMetadata::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new root node for a document.
    pub fn new_root(document_id: DocumentId, title: String) -> Self {
        Self::new(document_id, title, None, 0)
    }

    /// Create a new leaf node with content.
    pub fn new_leaf(
        document_id: DocumentId,
        title: String,
        content: String,
        summary: String,
        depth: u8,
    ) -> Self {
        let mut node = Self::new(document_id, title, Some(summary), depth);
        node.content = Some(content);
        node
    }

    // ==================== Query Methods ====================

    /// Check if this is a leaf node (has content, no children).
    pub fn is_leaf(&self) -> bool {
        self.children_ids.is_empty()
    }

    /// Check if this is the root node.
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Get the content or a placeholder if not a leaf.
    pub fn get_content(&self) -> &str {
        self.content
            .as_deref()
            .unwrap_or("[No content - internal node]")
    }

    // ==================== Mutation Methods ====================

    /// Add a child node ID.
    pub fn add_child(&mut self, child_id: NodeId) {
        self.children_ids.push(child_id);
        self.updated_at = Utc::now();
    }

    /// Set the parent node ID.
    pub fn set_parent(&mut self, parent_id: NodeId) {
        self.parent_id = Some(parent_id);
        self.updated_at = Utc::now();
    }

    /// Set the content and mark as leaf.
    pub fn set_content(&mut self, content: String) {
        self.content = Some(content);
        self.updated_at = Utc::now();
    }

    /// Set the summary.
    pub fn set_summary(&mut self, summary: String) {
        self.summary = summary;
        self.updated_at = Utc::now();
    }

    // ==================== LLM Context ====================

    /// Generate a compact representation for LLM context during traversal.
    pub fn to_llm_context(&self) -> String {
        format!(
            "ID: {}\nTitle: {}\nSummary: {}",
            self.id, self.title, self.summary
        )
    }
}

// ==================== NodeMetadata ====================

/// Additional metadata for a node.
///
/// Stores optional attributes like page numbers, section types,
/// confidence scores, and custom key-value pairs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NodeMetadata {
    /// Page number in the source document (if applicable)
    pub page_number: Option<u32>,

    /// Type of section (e.g., "chapter", "section", "paragraph")
    pub section_type: Option<String>,

    /// Confidence score from summarization (0.0 - 1.0)
    pub confidence_score: Option<f32>,

    /// Approximate token count of the content
    pub token_count: Option<u32>,

    /// Custom key-value attributes
    pub attributes: HashMap<String, String>,

    /// IDs of sibling nodes that this node references inline
    /// (e.g., "see Section 3.2" detected during ingestion).
    #[serde(default)]
    pub cross_ref_node_ids: Vec<String>,
}

impl NodeMetadata {
    /// Create metadata with a section type.
    pub fn with_section_type(section_type: &str) -> Self {
        Self {
            section_type: Some(section_type.to_string()),
            ..Default::default()
        }
    }

    /// Set the page number.
    pub fn with_page(mut self, page: u32) -> Self {
        self.page_number = Some(page);
        self
    }

    /// Add a custom attribute.
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }
}
