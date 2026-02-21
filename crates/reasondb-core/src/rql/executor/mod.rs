//! RQL Query Executor
//!
//! Executes parsed RQL queries against the NodeStore.
//!
//! # Execution Methods
//!
//! - `execute_rql()` - Basic execution for filter-only queries
//! - `execute_rql_with_search()` - Execution with BM25 full-text search support
//! - `execute_rql_async()` - Async execution with REASON (LLM semantic search)
//!
//! # Module Structure
//!
//! - `types` - Query result types and statistics
//! - `filter` - Document filtering and condition matching
//! - `aggregate` - Aggregate function computation
//! - `plan` - Query plan building for EXPLAIN
//! - `reason` - LLM-powered semantic search execution

mod types;
mod filter;
mod aggregate;
mod plan;
mod reason;

// Re-export public types
pub use types::*;

use std::collections::HashSet;
use std::sync::Arc;

use crate::error::Result;
use crate::llm::ReasoningEngine;
use crate::model::Document;
use crate::store::NodeStore;
use crate::text_index::TextIndex;

use super::ast::*;

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
        filter.table_id = Some(table_id.clone());

        // Find documents using existing infrastructure
        let documents = self.find_documents(&filter)?;

        // Apply RELATED clause filtering
        let related_filtered = self.apply_related_filter(documents, query.related.as_ref())?;

        // Apply additional filtering from WHERE clause
        let filtered = self.apply_where_filter(related_filtered, query.where_clause.as_ref());

        // Sort if ORDER BY specified
        let mut sorted = filtered;
        if let Some(ref order_by) = query.order_by {
            filter::sort_documents(&mut sorted, order_by);
        }

        // Get total count before pagination
        let total_count = sorted.len();

        // Handle EXPLAIN
        if query.explain {
            return self.build_explain_result(query, &table_id, total_count, start);
        }

        // Handle aggregates
        if let SelectClause::Aggregates(ref aggs) = query.select {
            return self.build_aggregate_result(sorted, aggs, query, total_count, start);
        }

        // Apply pagination and build result
        self.build_select_result(sorted, query, total_count, start)
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

        // Execute search and get documents with scores
        let documents = self.get_search_documents(query, text_index, &table_id)?;

        // Apply WHERE filtering
        let filtered = self.apply_where_filter_with_scores(documents, query.where_clause.as_ref());

        // Sort (BM25 already sorted, only sort if no search)
        let sorted = self.sort_search_results(filtered, query, text_index.is_some() && query.search.is_some());

        // Get total count before pagination
        let total_count = sorted.len();

        // Build stats
        let stats = QueryStats {
            index_used: if query.search.is_some() && text_index.is_some() {
                Some("bm25_full_text".to_string())
            } else {
                Some("idx_table_docs".to_string())
            },
            rows_scanned: total_count,
            rows_returned: 0, // Updated below
            search_executed: query.search.is_some() && text_index.is_some(),
            reason_executed: query.reason.is_some(),
            llm_calls: 0,
        };

        // Handle EXPLAIN
        if query.explain {
            let plan = plan::build_query_plan(query, &table_id, self);
            return Ok(QueryResult {
                documents: Vec::new(),
                total_count: 0,
                execution_time_ms: start.elapsed().as_millis() as u64,
                stats,
                aggregates: None,
                explain: Some(plan),
            });
        }

        // Handle aggregates
        if let SelectClause::Aggregates(ref aggs) = query.select {
            let matches = self.convert_to_matches_with_scores(sorted, true);
            let aggregates = aggregate::compute_aggregates(self, &matches, aggs, query.group_by.as_ref());
            return Ok(QueryResult {
                documents: Vec::new(),
                total_count,
                execution_time_ms: start.elapsed().as_millis() as u64,
                stats,
                aggregates: Some(aggregates),
                explain: None,
            });
        }

        // Apply pagination
        let paginated = self.apply_pagination_with_scores(sorted, query.limit.as_ref());

        // Convert to DocumentMatch
        let has_search = query.search.is_some() && text_index.is_some();
        let matches = self.convert_to_matches_with_scores(paginated, has_search);

        Ok(QueryResult {
            documents: matches,
            total_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats: QueryStats {
                rows_returned: total_count.min(query.limit.as_ref().map(|l| l.count).unwrap_or(total_count)),
                ..stats
            },
            aggregates: None,
            explain: None,
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
            return reason::execute_reason_query(
                self,
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

    /// Execute an RQL query with progress reporting for REASON queries.
    ///
    /// Same as `execute_rql_async` but accepts an optional progress channel
    /// to stream REASON execution progress to the caller (e.g. SSE endpoints).
    pub async fn execute_rql_async_with_progress<R: ReasoningEngine + Send + Sync + 'static>(
        self: &Arc<Self>,
        query: &Query,
        text_index: Option<&TextIndex>,
        reasoner: Arc<R>,
        progress_tx: Option<tokio::sync::mpsc::Sender<ReasonProgress>>,
    ) -> Result<QueryResult> {
        if let Some(ref reason_clause) = query.reason {
            return reason::execute_reason_query_with_progress(
                self,
                query,
                &reason_clause.query,
                reason_clause.min_confidence,
                text_index,
                reasoner,
                progress_tx,
            ).await;
        }

        self.execute_rql_with_search(query, text_index)
    }

    // ==================== Helper Methods ====================

    /// Resolve a table name to its ID.
    pub(crate) fn resolve_table_id(&self, name: &str) -> Result<String> {
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
        Ok(name.to_string())
    }

    /// Apply WHERE clause filtering to documents.
    fn apply_where_filter(&self, documents: Vec<Document>, where_clause: Option<&WhereClause>) -> Vec<Document> {
        if let Some(wc) = where_clause {
            documents
                .into_iter()
                .filter(|doc| filter::matches_condition(self, doc, &wc.condition))
                .collect()
        } else {
            documents
        }
    }

    /// Apply RELATED clause filtering to documents.
    fn apply_related_filter(
        &self,
        documents: Vec<Document>,
        related: Option<&RelatedClause>,
    ) -> Result<Vec<Document>> {
        let Some(related_clause) = related else {
            return Ok(documents);
        };

        // Get the set of related document IDs
        let related_ids = self.get_related_document_ids(
            &related_clause.document_id,
            related_clause.relation_type.as_ref(),
        )?;

        // Filter documents to only include related ones
        Ok(documents
            .into_iter()
            .filter(|doc| related_ids.contains(&doc.id))
            .collect())
    }

    /// Apply RELATED clause filtering to documents with scores.
    fn apply_related_filter_with_scores(
        &self,
        documents: Vec<(Document, f32, Option<String>)>,
        related: Option<&RelatedClause>,
    ) -> Result<Vec<(Document, f32, Option<String>)>> {
        let Some(related_clause) = related else {
            return Ok(documents);
        };

        // Get the set of related document IDs
        let related_ids = self.get_related_document_ids(
            &related_clause.document_id,
            related_clause.relation_type.as_ref(),
        )?;

        // Filter documents to only include related ones
        Ok(documents
            .into_iter()
            .filter(|(doc, _, _)| related_ids.contains(&doc.id))
            .collect())
    }

    /// Get document IDs related to a given document.
    fn get_related_document_ids(
        &self,
        document_id: &str,
        relation_filter: Option<&RelationFilter>,
    ) -> Result<HashSet<String>> {
        use crate::model::RelationType;

        // Convert RQL RelationFilter to model RelationType
        let relation_type = relation_filter.map(|rf| match rf {
            RelationFilter::Any => None,
            RelationFilter::References => Some(RelationType::References),
            RelationFilter::ReferencedBy => Some(RelationType::ReferencedBy),
            RelationFilter::FollowsUp => Some(RelationType::FollowsUp),
            RelationFilter::FollowedUpBy => Some(RelationType::FollowedUpBy),
            RelationFilter::Supersedes => Some(RelationType::Supersedes),
            RelationFilter::SupersededBy => Some(RelationType::SupersededBy),
            RelationFilter::ParentOf => Some(RelationType::ParentOf),
            RelationFilter::ChildOf => Some(RelationType::ChildOf),
            RelationFilter::Custom(s) => Some(RelationType::Custom(s.clone())),
        }).flatten();

        // Get related document IDs using the store's relation methods
        let related_ids = self.get_related_documents(document_id, relation_type.as_ref())?;
        Ok(related_ids.into_iter().collect())
    }

    /// Apply WHERE clause filtering to documents with scores.
    fn apply_where_filter_with_scores(
        &self,
        documents: Vec<(Document, f32, Option<String>)>,
        where_clause: Option<&WhereClause>,
    ) -> Vec<(Document, f32, Option<String>)> {
        if let Some(wc) = where_clause {
            documents
                .into_iter()
                .filter(|(doc, _, _)| filter::matches_condition(self, doc, &wc.condition))
                .collect()
        } else {
            documents
        }
    }

    /// Get documents from search or filter.
    fn get_search_documents(
        &self,
        query: &Query,
        text_index: Option<&TextIndex>,
        table_id: &str,
    ) -> Result<Vec<(Document, f32, Option<String>)>> {
        if let (Some(ref search_clause), Some(index)) = (&query.search, text_index) {
            // Execute BM25 search
            let results = index.search(&search_clause.query, 1000, Some(table_id))?;
            let mut docs = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for hit in results {
                if seen.contains(&hit.document_id) {
                    continue;
                }
                seen.insert(hit.document_id.clone());
                if let Ok(Some(doc)) = self.get_document(&hit.document_id) {
                    docs.push((doc, hit.score, hit.snippet.clone()));
                }
            }

            // Apply RELATED filter
            let docs = self.apply_related_filter_with_scores(docs, query.related.as_ref())?;
            Ok(docs)
        } else {
            // Fall back to filter-based search
            let mut filter = query.to_search_filter();
            filter.table_id = Some(table_id.to_string());
            let docs = self.find_documents(&filter)?;

            // Apply RELATED filter
            let docs = self.apply_related_filter(docs, query.related.as_ref())?;
            Ok(docs.into_iter().map(|d| (d, 0.0, None)).collect())
        }
    }

    /// Sort search results.
    fn sort_search_results(
        &self,
        mut results: Vec<(Document, f32, Option<String>)>,
        query: &Query,
        has_search: bool,
    ) -> Vec<(Document, f32, Option<String>)> {
        if !has_search {
            if let Some(ref order_by) = query.order_by {
                results.sort_by(|(a, _, _), (b, _, _)| {
                    let field = order_by.field.first_field().unwrap_or("");
                    let cmp = match field {
                        "title" => a.title.cmp(&b.title),
                        "created_at" => a.created_at.cmp(&b.created_at),
                        "updated_at" => a.updated_at.cmp(&b.updated_at),
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
        // BM25 results are already sorted by relevance
        results
    }

    /// Apply pagination to results with scores.
    fn apply_pagination_with_scores(
        &self,
        results: Vec<(Document, f32, Option<String>)>,
        limit: Option<&LimitClause>,
    ) -> Vec<(Document, f32, Option<String>)> {
        if let Some(limit) = limit {
            let offset = limit.offset.unwrap_or(0);
            results.into_iter().skip(offset).take(limit.count).collect()
        } else {
            results
        }
    }

    /// Convert documents with scores to DocumentMatch.
    fn convert_to_matches_with_scores(
        &self,
        results: Vec<(Document, f32, Option<String>)>,
        has_search: bool,
    ) -> Vec<DocumentMatch> {
        results
            .into_iter()
            .map(|(doc, score, snippet)| DocumentMatch {
                document: doc,
                score: if has_search { Some(score) } else { None },
                matched_nodes: Vec::new(),
                highlights: snippet.into_iter().collect(),
                answer: None,
                confidence: None,
            })
            .collect()
    }

    /// Build result for EXPLAIN queries.
    fn build_explain_result(
        &self,
        query: &Query,
        table_id: &str,
        total_count: usize,
        start: std::time::Instant,
    ) -> Result<QueryResult> {
        let stats = QueryStats {
            index_used: Some("idx_table_docs".to_string()),
            rows_scanned: total_count,
            rows_returned: 0,
            search_executed: query.search.is_some(),
            reason_executed: query.reason.is_some(),
            llm_calls: 0,
        };
        let plan = plan::build_query_plan(query, table_id, self);
        Ok(QueryResult {
            documents: Vec::new(),
            total_count: 0,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats,
            aggregates: None,
            explain: Some(plan),
        })
    }

    /// Build result for aggregate queries.
    fn build_aggregate_result(
        &self,
        sorted: Vec<Document>,
        aggs: &[AggregateExpr],
        query: &Query,
        total_count: usize,
        start: std::time::Instant,
    ) -> Result<QueryResult> {
        let matches: Vec<DocumentMatch> = sorted
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

        let stats = QueryStats {
            index_used: Some("idx_table_docs".to_string()),
            rows_scanned: total_count,
            rows_returned: matches.len(),
            search_executed: query.search.is_some(),
            reason_executed: query.reason.is_some(),
            llm_calls: 0,
        };

        let aggregates = aggregate::compute_aggregates(self, &matches, aggs, query.group_by.as_ref());

        Ok(QueryResult {
            documents: Vec::new(),
            total_count,
            execution_time_ms: start.elapsed().as_millis() as u64,
            stats,
            aggregates: Some(aggregates),
            explain: None,
        })
    }

    /// Build result for regular SELECT queries.
    fn build_select_result(
        &self,
        sorted: Vec<Document>,
        query: &Query,
        total_count: usize,
        start: std::time::Instant,
    ) -> Result<QueryResult> {
        // Apply pagination
        let paginated: Vec<Document> = if let Some(ref limit) = query.limit {
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
            aggregates: None,
            explain: None,
        })
    }
}
