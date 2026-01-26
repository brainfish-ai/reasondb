//! RQL Query Executor
//!
//! Executes parsed RQL queries against the NodeStore.
//!
//! # Execution Methods
//!
//! - `execute_rql()` - Basic execution for filter-only queries
//! - `execute_rql_with_search()` - Execution with BM25 full-text search support
//! - `execute_rql_async()` - Async execution with REASON (LLM semantic search)

use std::collections::HashSet;
use std::sync::Arc;

use crate::engine::{SearchConfig, SearchEngine};
use crate::error::Result;
use crate::llm::ReasoningEngine;
use crate::model::{Document, NodeId};
use crate::store::NodeStore;
use crate::text_index::TextIndex;

use super::ast::*;

/// Result of executing a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Matched documents
    pub documents: Vec<DocumentMatch>,
    /// Total count (before pagination)
    pub total_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Query execution statistics
    pub stats: QueryStats,
}

/// Query execution statistics for analysis and optimization.
#[derive(Debug, Clone, Default)]
pub struct QueryStats {
    /// Index used for initial filtering
    pub index_used: Option<String>,
    /// Total rows scanned
    pub rows_scanned: usize,
    /// Rows returned after filtering
    pub rows_returned: usize,
    /// Whether SEARCH clause was executed
    pub search_executed: bool,
    /// Whether REASON clause was executed
    pub reason_executed: bool,
    /// Number of LLM calls made (for REASON)
    pub llm_calls: usize,
}

/// A document match with relevance info.
#[derive(Debug, Clone)]
pub struct DocumentMatch {
    /// The matched document
    pub document: Document,
    /// Relevance score (for search queries)
    pub score: Option<f32>,
    /// Nodes that matched the query
    pub matched_nodes: Vec<NodeId>,
    /// Highlighted text snippets
    pub highlights: Vec<String>,
    /// LLM-extracted answer (for REASON queries)
    pub answer: Option<String>,
    /// Confidence score from LLM (for REASON queries)
    pub confidence: Option<f32>,
}

impl NodeStore {
    /// Execute an RQL query.
    ///
    /// The table name in the FROM clause can be:
    /// - Table ID (e.g., "tbl_abc123")
    /// - Table slug (e.g., "legal_contracts")
    /// - Table display name (e.g., "Legal Contracts")
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::{NodeStore, rql::Query};
    ///
    /// let store = NodeStore::open("./test.db").unwrap();
    /// let query = Query::parse("SELECT * FROM legal_contracts WHERE author = 'Alice'").unwrap();
    /// let result = store.execute_rql(&query).unwrap();
    /// ```
    pub fn execute_rql(&self, query: &Query) -> Result<QueryResult> {
        let start = std::time::Instant::now();

        // Resolve table name to ID
        let table_id = self.resolve_table_id(&query.from.table)?;

        // Convert query to search filter with resolved table ID
        let mut filter = query.to_search_filter();
        filter.table_id = Some(table_id);

        // Find documents using existing infrastructure
        let documents = self.find_documents(&filter)?;

        // Apply additional filtering from WHERE clause
        let filtered = if let Some(ref where_clause) = query.where_clause {
            documents
                .into_iter()
                .filter(|doc| self.matches_condition(doc, &where_clause.condition))
                .collect()
        } else {
            documents
        };

        // Sort if ORDER BY specified
        let mut sorted = filtered;
        if let Some(ref order_by) = query.order_by {
            self.sort_documents(&mut sorted, order_by);
        }

        // Get total count before pagination
        let total_count = sorted.len();

        // Apply pagination
        let paginated = if let Some(ref limit) = query.limit {
            let offset = limit.offset.unwrap_or(0);
            sorted.into_iter().skip(offset).take(limit.count).collect()
        } else {
            sorted
        };

        // Convert to DocumentMatch
        let matches: Vec<DocumentMatch> = paginated
            .into_iter()
            .map(|doc| DocumentMatch {
                document: doc,
                score: None,
                matched_nodes: Vec::new(),
                highlights: Vec::new(),
                answer: None,
                confidence: None,
            })
            .collect();

        // Build stats
        let stats = QueryStats {
            index_used: Some("idx_table_docs".to_string()),
            rows_scanned: total_count,
            rows_returned: matches.len(),
            search_executed: query.search.is_some(),
            reason_executed: query.reason.is_some(),
            llm_calls: 0,
        };

        Ok(QueryResult {
            documents: matches,
            total_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats,
        })
    }

    /// Execute an RQL query with full-text search support.
    ///
    /// This method supports the SEARCH clause using BM25 ranking.
    ///
    /// # Arguments
    ///
    /// * `query` - The parsed RQL query
    /// * `text_index` - Optional TextIndex for BM25 search
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::{NodeStore, TextIndex, rql::Query};
    ///
    /// let store = NodeStore::open("./test.db").unwrap();
    /// let text_index = TextIndex::open("./search_index").unwrap();
    /// let query = Query::parse("SELECT * FROM legal_contracts SEARCH 'payment terms'").unwrap();
    /// let result = store.execute_rql_with_search(&query, Some(&text_index)).unwrap();
    /// ```
    pub fn execute_rql_with_search(
        &self,
        query: &Query,
        text_index: Option<&TextIndex>,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();

        // Resolve table name to ID
        let table_id = self.resolve_table_id(&query.from.table)?;

        // Check if we have a SEARCH clause and a text index
        let search_results = if let (Some(ref search_clause), Some(index)) =
            (&query.search, text_index)
        {
            // Execute BM25 search
            let results = index.search(&search_clause.query, 1000, Some(&table_id))?;
            Some(results)
        } else {
            None
        };

        // Get documents either from search results or filter
        let documents = if let Some(ref search_hits) = search_results {
            // Get documents from search results, preserving BM25 order
            let mut docs = Vec::new();
            let seen: HashSet<String> = HashSet::new();
            for hit in search_hits {
                if seen.contains(&hit.document_id) {
                    continue;
                }
                if let Ok(Some(doc)) = self.get_document(&hit.document_id) {
                    docs.push((doc, hit.score, hit.snippet.clone()));
                }
            }
            docs
        } else {
            // Fall back to filter-based search
            let mut filter = query.to_search_filter();
            filter.table_id = Some(table_id.clone());
            let docs = self.find_documents(&filter)?;
            docs.into_iter().map(|d| (d, 0.0, None)).collect()
        };

        // Apply additional WHERE filtering
        let filtered: Vec<(Document, f32, Option<String>)> = if let Some(ref where_clause) = query.where_clause {
            documents
                .into_iter()
                .filter(|(doc, _, _)| self.matches_condition(doc, &where_clause.condition))
                .collect()
        } else {
            documents
        };

        // Sort - use BM25 score if search, otherwise by field
        let mut sorted = filtered;
        if search_results.is_none() {
            if let Some(ref order_by) = query.order_by {
                sorted.sort_by(|(a, _, _), (b, _, _)| {
                    let field = order_by.field.first_field().unwrap_or("");
                    let cmp = match field {
                        "title" => a.title.cmp(&b.title),
                        "created_at" => a.created_at.cmp(&b.created_at),
                        "updated_at" => a.updated_at.cmp(&b.updated_at),
                        "author" => a.author.cmp(&b.author),
                        _ => std::cmp::Ordering::Equal,
                    };
                    if order_by.direction == SortDirection::Desc {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
        }
        // BM25 results are already sorted by relevance (desc)

        // Get total count before pagination
        let total_count = sorted.len();

        // Apply pagination
        let paginated: Vec<(Document, f32, Option<String>)> = if let Some(ref limit) = query.limit {
            let offset = limit.offset.unwrap_or(0);
            sorted.into_iter().skip(offset).take(limit.count).collect()
        } else {
            sorted
        };

        // Convert to DocumentMatch with scores and highlights
        let matches: Vec<DocumentMatch> = paginated
            .into_iter()
            .map(|(doc, score, snippet)| DocumentMatch {
                document: doc,
                score: if search_results.is_some() {
                    Some(score)
                } else {
                    None
                },
                matched_nodes: Vec::new(),
                highlights: snippet.into_iter().collect(),
                answer: None,
                confidence: None,
            })
            .collect();

        // Build stats
        let stats = QueryStats {
            index_used: if search_results.is_some() {
                Some("bm25_full_text".to_string())
            } else {
                Some("idx_table_docs".to_string())
            },
            rows_scanned: total_count,
            rows_returned: matches.len(),
            search_executed: search_results.is_some(),
            reason_executed: query.reason.is_some(),
            llm_calls: 0,
        };

        Ok(QueryResult {
            documents: matches,
            total_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats,
        })
    }

    /// Execute an RQL query with full async support (SEARCH + REASON).
    ///
    /// This method supports:
    /// - SEARCH clause: BM25 full-text search
    /// - REASON clause: LLM-powered semantic search with answer extraction
    ///
    /// # Arguments
    ///
    /// * `query` - The parsed RQL query
    /// * `text_index` - Optional TextIndex for BM25 search
    /// * `reasoner` - The reasoning engine for REASON queries
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::{NodeStore, TextIndex, rql::Query};
    /// use reasondb_core::llm::MockReasoner;
    /// use std::sync::Arc;
    ///
    /// async fn example() {
    ///     let store = Arc::new(NodeStore::open("./test.db").unwrap());
    ///     let text_index = TextIndex::open("./search_index").unwrap();
    ///     let reasoner = Arc::new(MockReasoner::new());
    ///     let query = Query::parse("SELECT * FROM legal REASON 'What are the penalties?'").unwrap();
    ///     let result = store.execute_rql_async(&query, Some(&text_index), reasoner).await.unwrap();
    /// }
    /// ```
    pub async fn execute_rql_async<R: ReasoningEngine + Send + Sync + 'static>(
        self: &Arc<Self>,
        query: &Query,
        text_index: Option<&TextIndex>,
        reasoner: Arc<R>,
    ) -> Result<QueryResult> {
        // Check if this is a REASON query
        if let Some(ref reason_clause) = query.reason {
            return self.execute_reason_query(
                query,
                &reason_clause.query,
                reason_clause.min_confidence,
                text_index,
                reasoner,
            ).await;
        }

        // For non-REASON queries, delegate to execute_rql_with_search
        self.execute_rql_with_search(query, text_index)
    }

    /// Execute a REASON (semantic search) query using the LLM.
    ///
    /// Supports hybrid search: if SEARCH clause is present, use BM25 to pre-filter
    /// documents before applying LLM reasoning.
    async fn execute_reason_query<R: ReasoningEngine + Send + Sync + 'static>(
        self: &Arc<Self>,
        query: &Query,
        reason_query: &str,
        min_confidence: Option<f32>,
        text_index: Option<&TextIndex>,
        reasoner: Arc<R>,
    ) -> Result<QueryResult> {
        let start = std::time::Instant::now();

        // Resolve table name to ID
        let table_id = self.resolve_table_id(&query.from.table)?;

        // Build search config
        let config = SearchConfig {
            min_confidence: min_confidence.unwrap_or(0.3),
            max_results: query.limit.as_ref().map(|l| l.count).unwrap_or(10),
            ..Default::default()
        };

        // Create search engine
        let engine = SearchEngine::with_config(self.clone(), reasoner, config);

        // Get documents to search - supports hybrid search
        let documents: Vec<Document> = if let (Some(ref search_clause), Some(index)) = (&query.search, text_index) {
            // HYBRID SEARCH: Use BM25 to pre-filter, then apply LLM reasoning
            let search_results = index.search(&search_clause.query, 100, Some(&table_id))?;
            let mut seen: HashSet<String> = HashSet::new();
            let mut docs = Vec::new();
            for hit in search_results {
                if seen.contains(&hit.document_id) {
                    continue;
                }
                if let Ok(Some(doc)) = self.get_document(&hit.document_id) {
                    docs.push(doc);
                    seen.insert(hit.document_id);
                }
            }
            docs
        } else {
            // Filter-based search only
            let mut filter = query.to_search_filter();
            filter.table_id = Some(table_id.clone());
            self.find_documents(&filter)?
        };

        // Execute semantic search on each document
        let mut all_matches: Vec<DocumentMatch> = Vec::new();
        let mut total_llm_calls = 0;

        for doc in &documents {
            let search_result = engine.search_document(reason_query, &doc.id).await;

            if let Ok(response) = search_result {
                total_llm_calls += response.stats.llm_calls;

                // Convert search results to DocumentMatch
                for result in response.results {
                    // Apply min_confidence filter
                    if let Some(min_conf) = min_confidence {
                        if result.confidence < min_conf {
                            continue;
                        }
                    }

                    all_matches.push(DocumentMatch {
                        document: doc.clone(),
                        score: Some(result.confidence),
                        matched_nodes: vec![result.node_id.clone()],
                        highlights: vec![result.content.clone()],
                        answer: result.extracted_answer,
                        confidence: Some(result.confidence),
                    });
                }
            }
        }

        // Sort by confidence (highest first)
        all_matches.sort_by(|a, b| {
            b.confidence
                .unwrap_or(0.0)
                .partial_cmp(&a.confidence.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply pagination
        let total_count = all_matches.len();
        let paginated: Vec<DocumentMatch> = if let Some(ref limit) = query.limit {
            let offset = limit.offset.unwrap_or(0);
            all_matches.into_iter().skip(offset).take(limit.count).collect()
        } else {
            all_matches
        };

        // Build stats
        let stats = QueryStats {
            index_used: Some("llm_semantic".to_string()),
            rows_scanned: documents.len(),
            rows_returned: paginated.len(),
            search_executed: false,
            reason_executed: true,
            llm_calls: total_llm_calls,
        };

        Ok(QueryResult {
            documents: paginated,
            total_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats,
        })
    }

    /// Check if a document matches a condition.
    fn matches_condition(&self, doc: &Document, condition: &Condition) -> bool {
        match condition {
            Condition::Comparison(comp) => self.matches_comparison(doc, comp),
            Condition::And(left, right) => {
                self.matches_condition(doc, left) && self.matches_condition(doc, right)
            }
            Condition::Or(left, right) => {
                self.matches_condition(doc, left) || self.matches_condition(doc, right)
            }
            Condition::Not(inner) => !self.matches_condition(doc, inner),
        }
    }

    /// Check if a document matches a comparison.
    fn matches_comparison(&self, doc: &Document, comp: &Comparison) -> bool {
        let field_value = self.get_field_value(doc, &comp.left);

        match comp.operator {
            ComparisonOp::Eq => field_value == Some(comp.right.clone()),
            ComparisonOp::Ne => field_value != Some(comp.right.clone()),
            ComparisonOp::Lt => self.compare_values(&field_value, &comp.right, |a, b| a < b),
            ComparisonOp::Gt => self.compare_values(&field_value, &comp.right, |a, b| a > b),
            ComparisonOp::Le => self.compare_values(&field_value, &comp.right, |a, b| a <= b),
            ComparisonOp::Ge => self.compare_values(&field_value, &comp.right, |a, b| a >= b),
            ComparisonOp::Like => self.matches_like(&field_value, &comp.right),
            ComparisonOp::In => self.matches_in(&field_value, &comp.right),
            ComparisonOp::ContainsAll => self.matches_contains_all(doc, &comp.left, &comp.right),
            ComparisonOp::ContainsAny => self.matches_contains_any(doc, &comp.left, &comp.right),
            ComparisonOp::IsNull => field_value.is_none(),
            ComparisonOp::IsNotNull => field_value.is_some(),
        }
    }

    /// Get a field value from a document.
    fn get_field_value(&self, doc: &Document, path: &FieldPath) -> Option<Value> {
        if path.segments.is_empty() {
            return None;
        }

        let first = match &path.segments[0] {
            PathSegment::Field(name) => name.as_str(),
            _ => return None,
        };

        // Handle top-level document fields
        match first {
            "id" => Some(Value::String(doc.id.clone())),
            "title" => Some(Value::String(doc.title.clone())),
            "table_id" => Some(Value::String(doc.table_id.clone())),
            "author" => doc.author.as_ref().map(|a| Value::String(a.clone())),
            "source_url" => doc.source_url.as_ref().map(|u| Value::String(u.clone())),
            "language" => doc.language.as_ref().map(|l| Value::String(l.clone())),
            "version" => doc.version.as_ref().map(|v| Value::String(v.clone())),
            "tags" => Some(Value::Array(
                doc.tags.iter().map(|t| Value::String(t.clone())).collect(),
            )),
            "metadata" => {
                // Handle metadata.field_name
                if path.segments.len() > 1 {
                    if let PathSegment::Field(key) = &path.segments[1] {
                        return doc.metadata.get(key).map(|v| json_to_value(v));
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Compare two values with a comparator.
    fn compare_values<F>(&self, left: &Option<Value>, right: &Value, cmp: F) -> bool
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left, right) {
            (Some(Value::Number(a)), Value::Number(b)) => cmp(*a, *b),
            _ => false,
        }
    }

    /// Check if a value matches a LIKE pattern.
    fn matches_like(&self, value: &Option<Value>, pattern: &Value) -> bool {
        match (value, pattern) {
            (Some(Value::String(v)), Value::String(p)) => {
                // Simple LIKE implementation: % = any chars
                let regex_pattern = format!(
                    "^{}$",
                    regex::escape(p).replace(r"\%", ".*").replace(r"\_", ".")
                );
                regex::Regex::new(&regex_pattern)
                    .map(|re| re.is_match(v))
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Check if a value is in a list.
    fn matches_in(&self, value: &Option<Value>, list: &Value) -> bool {
        match (value, list) {
            (Some(v), Value::Array(arr)) => arr.contains(v),
            _ => false,
        }
    }

    /// Check if document field contains all specified values.
    fn matches_contains_all(&self, doc: &Document, path: &FieldPath, values: &Value) -> bool {
        let field_name = path.first_field().unwrap_or("");
        match (field_name, values) {
            ("tags", Value::Array(required)) => {
                required.iter().all(|v| match v {
                    Value::String(tag) => doc.tags.contains(tag),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    /// Check if document field contains any of the specified values.
    fn matches_contains_any(&self, doc: &Document, path: &FieldPath, values: &Value) -> bool {
        let field_name = path.first_field().unwrap_or("");
        match (field_name, values) {
            ("tags", Value::Array(candidates)) => {
                candidates.iter().any(|v| match v {
                    Value::String(tag) => doc.tags.contains(tag),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    /// Sort documents by a field.
    fn sort_documents(&self, docs: &mut [Document], order_by: &OrderByClause) {
        let field = order_by.field.first_field().unwrap_or("");
        let desc = order_by.direction == SortDirection::Desc;

        docs.sort_by(|a, b| {
            let cmp = match field {
                "title" => a.title.cmp(&b.title),
                "created_at" => a.created_at.cmp(&b.created_at),
                "updated_at" => a.updated_at.cmp(&b.updated_at),
                "author" => a.author.cmp(&b.author),
                _ => std::cmp::Ordering::Equal,
            };
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        });
    }

    /// Resolve a table name to its ID.
    ///
    /// Accepts:
    /// - Table ID (e.g., "tbl_abc123") - returns as-is
    /// - Table slug (e.g., "legal_contracts") - looks up by slug
    /// - Table display name (e.g., "Legal Contracts") - converts to slug and looks up
    fn resolve_table_id(&self, name: &str) -> Result<String> {
        // If it looks like a table ID, return as-is
        if name.starts_with("tbl_") {
            return Ok(name.to_string());
        }

        // Try to look up by slug first
        if let Some(table) = self.get_table_by_slug(name)? {
            return Ok(table.id);
        }

        // Try to look up by name (will be converted to slug)
        if let Some(table) = self.get_table_by_name(name)? {
            return Ok(table.id);
        }

        // Table not found - return the name as-is (will result in empty results)
        // This matches SQL behavior where querying a non-existent table returns empty
        Ok(name.to_string())
    }
}

/// Convert serde_json::Value to RQL Value.
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(_) => Value::Null, // Objects not supported as values
    }
}
