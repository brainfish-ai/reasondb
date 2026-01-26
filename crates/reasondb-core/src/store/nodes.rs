//! Node CRUD operations
//!
//! PageNodes represent hierarchical content within documents.

use redb::ReadableTable;

use super::{NodeStore, DOC_NODES_INDEX, NODES_TABLE};
use crate::error::{ReasonError, Result, StorageError};
use crate::model::{NodeId, PageNode};

impl NodeStore {
    // ==================== Basic CRUD ====================

    /// Insert a new node into the database.
    pub fn insert_node(&self, node: &PageNode) -> Result<()> {
        let key = node.id.as_str();
        let value = bincode::serialize(node)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(NODES_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Update document-node index
            Self::update_doc_node_index_in_txn(&write_txn, &node.document_id, &node.id)?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Insert multiple nodes in a single transaction.
    ///
    /// More efficient than calling `insert_node` multiple times.
    pub fn insert_nodes(&self, nodes: &[PageNode]) -> Result<()> {
        if nodes.is_empty() {
            return Ok(());
        }

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(NODES_TABLE)
                .map_err(StorageError::from)?;

            for node in nodes {
                let key = node.id.as_str();
                let value = bincode::serialize(node)?;
                table
                    .insert(key, value.as_slice())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }

            // Update document-node indexes
            for node in nodes {
                Self::update_doc_node_index_in_txn(&write_txn, &node.document_id, &node.id)?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Get a node by its ID.
    pub fn get_node(&self, id: &str) -> Result<Option<PageNode>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(NODES_TABLE)
            .map_err(StorageError::from)?;

        match table
            .get(id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => {
                let node: PageNode = bincode::deserialize(value.value())?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    /// Get a node, returning an error if not found.
    pub fn get_node_required(&self, id: &str) -> Result<PageNode> {
        self.get_node(id)?
            .ok_or_else(|| ReasonError::NodeNotFound(id.to_string()))
    }

    /// Update an existing node.
    pub fn update_node(&self, node: &PageNode) -> Result<()> {
        let key = node.id.as_str();
        let value = bincode::serialize(node)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(NODES_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Delete a node by its ID.
    pub fn delete_node(&self, id: &str) -> Result<bool> {
        // Get the node first to update the document-node index
        let node = match self.get_node(id)? {
            Some(n) => n,
            None => return Ok(false),
        };

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        let deleted = {
            let mut table = write_txn
                .open_table(NODES_TABLE)
                .map_err(StorageError::from)?;
            let deleted = table
                .remove(id)
                .map_err(|e| StorageError::TableError(e.to_string()))?
                .is_some();

            if deleted {
                Self::remove_from_doc_node_index_in_txn(&write_txn, &node.document_id, id)?;
            }
            deleted
        };
        write_txn.commit().map_err(StorageError::from)?;
        Ok(deleted)
    }

    // ==================== Tree Traversal ====================

    /// Get the root node of a document.
    ///
    /// Looks up the document's `root_node_id` and returns that node if it exists.
    pub fn get_root_node(&self, doc_id: &str) -> Result<Option<PageNode>> {
        let doc = match self.get_document(doc_id)? {
            Some(d) => d,
            None => return Ok(None),
        };

        if doc.root_node_id.is_empty() {
            return Ok(None);
        }

        self.get_node(&doc.root_node_id)
    }

    /// Get all nodes belonging to a document.
    pub fn get_nodes_for_document(&self, doc_id: &str) -> Result<Vec<PageNode>> {
        let node_ids = self.get_node_ids_for_document(doc_id)?;
        let mut nodes = Vec::with_capacity(node_ids.len());

        for id in node_ids {
            if let Some(node) = self.get_node(&id)? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    /// Get child nodes of a given node.
    pub fn get_children(&self, parent: &PageNode) -> Result<Vec<PageNode>> {
        let mut children = Vec::with_capacity(parent.children_ids.len());
        for child_id in &parent.children_ids {
            if let Some(child) = self.get_node(child_id)? {
                children.push(child);
            }
        }
        Ok(children)
    }

    /// Get the parent node if it exists.
    pub fn get_parent(&self, node: &PageNode) -> Result<Option<PageNode>> {
        match &node.parent_id {
            Some(parent_id) => self.get_node(parent_id),
            None => Ok(None),
        }
    }

    /// Get siblings of a node (other children of the same parent).
    pub fn get_siblings(&self, node: &PageNode) -> Result<Vec<PageNode>> {
        match &node.parent_id {
            Some(parent_id) => {
                let parent = self.get_node_required(parent_id)?;
                let mut siblings = Vec::new();
                for child_id in &parent.children_ids {
                    if child_id != &node.id {
                        if let Some(sibling) = self.get_node(child_id)? {
                            siblings.push(sibling);
                        }
                    }
                }
                Ok(siblings)
            }
            None => Ok(vec![]),
        }
    }

    /// Get ancestors of a node (parent, grandparent, etc.).
    pub fn get_ancestors(&self, node: &PageNode) -> Result<Vec<PageNode>> {
        let mut ancestors = Vec::new();
        let mut current = node.clone();

        while let Some(parent_id) = &current.parent_id {
            if let Some(parent) = self.get_node(parent_id)? {
                ancestors.push(parent.clone());
                current = parent;
            } else {
                break;
            }
        }
        Ok(ancestors)
    }

    /// Get all descendants of a node (children, grandchildren, etc.).
    pub fn get_descendants(&self, node: &PageNode) -> Result<Vec<PageNode>> {
        let mut descendants = Vec::new();
        let mut stack: Vec<String> = node.children_ids.clone();

        while let Some(child_id) = stack.pop() {
            if let Some(child) = self.get_node(&child_id)? {
                stack.extend(child.children_ids.clone());
                descendants.push(child);
            }
        }
        Ok(descendants)
    }

    // ==================== Internal Index Helpers ====================

    /// Get node IDs for a document from the index.
    fn get_node_ids_for_document(&self, doc_id: &str) -> Result<Vec<NodeId>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(DOC_NODES_INDEX)
            .map_err(StorageError::from)?;

        match table
            .get(doc_id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => {
                let ids: Vec<NodeId> = serde_json::from_slice(value.value())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
                Ok(ids)
            }
            None => Ok(vec![]),
        }
    }

    /// Update document-node index within a transaction.
    pub(crate) fn update_doc_node_index_in_txn(
        write_txn: &redb::WriteTransaction,
        doc_id: &str,
        node_id: &str,
    ) -> Result<()> {
        let mut idx_table = write_txn
            .open_table(DOC_NODES_INDEX)
            .map_err(StorageError::from)?;

        // Get existing node IDs - extract data before releasing borrow
        let mut node_ids: Vec<String> = {
            let existing_option = idx_table
                .get(doc_id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            match existing_option {
                Some(value) => serde_json::from_slice(value.value())
                    .map_err(|e| StorageError::TableError(e.to_string()))?,
                None => vec![],
            }
        };

        // Add new node ID if not present
        let node_id_string = node_id.to_string();
        if !node_ids.contains(&node_id_string) {
            node_ids.push(node_id_string);

            let serialized = serde_json::to_vec(&node_ids)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
            idx_table
                .insert(doc_id, serialized.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        Ok(())
    }

    /// Remove a node from the document-node index.
    pub(crate) fn remove_from_doc_node_index_in_txn(
        write_txn: &redb::WriteTransaction,
        doc_id: &str,
        node_id: &str,
    ) -> Result<()> {
        let mut idx_table = write_txn
            .open_table(DOC_NODES_INDEX)
            .map_err(StorageError::from)?;

        // Extract data before releasing borrow
        let node_ids_opt: Option<Vec<String>> = {
            let existing_option = idx_table
                .get(doc_id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            match existing_option {
                Some(value) => {
                    let ids: Vec<String> = serde_json::from_slice(value.value())
                        .map_err(|e| StorageError::TableError(e.to_string()))?;
                    Some(ids)
                }
                None => None,
            }
        };

        if let Some(mut node_ids) = node_ids_opt {
            node_ids.retain(|id| id != node_id);

            let serialized = serde_json::to_vec(&node_ids)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
            idx_table
                .insert(doc_id, serialized.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        Ok(())
    }

    /// Delete all nodes for a document within a transaction.
    pub(crate) fn delete_document_nodes_in_txn(
        &self,
        write_txn: &redb::WriteTransaction,
        doc_id: &str,
    ) -> Result<()> {
        // Get all node IDs for this document
        let node_ids = self.get_node_ids_for_document(doc_id)?;

        // Delete each node
        {
            let mut nodes_table = write_txn
                .open_table(NODES_TABLE)
                .map_err(StorageError::from)?;
            for node_id in &node_ids {
                nodes_table
                    .remove(node_id.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }

        // Clear the document-node index
        {
            let mut idx_table = write_txn
                .open_table(DOC_NODES_INDEX)
                .map_err(StorageError::from)?;
            idx_table
                .remove(doc_id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }

        Ok(())
    }
}
