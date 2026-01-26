# ReasonDB Query Language (RQL) Plan

## Overview

ReasonDB Query Language (RQL) is a SQL-like query language designed for searching, filtering, and reasoning over documents. It combines traditional SQL filtering with full-text search and semantic (AI-powered) search capabilities.

## Goals

1. **Familiar Syntax** - SQL-like syntax that developers already know
2. **Document-Native** - Designed for hierarchical document structures
3. **Search Integration** - Native full-text and semantic search
4. **Type-Safe** - Validated at parse time with helpful errors
5. **Extensible** - Easy to add new functions and operators

---

## Query Syntax

### Basic Structure

```sql
SELECT [fields]
FROM table_name
[WHERE conditions]
[SEARCH "query" [OPTIONS]]
[ORDER BY field [ASC|DESC]]
[LIMIT n [OFFSET m]]
```

### Examples

#### 1. Simple Filtering

```sql
-- Get all documents in a table
SELECT * FROM legal_contracts

-- Filter by metadata
SELECT * FROM legal_contracts
WHERE metadata.contract_type = 'nda'
  AND metadata.value_usd > 10000

-- Filter by tags
SELECT * FROM legal_contracts
WHERE 'confidential' IN tags

-- Filter by author
SELECT * FROM hr_documents
WHERE author LIKE '%Smith%'

-- Date filtering
SELECT * FROM legal_contracts
WHERE created_at > '2025-01-01'
  AND updated_at < '2025-06-01'
```

#### 2. Full-Text Search

```sql
-- Search document content
SELECT * FROM legal_contracts
SEARCH 'indemnification clause'

-- Combined filter + search
SELECT * FROM legal_contracts
WHERE metadata.status = 'active'
SEARCH 'liability'
```

#### 3. Semantic (AI) Search

```sql
-- Semantic search with reasoning
SELECT * FROM legal_contracts
REASON 'What are the termination conditions?'

-- Semantic search with confidence threshold
SELECT * FROM legal_contracts
REASON 'penalty clauses' WITH CONFIDENCE > 0.8
```

#### 4. Advanced Queries

```sql
-- Multiple tag match (AND)
SELECT * FROM legal_contracts
WHERE tags CONTAINS ALL ('nda', 'signed', 'active')

-- Multiple tag match (OR)
SELECT * FROM legal_contracts
WHERE tags CONTAINS ANY ('draft', 'pending')

-- JSON metadata queries
SELECT * FROM legal_contracts
WHERE metadata.parties[0].name = 'Acme Corp'

-- Nested metadata
SELECT * FROM hr_documents
WHERE metadata.employee.department = 'Engineering'
```

#### 5. Aggregations

```sql
-- Count documents
SELECT COUNT(*) FROM legal_contracts
WHERE metadata.status = 'active'

-- Group by
SELECT metadata.contract_type, COUNT(*)
FROM legal_contracts
GROUP BY metadata.contract_type
```

---

## Grammar Specification

### Tokens

```
SELECT, FROM, WHERE, SEARCH, REASON, ORDER, BY, LIMIT, OFFSET
AND, OR, NOT, IN, LIKE, CONTAINS, ALL, ANY, WITH, CONFIDENCE
ASC, DESC, GROUP, COUNT, AS
=, !=, <, >, <=, >=, LIKE, IS, NULL
(, ), [, ], {, }, ',', .
STRING, NUMBER, BOOLEAN, IDENTIFIER, WILDCARD
```

### BNF Grammar

```bnf
<query> ::= <select_clause> <from_clause> [<where_clause>] [<search_clause>] [<order_clause>] [<limit_clause>]

<select_clause> ::= SELECT <select_list>
<select_list> ::= '*' | <field_list>
<field_list> ::= <field> (',' <field>)*
<field> ::= <identifier> | <aggregate_func> | <identifier> '.' <identifier>

<from_clause> ::= FROM <table_name>
<table_name> ::= <identifier>

<where_clause> ::= WHERE <condition>
<condition> ::= <comparison> | <condition> AND <condition> | <condition> OR <condition> | NOT <condition> | '(' <condition> ')'

<comparison> ::= <field_path> <operator> <value>
              | <value> IN <field_path>
              | <field_path> CONTAINS ALL <value_list>
              | <field_path> CONTAINS ANY <value_list>
              | <field_path> IS NULL
              | <field_path> IS NOT NULL
              | <field_path> LIKE <string>

<field_path> ::= <identifier> ('.' <identifier> | '[' <number> ']')*
<operator> ::= '=' | '!=' | '<' | '>' | '<=' | '>='
<value> ::= <string> | <number> | <boolean> | <null>
<value_list> ::= '(' <value> (',' <value>)* ')'

<search_clause> ::= SEARCH <string> | REASON <string> [WITH CONFIDENCE <operator> <number>]

<order_clause> ::= ORDER BY <field_path> [ASC | DESC]
<limit_clause> ::= LIMIT <number> [OFFSET <number>]

<aggregate_func> ::= COUNT '(' '*' ')' | COUNT '(' <field> ')'
```

---

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────┐
│                    RQL Query String                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    1. Lexer/Tokenizer                    │
│                  (rql/lexer.rs)                          │
│         Converts query string → Token stream             │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    2. Parser                             │
│                  (rql/parser.rs)                         │
│          Converts tokens → AST (Query struct)            │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   3. Validator                           │
│                 (rql/validator.rs)                       │
│    Validates table exists, fields valid, types match     │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                  4. Query Planner                        │
│                 (rql/planner.rs)                         │
│   Optimizes query, chooses indexes, plans execution      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                  5. Executor                             │
│                 (rql/executor.rs)                        │
│       Executes plan against NodeStore + LLM              │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   QueryResult                            │
│         Vec<Document> with matched nodes/content         │
└─────────────────────────────────────────────────────────┘
```

### Data Structures

```rust
// === AST Types ===

/// Parsed query representation
pub struct Query {
    pub select: SelectClause,
    pub from: FromClause,
    pub where_clause: Option<WhereClause>,
    pub search: Option<SearchClause>,
    pub order_by: Option<OrderByClause>,
    pub limit: Option<LimitClause>,
}

/// SELECT clause
pub enum SelectClause {
    All,                           // SELECT *
    Fields(Vec<FieldSelector>),    // SELECT field1, field2
    Count,                         // SELECT COUNT(*)
}

pub struct FieldSelector {
    pub path: FieldPath,
    pub alias: Option<String>,
}

/// Field path like metadata.contract_type or tags[0]
pub struct FieldPath {
    pub segments: Vec<PathSegment>,
}

pub enum PathSegment {
    Field(String),      // .field_name
    Index(usize),       // [0]
}

/// FROM clause  
pub struct FromClause {
    pub table: String,
}

/// WHERE clause conditions
pub enum Condition {
    Comparison(Comparison),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
}

pub struct Comparison {
    pub left: FieldPath,
    pub operator: ComparisonOp,
    pub right: Value,
}

pub enum ComparisonOp {
    Eq, Ne, Lt, Gt, Le, Ge,
    Like,
    In,
    ContainsAll,
    ContainsAny,
    IsNull,
    IsNotNull,
}

/// SEARCH/REASON clause
pub enum SearchClause {
    FullText(String),
    Semantic {
        query: String,
        min_confidence: Option<f32>,
    },
}

/// ORDER BY clause
pub struct OrderByClause {
    pub field: FieldPath,
    pub direction: SortDirection,
}

pub enum SortDirection {
    Asc,
    Desc,
}

/// LIMIT clause
pub struct LimitClause {
    pub limit: usize,
    pub offset: Option<usize>,
}

// === Execution Types ===

/// Query execution plan
pub struct QueryPlan {
    pub table_id: String,
    pub index_scans: Vec<IndexScan>,
    pub filter: Option<Condition>,
    pub search: Option<SearchPlan>,
    pub sort: Option<SortPlan>,
    pub pagination: Option<Pagination>,
}

pub enum IndexScan {
    TableIndex(String),           // Use idx_table_docs
    TagIndex(Vec<String>),        // Use idx_tag_docs
    AuthorIndex(String),          // Use idx_author_docs
    MetadataIndex(String, Value), // Use idx_metadata
    FullScan,                     // No index available
}

/// Query result
pub struct QueryResult {
    pub documents: Vec<DocumentMatch>,
    pub total_count: usize,
    pub execution_time_ms: u64,
}

pub struct DocumentMatch {
    pub document: Document,
    pub score: Option<f32>,           // Search relevance score
    pub matched_nodes: Vec<NodeId>,   // Nodes that matched
    pub highlights: Vec<String>,      // Text highlights
}
```

---

## Implementation Phases

### Phase 1: Core Parser (Week 1)
- [ ] Create `rql` module structure
- [ ] Implement lexer with all tokens
- [ ] Implement parser for basic queries
- [ ] Add comprehensive error messages
- [ ] Unit tests for parser

**Files:**
- `crates/reasondb-core/src/rql/mod.rs`
- `crates/reasondb-core/src/rql/lexer.rs`
- `crates/reasondb-core/src/rql/parser.rs`
- `crates/reasondb-core/src/rql/ast.rs`
- `crates/reasondb-core/src/rql/error.rs`

### Phase 2: Query Execution (Week 2)
- [ ] Implement validator
- [ ] Implement query planner
- [ ] Implement executor with index selection
- [ ] Integrate with existing `SearchFilter`
- [ ] Unit tests for execution

**Files:**
- `crates/reasondb-core/src/rql/validator.rs`
- `crates/reasondb-core/src/rql/planner.rs`
- `crates/reasondb-core/src/rql/executor.rs`

### Phase 3: Search Integration (Week 3)
- [ ] Implement SEARCH (full-text) clause
- [ ] Implement REASON (semantic) clause
- [ ] Add confidence scoring
- [ ] Add result highlighting
- [ ] Integration tests

**Files:**
- `crates/reasondb-core/src/rql/search.rs`
- `crates/reasondb-core/src/rql/semantic.rs`

### Phase 4: API Integration (Week 4)
- [ ] Add `/v1/query` endpoint
- [ ] Add query syntax documentation
- [ ] Add query builder (programmatic API)
- [ ] Performance optimization
- [ ] End-to-end tests

**Files:**
- `crates/reasondb-server/src/routes/query.rs`
- `crates/reasondb-core/src/rql/builder.rs`

---

## API Design

### REST Endpoint

```http
POST /v1/query
Content-Type: application/json

{
  "query": "SELECT * FROM legal_contracts WHERE metadata.status = 'active' SEARCH 'liability'",
  "parameters": {
    "timeout_ms": 5000
  }
}
```

### Response

```json
{
  "documents": [
    {
      "id": "doc_abc123",
      "title": "Service Agreement",
      "table_id": "tbl_xyz",
      "score": 0.95,
      "highlights": [
        "...shall not be <em>liable</em> for indirect damages..."
      ],
      "matched_nodes": ["node_1", "node_2"]
    }
  ],
  "total_count": 42,
  "execution_time_ms": 125,
  "query_plan": {
    "index_used": "idx_table_docs",
    "rows_scanned": 150,
    "rows_returned": 42
  }
}
```

### Programmatic API

```rust
use reasondb_core::rql::{Query, QueryBuilder};

// Parse from string
let query = Query::parse("SELECT * FROM legal WHERE status = 'active'")?;
let results = store.execute_query(&query)?;

// Builder pattern
let query = QueryBuilder::new()
    .from("legal_contracts")
    .where_eq("metadata.status", "active")
    .where_in_tags(&["nda", "signed"])
    .search("indemnification")
    .order_by("created_at", Desc)
    .limit(10)
    .build()?;

let results = store.execute_query(&query)?;
```

---

## Error Handling

### Parse Errors

```
Error: Unexpected token at line 1, column 23
  |
1 | SELECT * FROM legal WHERE status ==
  |                                   ^^ expected value, found '='
  |
help: Did you mean '=' instead of '=='?
```

### Validation Errors

```
Error: Unknown table 'legal_docs'
  |
1 | SELECT * FROM legal_docs
  |               ^^^^^^^^^^ table not found
  |
help: Did you mean 'legal_documents'?
  Available tables: legal_contracts, legal_documents, hr_files
```

### Runtime Errors

```
Error: Query timeout after 5000ms
  |
  Query: SELECT * FROM huge_table SEARCH 'complex query'
  |
help: Consider adding filters to reduce result set:
  WHERE created_at > '2025-01-01'
```

---

## Index Optimization

### Query → Index Mapping

| Query Pattern | Index Used | Scan Type |
|---------------|------------|-----------|
| `FROM table_name` | `idx_table_docs` | Range |
| `WHERE 'tag' IN tags` | `idx_tag_docs` | Lookup |
| `WHERE author = 'x'` | `idx_author_docs` | Lookup |
| `WHERE metadata.key = 'val'` | `idx_metadata` | Lookup |
| `WHERE created_at > date` | Full scan | Filter |
| `SEARCH 'text'` | Full-text index | Search |
| `REASON 'query'` | Vector index | ANN |

### Query Planning Rules

1. **Use narrowest index first** - If filtering by tag AND table, use tag index (usually fewer results)
2. **Combine index scans** - Intersect results from multiple indexes
3. **Push filters down** - Apply filters during index scan when possible
4. **Defer expensive operations** - Run SEARCH/REASON after filtering

---

## Future Enhancements

### Phase 5: Advanced Features
- [ ] JOINs across tables
- [ ] Subqueries
- [ ] Aggregations (GROUP BY, HAVING)
- [ ] Window functions
- [ ] CTEs (WITH clauses)

### Phase 6: Performance
- [ ] Query caching
- [ ] Prepared statements
- [ ] Parallel query execution
- [ ] Query profiling/EXPLAIN

### Phase 7: Full-Text Search
- [ ] Stemming and tokenization
- [ ] Fuzzy matching
- [ ] Phrase queries
- [ ] Field boosting

---

## Success Metrics

1. **Parse time** < 1ms for typical queries
2. **Simple queries** < 10ms execution
3. **Complex queries** < 100ms execution
4. **Search queries** < 500ms execution
5. **Semantic queries** < 2s execution
6. **Error messages** rated helpful by 90% of users

---

## Appendix: Full Example Queries

```sql
-- Find all active NDAs worth over $50k, search for penalty clauses
SELECT * FROM legal_contracts
WHERE metadata.contract_type = 'nda'
  AND metadata.status = 'active'
  AND metadata.value_usd > 50000
SEARCH 'penalty clause termination'
ORDER BY metadata.value_usd DESC
LIMIT 10

-- Find recent HR documents by specific author
SELECT title, author, created_at FROM hr_documents
WHERE author LIKE '%Johnson%'
  AND created_at > '2025-01-01'
ORDER BY created_at DESC

-- Semantic search for specific legal concept
SELECT * FROM legal_contracts
REASON 'What happens if the vendor fails to deliver on time?'
WITH CONFIDENCE > 0.7
LIMIT 5

-- Count documents by type
SELECT metadata.contract_type, COUNT(*) as count
FROM legal_contracts
WHERE metadata.status = 'active'
GROUP BY metadata.contract_type
ORDER BY count DESC

-- Find documents with multiple specific tags
SELECT * FROM legal_contracts
WHERE tags CONTAINS ALL ('nda', 'signed', 'reviewed')
  AND 'confidential' IN tags
```
