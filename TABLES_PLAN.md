# Tables & Metadata Plan

## Overview

Add the concept of **Tables** (collections) to organize documents into logical groups with custom metadata, enabling targeted search and filtering.

## Current State

```
Documents (flat list)
├── Document 1
├── Document 2
└── Document 3
```

- No logical grouping
- Search spans all documents or requires explicit `document_id`
- Limited filtering capabilities

## Proposed State

```
Tables (collections with metadata)
│
├── Table: "Legal Contracts"
│   │   metadata: {department: "legal", confidential: true}
│   │
│   ├── Document: "NDA - Acme Corp"
│   │       metadata: {contract_type: "nda", value: 50000, signed: true}
│   │       tags: ["nda", "active", "2026"]
│   │       author: "Legal Team"
│   │
│   └── Document: "Service Agreement - Client Inc"
│           metadata: {contract_type: "msa", renewal_date: "2027-01"}
│           tags: ["msa", "active"]
│           author: "Legal Team"
│
├── Table: "Technical Docs"
│   │   metadata: {product: "payment-api", team: "engineering"}
│   │
│   ├── Document: "API Reference v2"
│   │       metadata: {api_version: "2.0", status: "published"}
│   │       tags: ["api", "payments", "reference"]
│   │       version: "2.0.0"
│   │
│   └── Document: "Integration Guide"
│           metadata: {audience: "developers", difficulty: "intermediate"}
│           tags: ["guide", "integration"]
│
└── Table: "default" (fallback)
        └── Document: "Misc Notes"
```

### Metadata Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│                         TABLE                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ id: "legal-contracts"                                 │  │
│  │ name: "Legal Contracts"                               │  │
│  │ metadata: {department: "legal", confidential: true}   │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                 │
│         ┌──────────────────┼──────────────────┐             │
│         ▼                  ▼                  ▼             │
│  ┌────────────┐     ┌────────────┐     ┌────────────┐      │
│  │ DOCUMENT 1 │     │ DOCUMENT 2 │     │ DOCUMENT 3 │      │
│  ├────────────┤     ├────────────┤     ├────────────┤      │
│  │ metadata:  │     │ metadata:  │     │ metadata:  │      │
│  │  type: nda │     │  type: msa │     │  type: sow │      │
│  │  value: 50k│     │  value: 1M │     │  value: 200k│     │
│  ├────────────┤     ├────────────┤     ├────────────┤      │
│  │ tags:      │     │ tags:      │     │ tags:      │      │
│  │  [nda,     │     │  [msa,     │     │  [sow,     │      │
│  │   active]  │     │   renewal] │     │   pending] │      │
│  ├────────────┤     ├────────────┤     ├────────────┤      │
│  │ author:    │     │ author:    │     │ author:    │      │
│  │  "Legal"   │     │  "Legal"   │     │  "Sales"   │      │
│  └────────────┘     └────────────┘     └────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

---

## Data Model

### Table Model

```rust
/// A collection of related documents with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Unique identifier
    pub id: String,
    
    /// Human-readable name
    pub name: String,
    
    /// Optional description
    pub description: Option<String>,
    
    /// Custom metadata (key-value pairs)
    /// Examples: {"department": "legal", "region": "us-west", "confidential": true}
    pub metadata: HashMap<String, serde_json::Value>,
    
    /// Number of documents in this table
    pub document_count: usize,
    
    /// Total nodes across all documents
    pub total_nodes: usize,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}
```

### Document Model Changes

```rust
pub struct Document {
    pub id: String,
    pub title: String,
    pub root_node_id: String,
    
    // NEW: Table association
    pub table_id: Option<String>,  // None = "default" table
    
    // NEW: Document-level metadata (custom key-value pairs)
    // Examples: {"author": "John", "version": "2.1", "tags": ["finance", "q4"]}
    pub metadata: HashMap<String, serde_json::Value>,
    
    // NEW: Common document attributes (typed fields for common use cases)
    pub tags: Vec<String>,           // Quick filtering by tags
    pub author: Option<String>,      // Document author
    pub source_url: Option<String>,  // Original source URL
    pub language: Option<String>,    // Document language (e.g., "en", "es")
    pub version: Option<String>,     // Document version
    
    // ... existing fields (title, created_at, etc.)
}
```

### Document Metadata Examples

```rust
// Legal document
Document {
    title: "NDA Agreement",
    table_id: Some("legal-contracts"),
    metadata: hashmap! {
        "contract_type" => json!("nda"),
        "parties" => json!(["Acme Corp", "Client Inc"]),
        "effective_date" => json!("2026-01-15"),
        "expiry_date" => json!("2027-01-15"),
        "value_usd" => json!(50000),
        "signed" => json!(true),
    },
    tags: vec!["nda", "confidential", "active"],
    author: Some("Legal Team"),
    ..
}

// Technical documentation
Document {
    title: "API Reference v2",
    table_id: Some("technical-docs"),
    metadata: hashmap! {
        "product" => json!("payment-api"),
        "api_version" => json!("2.0"),
        "status" => json!("published"),
        "last_reviewed" => json!("2026-01-20"),
    },
    tags: vec!["api", "payments", "documentation"],
    version: Some("2.0.0"),
    ..
}

// Research paper
Document {
    title: "ML in Healthcare",
    table_id: Some("research"),
    metadata: hashmap! {
        "journal" => json!("Nature Medicine"),
        "doi" => json!("10.1038/example"),
        "citations" => json!(150),
        "peer_reviewed" => json!(true),
    },
    tags: vec!["ml", "healthcare", "research"],
    author: Some("Dr. Smith et al."),
    language: Some("en"),
    ..
}
```

### Filter Model

```rust
/// Search filter criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilter {
    // === Table Filters ===
    /// Filter by table ID
    pub table_id: Option<String>,
    
    /// Filter by table name (fuzzy match)
    pub table_name: Option<String>,
    
    /// Filter by table metadata
    /// Example: {"department": "legal"}
    pub table_metadata: Option<HashMap<String, serde_json::Value>>,
    
    // === Document Filters ===
    /// Filter by document tags (any match)
    pub tags: Option<Vec<String>>,
    
    /// Filter by document tags (all must match)
    pub tags_all: Option<Vec<String>>,
    
    /// Filter by author
    pub author: Option<String>,
    
    /// Filter by document metadata
    /// Example: {"contract_type": "nda", "signed": true}
    pub document_metadata: Option<HashMap<String, serde_json::Value>>,
    
    // === Date Filters ===
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub updated_after: Option<DateTime<Utc>>,
}

/// Filter operators for advanced queries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum FilterOp {
    Eq { value: serde_json::Value },           // equals
    Ne { value: serde_json::Value },           // not equals  
    Gt { value: serde_json::Value },           // greater than
    Gte { value: serde_json::Value },          // greater than or equal
    Lt { value: serde_json::Value },           // less than
    Lte { value: serde_json::Value },          // less than or equal
    In { values: Vec<serde_json::Value> },     // in list
    Contains { value: String },                 // string contains
    StartsWith { value: String },               // string starts with
}
```

---

## Storage Changes

### New redb Table

```rust
// In store.rs
const TABLES: TableDefinition<&str, &[u8]> = TableDefinition::new("tables");

// Index: table_id -> [document_ids]
const TABLE_DOCUMENTS: TableDefinition<&str, &[u8]> = TableDefinition::new("table_documents");
```

### NodeStore Extensions

```rust
impl NodeStore {
    // Table CRUD
    pub fn create_table(&self, table: &Table) -> Result<()>;
    pub fn get_table(&self, id: &str) -> Result<Option<Table>>;
    pub fn update_table(&self, table: &Table) -> Result<()>;
    pub fn delete_table(&self, id: &str, cascade: bool) -> Result<()>;
    pub fn list_tables(&self) -> Result<Vec<Table>>;
    
    // Table-Document associations
    pub fn get_documents_in_table(&self, table_id: &str) -> Result<Vec<Document>>;
    pub fn move_document_to_table(&self, doc_id: &str, table_id: &str) -> Result<()>;
    
    // Filtered queries
    pub fn find_documents(&self, filter: &SearchFilter) -> Result<Vec<Document>>;
    pub fn find_tables(&self, metadata_filter: &HashMap<String, Value>) -> Result<Vec<Table>>;
}
```

---

## API Changes

### New Endpoints

```
Tables Management:
  POST   /v1/tables              Create a new table
  GET    /v1/tables              List all tables
  GET    /v1/tables/{id}         Get table details
  PATCH  /v1/tables/{id}         Update table metadata
  DELETE /v1/tables/{id}         Delete table (optional cascade)
  GET    /v1/tables/{id}/documents  List documents in table

Document Management:
  PATCH  /v1/documents/{id}            Update document metadata/tags/table
  POST   /v1/documents/{id}/move       Move document to different table
```

### Modified Endpoints

```
Ingestion (add table_id and metadata):
  POST /v1/ingest/text
    {
      "title": "Contract",
      "content": "...",
      "table_id": "legal-contracts",      // NEW: assign to table
      "metadata": {                        // NEW: document metadata
        "contract_type": "nda",
        "parties": ["Acme", "Client"],
        "value_usd": 50000
      },
      "tags": ["nda", "active"],          // NEW: document tags
      "author": "Legal Team"              // NEW: document author
    }

  POST /v1/ingest/file
    - Add "table_id" form field
    - Add "metadata" JSON form field
    - Add "tags" JSON array form field

Search (add filters):
  POST /v1/search
    {
      "query": "termination clause",
      "table_id": "legal-contracts",       // NEW: restrict to table
      "filters": {                         // NEW: metadata filters
        "document_metadata": {"contract_type": "nda"},
        "tags": ["active"],
        "created_after": "2024-01-01"
      },
      "limit": 10
    }

Document Updates:
  PATCH /v1/documents/{id}
    {
      "table_id": "new-table",            // Move to different table
      "metadata": {                        // Update metadata
        "status": "archived"
      },
      "tags": ["archived", "2024"]        // Update tags
    }
```

---

## API Request/Response Examples

### Create Table

```bash
curl -X POST http://localhost:4444/v1/tables \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Legal Contracts",
    "description": "All legal documents and contracts",
    "metadata": {
      "department": "legal",
      "confidential": true,
      "retention_years": 7
    }
  }'
```

Response:
```json
{
  "id": "tbl_a1b2c3d4",
  "name": "Legal Contracts",
  "description": "All legal documents and contracts",
  "metadata": {
    "department": "legal",
    "confidential": true,
    "retention_years": 7
  },
  "document_count": 0,
  "total_nodes": 0,
  "created_at": "2026-01-27T10:00:00Z"
}
```

### List Tables

```bash
curl http://localhost:4444/v1/tables
```

Response:
```json
{
  "tables": [
    {
      "id": "tbl_a1b2c3d4",
      "name": "Legal Contracts",
      "document_count": 15,
      "total_nodes": 234
    },
    {
      "id": "tbl_default",
      "name": "Default",
      "document_count": 3,
      "total_nodes": 12
    }
  ],
  "total": 2
}
```

### Ingest Document with Metadata

```bash
curl -X POST http://localhost:4444/v1/ingest/text \
  -H "Content-Type: application/json" \
  -d '{
    "title": "NDA Agreement",
    "content": "# Non-Disclosure Agreement...",
    "table_id": "tbl_a1b2c3d4",
    "metadata": {
      "contract_type": "nda",
      "parties": ["Acme Corp", "Client Inc"],
      "effective_date": "2026-01-15",
      "value_usd": 50000
    },
    "tags": ["nda", "confidential", "active"],
    "author": "Legal Team"
  }'
```

Response:
```json
{
  "document_id": "doc_xyz123",
  "title": "NDA Agreement",
  "table_id": "tbl_a1b2c3d4",
  "nodes_created": 5,
  "metadata": {
    "contract_type": "nda",
    "parties": ["Acme Corp", "Client Inc"],
    "effective_date": "2026-01-15",
    "value_usd": 50000
  },
  "tags": ["nda", "confidential", "active"]
}
```

### Update Document Metadata

```bash
curl -X PATCH http://localhost:4444/v1/documents/doc_xyz123 \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "status": "signed",
      "signed_date": "2026-01-20"
    },
    "tags": ["nda", "confidential", "signed"]
  }'
```

### Move Document to Different Table

```bash
curl -X POST http://localhost:4444/v1/documents/doc_xyz123/move \
  -H "Content-Type: application/json" \
  -d '{
    "table_id": "archived-contracts"
  }'
```

### Filtered Search Examples

#### Search by Table

```bash
curl -X POST http://localhost:4444/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "What are the termination conditions?",
    "table_id": "legal-contracts"
  }'
```

#### Search by Document Tags

```bash
curl -X POST http://localhost:4444/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "payment processing flow",
    "filters": {
      "tags": ["api", "payments"]
    }
  }'
```

#### Search by Document Metadata

```bash
curl -X POST http://localhost:4444/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "What are the termination conditions?",
    "table_id": "legal-contracts",
    "filters": {
      "document_metadata": {
        "contract_type": "nda",
        "signed": true
      },
      "author": "Legal Team"
    }
  }'
```

#### Search with Date Range

```bash
curl -X POST http://localhost:4444/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "quarterly revenue",
    "filters": {
      "tags": ["finance", "quarterly"],
      "created_after": "2025-01-01",
      "created_before": "2026-01-01"
    }
  }'
```

#### Response with Document Metadata

```json
{
  "results": [{
    "document_id": "doc_xyz",
    "document_title": "NDA Agreement - Acme Corp",
    "table_id": "legal-contracts",
    "table_name": "Legal Contracts",
    "content": "...",
    "answer": "The agreement may be terminated with 30 days written notice...",
    "confidence": 0.92,
    "document_metadata": {
      "contract_type": "nda",
      "parties": ["Acme Corp", "Client Inc"],
      "effective_date": "2026-01-15",
      "value_usd": 50000
    },
    "tags": ["nda", "confidential", "active"],
    "author": "Legal Team"
  }],
  "stats": {
    "tables_searched": 1,
    "documents_matched": 5,
    "documents_searched": 15,
    "nodes_visited": 45,
    "llm_calls": 12
  }
}
```

---

## Implementation Plan

### Phase 5A: Core Table Support (2-3 days)

1. **Model Changes** (`reasondb-core/src/model.rs`)
   - Add `Table` struct
   - Add `table_id` and enhanced `metadata` to `Document`
   - Add `SearchFilter` struct

2. **Storage Changes** (`reasondb-core/src/store.rs`)
   - Add TABLES table to redb
   - Add TABLE_DOCUMENTS index
   - Implement table CRUD operations
   - Implement filtered queries

3. **Migration** (handle existing data)
   - Create "default" table
   - Assign existing documents to default table

### Phase 5B: API Integration (1-2 days)

4. **New Routes** (`reasondb-server/src/routes/tables.rs`)
   - POST/GET/PATCH/DELETE /v1/tables
   - GET /v1/tables/{id}/documents

5. **Modified Routes**
   - Update ingest endpoints for table_id
   - Update search endpoint for filters
   - Update document endpoints for table context

6. **OpenAPI Updates**
   - Add table schemas
   - Document new endpoints

### Phase 5C: Search Enhancement (1-2 days)

7. **Filtered Search** (`reasondb-core/src/engine.rs`)
   - Filter documents by table before search
   - Filter by metadata criteria
   - Optimize to skip irrelevant tables

8. **Testing**
   - Unit tests for table CRUD
   - Integration tests for filtered search
   - Performance tests with multiple tables

---

## Migration Strategy

For existing databases:

```rust
pub fn migrate_v1_to_v2(store: &NodeStore) -> Result<()> {
    // 1. Create default table
    let default_table = Table::new("Default", "Documents without table assignment");
    store.create_table(&default_table)?;
    
    // 2. Assign all existing documents to default table
    let documents = store.list_documents()?;
    for doc in documents {
        store.move_document_to_table(&doc.id, &default_table.id)?;
    }
    
    // 3. Update schema version
    store.set_schema_version(2)?;
    
    Ok(())
}
```

---

## Indexing Strategy

### Index Types

| Index Type | Use Case | Complexity | Example |
|------------|----------|------------|---------|
| **Hash Index** | Exact match | O(1) | `table_id = "legal"` |
| **B-Tree Index** | Range queries | O(log n) | `created_at > "2025-01"` |
| **Inverted Index** | Array membership | O(1) lookup | `tags contains "nda"` |
| **Composite Index** | Multi-field queries | O(log n) | `table_id + created_at` |

### Index Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           INDEX LAYER                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  PRIMARY INDEXES (redb tables)                                          │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐      │
│  │ DOCUMENTS        │  │ TABLES           │  │ NODES            │      │
│  │ doc_id → doc     │  │ table_id → table │  │ node_id → node   │      │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘      │
│                                                                         │
│  SECONDARY INDEXES (inverted lookups)                                   │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐      │
│  │ TABLE_DOCS       │  │ TAG_DOCS         │  │ AUTHOR_DOCS      │      │
│  │ table_id →       │  │ tag →            │  │ author →         │      │
│  │   [doc_ids]      │  │   [doc_ids]      │  │   [doc_ids]      │      │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘      │
│                                                                         │
│  COMPOSITE INDEXES (multi-field)                                        │
│  ┌──────────────────────────────┐  ┌──────────────────────────────┐    │
│  │ TABLE_CREATED               │  │ METADATA_VALUE               │    │
│  │ (table_id, created_at) →    │  │ (key, value) →               │    │
│  │   doc_id                     │  │   [doc_ids]                  │    │
│  └──────────────────────────────┘  └──────────────────────────────┘    │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### redb Table Definitions

```rust
use redb::{TableDefinition, MultimapTableDefinition};

// === PRIMARY TABLES ===
// Direct key-value lookups
const DOCUMENTS: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("documents");           // doc_id -> Document

const TABLES: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("tables");              // table_id -> Table

const NODES: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("nodes");               // node_id -> PageNode

// === SECONDARY INDEXES (One-to-Many) ===
// Using MultimapTableDefinition for inverted indexes

const IDX_TABLE_DOCS: MultimapTableDefinition<&str, &str> = 
    MultimapTableDefinition::new("idx_table_docs");    // table_id -> doc_ids

const IDX_TAG_DOCS: MultimapTableDefinition<&str, &str> = 
    MultimapTableDefinition::new("idx_tag_docs");      // tag -> doc_ids

const IDX_AUTHOR_DOCS: MultimapTableDefinition<&str, &str> = 
    MultimapTableDefinition::new("idx_author_docs");   // author -> doc_ids

// === COMPOSITE INDEXES ===
// Concatenated keys for multi-field queries

const IDX_TABLE_CREATED: TableDefinition<&str, &str> = 
    TableDefinition::new("idx_table_created");   // "table_id:timestamp" -> doc_id

// === METADATA VALUE INDEX ===
// For arbitrary metadata field lookups
// Key format: "field_path:value_hash" -> doc_ids

const IDX_METADATA: MultimapTableDefinition<&str, &str> = 
    MultimapTableDefinition::new("idx_metadata");      // "contract_type:nda" -> doc_ids
```

### Index Operations

```rust
impl NodeStore {
    // === Index Maintenance ===
    
    /// Index a document on insert/update
    fn index_document(&self, doc: &Document) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        
        // 1. Table index
        if let Some(table_id) = &doc.table_id {
            let mut table_docs = write_txn.open_multimap_table(IDX_TABLE_DOCS)?;
            table_docs.insert(table_id.as_str(), doc.id.as_str())?;
        }
        
        // 2. Tags index (inverted)
        {
            let mut tag_docs = write_txn.open_multimap_table(IDX_TAG_DOCS)?;
            for tag in &doc.tags {
                tag_docs.insert(tag.as_str(), doc.id.as_str())?;
            }
        }
        
        // 3. Author index
        if let Some(author) = &doc.author {
            let mut author_docs = write_txn.open_multimap_table(IDX_AUTHOR_DOCS)?;
            author_docs.insert(author.as_str(), doc.id.as_str())?;
        }
        
        // 4. Composite index (table + created_at for range queries)
        if let Some(table_id) = &doc.table_id {
            let composite_key = format!("{}:{}", table_id, doc.created_at.timestamp());
            let mut idx = write_txn.open_table(IDX_TABLE_CREATED)?;
            idx.insert(composite_key.as_str(), doc.id.as_str())?;
        }
        
        // 5. Metadata field indexes
        {
            let mut meta_idx = write_txn.open_multimap_table(IDX_METADATA)?;
            for (key, value) in &doc.metadata {
                let index_key = format_metadata_key(key, value);
                meta_idx.insert(index_key.as_str(), doc.id.as_str())?;
            }
        }
        
        write_txn.commit()?;
        Ok(())
    }
    
    /// Remove document from all indexes
    fn unindex_document(&self, doc: &Document) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        
        // Remove from table index
        if let Some(table_id) = &doc.table_id {
            let mut table_docs = write_txn.open_multimap_table(IDX_TABLE_DOCS)?;
            table_docs.remove(table_id.as_str(), doc.id.as_str())?;
        }
        
        // Remove from tag indexes
        {
            let mut tag_docs = write_txn.open_multimap_table(IDX_TAG_DOCS)?;
            for tag in &doc.tags {
                tag_docs.remove(tag.as_str(), doc.id.as_str())?;
            }
        }
        
        // ... remove from other indexes
        
        write_txn.commit()?;
        Ok(())
    }
}

/// Format metadata key for indexing
/// Handles different value types for consistent indexing
fn format_metadata_key(field: &str, value: &serde_json::Value) -> String {
    match value {
        Value::String(s) => format!("{}:s:{}", field, s),
        Value::Number(n) => format!("{}:n:{}", field, n),
        Value::Bool(b) => format!("{}:b:{}", field, b),
        _ => format!("{}:h:{}", field, hash_value(value)),
    }
}
```

### Query Execution with Indexes

```rust
impl NodeStore {
    /// Find documents using indexes
    pub fn find_documents(&self, filter: &SearchFilter) -> Result<Vec<Document>> {
        let read_txn = self.db.begin_read()?;
        
        // Start with all doc IDs or narrow by most selective index
        let mut candidate_ids: Option<HashSet<String>> = None;
        
        // 1. Table filter (most selective, use first)
        if let Some(table_id) = &filter.table_id {
            let table_docs = read_txn.open_multimap_table(IDX_TABLE_DOCS)?;
            let ids: HashSet<String> = table_docs
                .get(table_id.as_str())?
                .map(|r| r.value().to_string())
                .collect();
            candidate_ids = Some(ids);
        }
        
        // 2. Tag filter (intersect with candidates)
        if let Some(tags) = &filter.tags {
            let tag_docs = read_txn.open_multimap_table(IDX_TAG_DOCS)?;
            
            for tag in tags {
                let tag_ids: HashSet<String> = tag_docs
                    .get(tag.as_str())?
                    .map(|r| r.value().to_string())
                    .collect();
                
                candidate_ids = match candidate_ids {
                    Some(existing) => Some(existing.intersection(&tag_ids).cloned().collect()),
                    None => Some(tag_ids),
                };
            }
        }
        
        // 3. Metadata filters
        if let Some(meta_filters) = &filter.document_metadata {
            let meta_idx = read_txn.open_multimap_table(IDX_METADATA)?;
            
            for (key, value) in meta_filters {
                let index_key = format_metadata_key(key, value);
                let meta_ids: HashSet<String> = meta_idx
                    .get(index_key.as_str())?
                    .map(|r| r.value().to_string())
                    .collect();
                
                candidate_ids = match candidate_ids {
                    Some(existing) => Some(existing.intersection(&meta_ids).cloned().collect()),
                    None => Some(meta_ids),
                };
            }
        }
        
        // 4. Fetch actual documents
        let docs_table = read_txn.open_table(DOCUMENTS)?;
        let mut results = Vec::new();
        
        let ids_to_fetch = candidate_ids.unwrap_or_else(|| {
            // No filters - return all (expensive!)
            self.list_all_document_ids(&read_txn).unwrap_or_default()
        });
        
        for id in ids_to_fetch {
            if let Some(data) = docs_table.get(id.as_str())? {
                let doc: Document = bincode::deserialize(data.value())?;
                
                // 5. Apply non-indexed filters (date ranges, etc.)
                if filter.matches_date_range(&doc) {
                    results.push(doc);
                }
            }
        }
        
        Ok(results)
    }
}
```

### Index Selection Strategy

```rust
/// Query planner chooses optimal index path
pub struct QueryPlanner;

impl QueryPlanner {
    /// Estimate selectivity and choose best index
    pub fn plan_query(filter: &SearchFilter, stats: &IndexStats) -> QueryPlan {
        let mut steps = Vec::new();
        
        // Calculate selectivity for each filter
        let selectivities = vec![
            ("table_id", filter.table_id.as_ref().map(|_| 
                stats.avg_docs_per_table as f64 / stats.total_docs as f64)),
            ("tags", filter.tags.as_ref().map(|t| 
                stats.estimate_tag_selectivity(t))),
            ("author", filter.author.as_ref().map(|_| 
                stats.avg_docs_per_author as f64 / stats.total_docs as f64)),
            ("metadata", filter.document_metadata.as_ref().map(|m|
                stats.estimate_metadata_selectivity(m))),
        ];
        
        // Sort by selectivity (most selective first)
        let mut ordered: Vec<_> = selectivities
            .into_iter()
            .filter_map(|(name, sel)| sel.map(|s| (name, s)))
            .collect();
        ordered.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Build query plan
        for (index_name, _selectivity) in ordered {
            steps.push(QueryStep::IndexLookup(index_name.to_string()));
            steps.push(QueryStep::Intersect);
        }
        
        // Add non-indexed filters at the end
        if filter.created_after.is_some() || filter.created_before.is_some() {
            steps.push(QueryStep::FilterDateRange);
        }
        
        QueryPlan { steps }
    }
}

pub struct QueryPlan {
    pub steps: Vec<QueryStep>,
}

pub enum QueryStep {
    IndexLookup(String),
    Intersect,
    FilterDateRange,
    FetchDocuments,
}
```

### Performance Benchmarks (Target)

| Operation | Without Index | With Index | Improvement |
|-----------|---------------|------------|-------------|
| Find by table_id | O(n) scan | O(1) | 1000x for 1M docs |
| Find by tag | O(n) scan | O(1) + O(k) | 500x |
| Find by metadata field | O(n) scan | O(1) + O(k) | 500x |
| Range query (date) | O(n) scan | O(log n) + O(k) | 100x |
| Multi-filter query | O(n) scan | O(1) intersect | 1000x |

*n = total documents, k = matching documents*

### Index Maintenance Costs

| Operation | Index Overhead |
|-----------|----------------|
| Insert document | ~5-10 index writes |
| Update metadata | ~2-8 index updates |
| Delete document | ~5-10 index deletes |
| Add new tag | 1 index write |

### Automatic Index Suggestions

```rust
/// Track query patterns to suggest indexes
pub struct IndexAdvisor {
    query_log: Vec<QueryPattern>,
}

impl IndexAdvisor {
    /// Analyze query patterns and suggest new indexes
    pub fn suggest_indexes(&self) -> Vec<IndexSuggestion> {
        let mut suggestions = Vec::new();
        
        // Count field usage in filters
        let mut field_counts: HashMap<String, usize> = HashMap::new();
        for query in &self.query_log {
            for field in &query.filter_fields {
                *field_counts.entry(field.clone()).or_default() += 1;
            }
        }
        
        // Suggest indexes for frequently filtered fields
        for (field, count) in field_counts {
            if count > 100 && !self.has_index(&field) {
                suggestions.push(IndexSuggestion {
                    field: field.clone(),
                    index_type: self.suggest_index_type(&field),
                    estimated_improvement: self.estimate_improvement(&field),
                });
            }
        }
        
        suggestions
    }
}
```

## Performance Considerations

1. **Index on table_id**: Fast document lookup by table (O(1))
2. **Inverted tag index**: Efficient tag-based filtering
3. **Metadata indexing**: Index frequently queried metadata fields
4. **Composite indexes**: For common multi-field queries
5. **Table-level caching**: Cache table metadata for quick access
6. **Parallel table search**: When searching multiple tables, process in parallel
7. **Query planning**: Choose optimal index based on selectivity

---

## Future Enhancements

1. **Table Permissions**: Access control per table
2. **Table Schemas**: Enforce metadata schema per table
3. **Cross-table Search**: Search across multiple tables with weights
4. **Table Analytics**: Usage stats, popular queries per table
5. **Table Versioning**: Track changes to table structure over time
6. **Nested Tables**: Hierarchical table organization

---

## File Changes Summary

```
Modified:
  crates/reasondb-core/src/model.rs      - Add Table, update Document
  crates/reasondb-core/src/store.rs      - Table storage operations
  crates/reasondb-core/src/engine.rs     - Filtered search
  crates/reasondb-core/src/lib.rs        - Export new types
  crates/reasondb-server/src/routes/     - New tables.rs, update others
  crates/reasondb-server/src/openapi.rs  - New schemas

New:
  crates/reasondb-server/src/routes/tables.rs  - Table management endpoints
```

---

## Timeline Estimate

| Phase | Task | Effort |
|-------|------|--------|
| 5A | Core Table Support | 2-3 days |
| 5B | API Integration | 1-2 days |
| 5C | Search Enhancement | 1-2 days |
| - | Testing & Polish | 1 day |
| **Total** | | **5-8 days** |
