//! ReasonDB Query Language (RQL)
//!
//! A SQL-like query language for searching, filtering, and reasoning over documents.
//!
//! # Features
//!
//! - SQL-like syntax familiar to developers
//! - Document-native filtering (tables, tags, metadata)
//! - Full-text search with `SEARCH` clause
//! - Semantic AI search with `REASON` clause
//!
//! # Example
//!
//! ```rust
//! use reasondb_core::rql::Query;
//!
//! // Parse a query
//! let query = Query::parse("SELECT * FROM legal_contracts WHERE metadata.status = 'active'").unwrap();
//!
//! // Or use the builder
//! use reasondb_core::rql::QueryBuilder;
//! let query = QueryBuilder::new()
//!     .from("legal_contracts")
//!     .where_eq("metadata.status", "active")
//!     .build()
//!     .unwrap();
//! ```

mod ast;
mod error;
mod lexer;
mod parser;
mod executor;

#[cfg(test)]
mod tests;

// Re-export public types
pub use ast::*;
pub use error::{RqlError, RqlResult};
pub use executor::{
    AggregateResult, AggregateValue, DocumentMatch, PlanStep, QueryPlan, QueryResult, QueryStats,
    ReasonPhase, ReasonPhaseStatus, ReasonProgress,
};

use crate::model::SearchFilter;

impl Query {
    /// Parse an RQL query string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use reasondb_core::rql::Query;
    ///
    /// let query = Query::parse("SELECT * FROM legal_contracts").unwrap();
    /// assert_eq!(query.from.table, "legal_contracts");
    /// ```
    pub fn parse(input: &str) -> RqlResult<Self> {
        let tokens = lexer::Lexer::new(input).tokenize()?;
        parser::Parser::new(tokens).parse()
    }

    /// Convert query to a SearchFilter for execution.
    ///
    /// Note: Pagination (LIMIT/OFFSET) is NOT applied to the SearchFilter.
    /// It's handled by the executor after sorting to get correct total counts.
    pub fn to_search_filter(&self) -> SearchFilter {
        let mut filter = SearchFilter::new();

        // Set table
        filter.table_id = Some(self.from.table.clone());

        // Apply WHERE conditions (only equality filters)
        if let Some(ref where_clause) = self.where_clause {
            self.apply_conditions_to_filter(&where_clause.condition, &mut filter);
        }

        // Note: Don't apply limit/offset here - executor handles it after sorting
        filter
    }

    fn apply_conditions_to_filter(&self, condition: &Condition, filter: &mut SearchFilter) {
        match condition {
            Condition::Comparison(comp) => {
                self.apply_comparison_to_filter(comp, filter);
            }
            Condition::And(left, right) => {
                self.apply_conditions_to_filter(left, filter);
                self.apply_conditions_to_filter(right, filter);
            }
            Condition::Or(_, _) => {
                // OR conditions need special handling - for now, skip
            }
            Condition::Not(_) => {
                // NOT conditions need special handling - for now, skip
            }
        }
    }

    fn apply_comparison_to_filter(&self, comp: &Comparison, filter: &mut SearchFilter) {
        // Only apply exact match filters to SearchFilter
        // Non-equality comparisons are handled by the executor
        if comp.operator != ComparisonOp::Eq {
            return;
        }

        let field = comp.left.to_string();

        match field.as_str() {
            f if f.starts_with("metadata.") => {
                let key = f.strip_prefix("metadata.").unwrap();
                // Only add top-level metadata fields to the filter
                // Nested paths (e.g., "employee.department") are handled by apply_where_filter
                if !key.contains('.') && !key.contains('[') {
                    if let Some(meta) = filter.document_metadata.as_mut() {
                        meta.insert(key.to_string(), comp.right.to_json());
                    } else {
                        let mut meta = std::collections::HashMap::new();
                        meta.insert(key.to_string(), comp.right.to_json());
                        filter.document_metadata = Some(meta);
                    }
                }
                // Nested metadata paths will be filtered by apply_where_filter using get_field_value
            }
            _ => {}
        }
    }
}

/// Builder for constructing queries programmatically.
///
/// # Example
///
/// ```rust
/// use reasondb_core::rql::QueryBuilder;
///
/// let query = QueryBuilder::new()
///     .from("legal_contracts")
///     .where_eq("metadata.status", "active")
///     .where_in_tags(&["nda", "signed"])
///     .limit(10)
///     .build()
///     .unwrap();
/// ```
#[derive(Default)]
pub struct QueryBuilder {
    select: Option<SelectClause>,
    from: Option<String>,
    conditions: Vec<Condition>,
    search: Option<SearchClause>,
    reason: Option<ReasonClause>,
    related: Option<RelatedClause>,
    order_by: Option<OrderByClause>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl QueryBuilder {
    /// Create a new query builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the table to query.
    pub fn from(mut self, table: &str) -> Self {
        self.from = Some(table.to_string());
        self
    }

    /// Select all fields.
    pub fn select_all(mut self) -> Self {
        self.select = Some(SelectClause::All);
        self
    }

    /// Add an equality condition.
    pub fn where_eq(mut self, field: &str, value: &str) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse(field),
            operator: ComparisonOp::Eq,
            right: Value::String(value.to_string()),
        }));
        self
    }

    /// Add a numeric equality condition.
    pub fn where_eq_num(mut self, field: &str, value: f64) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse(field),
            operator: ComparisonOp::Eq,
            right: Value::Number(value),
        }));
        self
    }

    /// Add a greater-than condition.
    pub fn where_gt(mut self, field: &str, value: f64) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse(field),
            operator: ComparisonOp::Gt,
            right: Value::Number(value),
        }));
        self
    }

    /// Add a less-than condition.
    pub fn where_lt(mut self, field: &str, value: f64) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse(field),
            operator: ComparisonOp::Lt,
            right: Value::Number(value),
        }));
        self
    }

    /// Add a LIKE condition.
    pub fn where_like(mut self, field: &str, pattern: &str) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse(field),
            operator: ComparisonOp::Like,
            right: Value::String(pattern.to_string()),
        }));
        self
    }

    /// Add a tag filter (any match).
    pub fn where_in_tags(mut self, tags: &[&str]) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse("tags"),
            operator: ComparisonOp::ContainsAny,
            right: Value::Array(tags.iter().map(|s| Value::String(s.to_string())).collect()),
        }));
        self
    }

    /// Add a tag filter (all must match).
    pub fn where_all_tags(mut self, tags: &[&str]) -> Self {
        self.conditions.push(Condition::Comparison(Comparison {
            left: FieldPath::parse("tags"),
            operator: ComparisonOp::ContainsAll,
            right: Value::Array(tags.iter().map(|s| Value::String(s.to_string())).collect()),
        }));
        self
    }

    /// Add a full-text search clause (BM25).
    pub fn search(mut self, query: &str) -> Self {
        self.search = Some(SearchClause { query: query.to_string() });
        self
    }

    /// Add a semantic search clause (LLM reasoning).
    pub fn reason(mut self, query: &str) -> Self {
        self.reason = Some(ReasonClause {
            query: query.to_string(),
            min_confidence: None,
        });
        self
    }

    /// Add a semantic search clause with confidence threshold.
    pub fn reason_with_confidence(mut self, query: &str, min_confidence: f32) -> Self {
        self.reason = Some(ReasonClause {
            query: query.to_string(),
            min_confidence: Some(min_confidence),
        });
        self
    }

    /// Filter by documents related to a specific document.
    pub fn related_to(mut self, document_id: &str) -> Self {
        self.related = Some(RelatedClause {
            document_id: document_id.to_string(),
            relation_type: None,
        });
        self
    }

    /// Filter by documents with a specific relationship to another document.
    pub fn related_with_type(mut self, document_id: &str, relation_type: RelationFilter) -> Self {
        self.related = Some(RelatedClause {
            document_id: document_id.to_string(),
            relation_type: Some(relation_type),
        });
        self
    }

    /// Set ordering.
    pub fn order_by(mut self, field: &str, direction: SortDirection) -> Self {
        self.order_by = Some(OrderByClause {
            field: FieldPath::parse(field),
            direction,
        });
        self
    }

    /// Set result limit.
    pub fn limit(mut self, count: usize) -> Self {
        self.limit = Some(count);
        self
    }

    /// Set result offset.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Build the query.
    pub fn build(self) -> RqlResult<Query> {
        let from = self.from.ok_or_else(|| {
            RqlError::Validation("FROM clause is required".to_string())
        })?;

        // Combine conditions with AND
        let where_clause = if self.conditions.is_empty() {
            None
        } else {
            let mut iter = self.conditions.into_iter();
            let first = iter.next().unwrap();
            let combined = iter.fold(first, |acc, cond| {
                Condition::And(Box::new(acc), Box::new(cond))
            });
            Some(WhereClause { condition: combined })
        };

        let limit_clause = self.limit.map(|count| LimitClause {
            count,
            offset: self.offset,
        });

        Ok(Query {
            explain: false,
            select: self.select.unwrap_or(SelectClause::All),
            from: FromClause { table: from },
            where_clause,
            search: self.search,
            reason: self.reason,
            related: self.related,
            group_by: None,
            order_by: self.order_by,
            limit: limit_clause,
        })
    }
}
