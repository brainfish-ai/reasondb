//! Table CRUD operations
//!
//! Tables are the primary organizational unit for documents in ReasonDB.
//! Each document MUST belong to a table.
//!
//! # Name Uniqueness
//!
//! Table names must be unique. The uniqueness is enforced via a slug index
//! that normalizes names (lowercase, underscores for special chars).

use redb::ReadableTable;

use super::{NodeStore, IDX_TABLE_SLUG, TABLES_TABLE};
use crate::error::{ReasonError, Result, StorageError};
use crate::model::Table;

impl NodeStore {
    /// Insert a new table into the database.
    ///
    /// # Arguments
    ///
    /// * `table` - The table to insert
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A table with the same ID already exists
    /// - A table with the same name (slug) already exists
    pub fn insert_table(&self, table: &Table) -> Result<()> {
        // Check if table ID already exists
        if self.get_table(&table.id)?.is_some() {
            return Err(ReasonError::Storage(StorageError::TableAlreadyExists(
                table.id.clone(),
            )));
        }

        // Check if table name (slug) already exists
        if self.get_table_by_slug(&table.slug)?.is_some() {
            return Err(ReasonError::Storage(StorageError::TableNameExists(
                table.name.clone(),
            )));
        }

        let key = table.id.as_str();
        let value = bincode::serialize(table)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            // Insert table data
            let mut t = write_txn
                .open_table(TABLES_TABLE)
                .map_err(StorageError::from)?;
            t.insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Insert slug index (slug -> table_id)
            let mut slug_idx = write_txn
                .open_table(IDX_TABLE_SLUG)
                .map_err(StorageError::from)?;
            slug_idx
                .insert(table.slug.as_str(), table.id.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Get a table by its ID.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(table))` if found, `Ok(None)` if not found.
    pub fn get_table(&self, id: &str) -> Result<Option<Table>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(TABLES_TABLE)
            .map_err(StorageError::from)?;

        match table
            .get(id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => {
                let t: Table = bincode::deserialize(value.value())?;
                Ok(Some(t))
            }
            None => Ok(None),
        }
    }

    /// Get a table by its slug (normalized name).
    ///
    /// Use this for exact slug lookups when you know the normalized name.
    ///
    /// # Arguments
    ///
    /// * `slug` - The normalized table name (use `Table::slugify(name)` to normalize)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::{NodeStore, Table};
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let store = NodeStore::open("./test.db")?;
    ///
    /// // Look up by exact slug
    /// let table = store.get_table_by_slug("legal_contracts")?;
    ///
    /// // Or normalize a display name
    /// let slug = Table::slugify("Legal Contracts");
    /// let table = store.get_table_by_slug(&slug)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_table_by_slug(&self, slug: &str) -> Result<Option<Table>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let slug_idx = read_txn
            .open_table(IDX_TABLE_SLUG)
            .map_err(StorageError::from)?;

        match slug_idx
            .get(slug)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(table_id) => {
                let id = table_id.value();
                self.get_table(id)
            }
            None => Ok(None),
        }
    }

    /// Get a table by name (normalizes to slug internally).
    ///
    /// This is a convenience method that normalizes the name before lookup.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::NodeStore;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let store = NodeStore::open("./test.db")?;
    ///
    /// // These all find the same table:
    /// let t1 = store.get_table_by_name("Legal Contracts")?;
    /// let t2 = store.get_table_by_name("legal contracts")?;
    /// let t3 = store.get_table_by_name("LEGAL_CONTRACTS")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_table_by_name(&self, name: &str) -> Result<Option<Table>> {
        let slug = Table::slugify(name);
        self.get_table_by_slug(&slug)
    }

    /// Get a table, returning an error if not found.
    pub fn get_table_required(&self, id: &str) -> Result<Table> {
        self.get_table(id)?
            .ok_or_else(|| ReasonError::TableNotFound(id.to_string()))
    }

    /// Update an existing table.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The table doesn't exist
    /// - The new name conflicts with another table
    pub fn update_table(&self, table: &Table) -> Result<()> {
        // Get existing table to check slug change
        let existing = self.get_table_required(&table.id)?;

        // If slug changed, check for conflicts
        if existing.slug != table.slug {
            if let Some(conflict) = self.get_table_by_slug(&table.slug)? {
                if conflict.id != table.id {
                    return Err(ReasonError::Storage(StorageError::TableNameExists(
                        table.name.clone(),
                    )));
                }
            }
        }

        let key = table.id.as_str();
        let value = bincode::serialize(table)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            // Update table data
            let mut t = write_txn
                .open_table(TABLES_TABLE)
                .map_err(StorageError::from)?;
            t.insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Update slug index if changed
            if existing.slug != table.slug {
                let mut slug_idx = write_txn
                    .open_table(IDX_TABLE_SLUG)
                    .map_err(StorageError::from)?;

                // Remove old slug
                slug_idx
                    .remove(existing.slug.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;

                // Insert new slug
                slug_idx
                    .insert(table.slug.as_str(), table.id.as_str())
                    .map_err(|e| StorageError::TableError(e.to_string()))?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Delete a table.
    ///
    /// # Arguments
    ///
    /// * `id` - The table ID to delete
    /// * `cascade` - If true, delete all documents in the table. If false, return error if table has documents.
    ///
    /// # Errors
    ///
    /// Returns an error if the table doesn't exist or has documents (when cascade=false).
    pub fn delete_table(&self, id: &str, cascade: bool) -> Result<bool> {
        // Get existing table to know its slug
        let existing = match self.get_table(id)? {
            Some(t) => t,
            None => return Ok(false),
        };

        // Check if table has documents
        let docs = self.get_documents_in_table(id)?;

        if !docs.is_empty() {
            if cascade {
                // Delete all documents in the table
                for doc in &docs {
                    self.delete_document(&doc.id)?;
                }
            } else {
                return Err(ReasonError::Storage(StorageError::TableNotEmpty(
                    id.to_string(),
                )));
            }
        }

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        let deleted = {
            // Remove from primary table
            let mut table = write_txn
                .open_table(TABLES_TABLE)
                .map_err(StorageError::from)?;
            let result = table
                .remove(id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Remove from slug index
            let mut slug_idx = write_txn
                .open_table(IDX_TABLE_SLUG)
                .map_err(StorageError::from)?;
            slug_idx
                .remove(existing.slug.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            result.is_some()
        };
        write_txn.commit().map_err(StorageError::from)?;
        Ok(deleted)
    }

    /// List all tables.
    pub fn list_tables(&self) -> Result<Vec<Table>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(TABLES_TABLE)
            .map_err(StorageError::from)?;

        let mut tables = Vec::new();
        for result in table
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            let (_, value) = result.map_err(|e| StorageError::TableError(e.to_string()))?;
            let t: Table = bincode::deserialize(value.value())?;
            tables.push(t);
        }
        Ok(tables)
    }
}
