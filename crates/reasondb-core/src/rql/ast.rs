//! Abstract Syntax Tree types for RQL
//!
//! This module defines the data structures that represent parsed RQL queries.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A parsed RQL statement (SELECT, UPDATE, or DELETE).
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// SELECT query
    Select(Query),
    /// UPDATE query
    Update(UpdateQuery),
    /// DELETE query
    Delete(DeleteQuery),
}

/// An UPDATE query: `UPDATE table SET field = value, ... WHERE condition`
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateQuery {
    /// Target table
    pub table: FromClause,
    /// Field assignments (SET clause)
    pub assignments: Vec<SetAssignment>,
    /// Optional WHERE clause to filter which documents to update
    pub where_clause: Option<WhereClause>,
}

/// A single SET assignment: `field = value`
#[derive(Debug, Clone, PartialEq)]
pub struct SetAssignment {
    /// Field path to update
    pub field: FieldPath,
    /// New value
    pub value: Value,
}

/// A DELETE query: `DELETE FROM table WHERE condition`
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteQuery {
    /// Target table
    pub table: FromClause,
    /// Optional WHERE clause to filter which documents to delete
    pub where_clause: Option<WhereClause>,
}

/// A complete parsed RQL query.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Whether this is an EXPLAIN query
    pub explain: bool,

    /// SELECT clause (what to return)
    pub select: SelectClause,

    /// FROM clause (which table)
    pub from: FromClause,

    /// Optional WHERE clause (filters)
    pub where_clause: Option<WhereClause>,

    /// Optional SEARCH clause (BM25 full-text search)
    pub search: Option<SearchClause>,

    /// Optional REASON clause (LLM semantic search) - can be combined with SEARCH
    pub reason: Option<ReasonClause>,

    /// Optional RELATED clause (filter by document relationships)
    pub related: Option<RelatedClause>,

    /// Optional GROUP BY clause
    pub group_by: Option<GroupByClause>,

    /// Optional ORDER BY clause
    pub order_by: Option<OrderByClause>,

    /// Optional LIMIT clause
    pub limit: Option<LimitClause>,
}

// ==================== GROUP BY ====================

/// GROUP BY clause for aggregations.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupByClause {
    /// Fields to group by
    pub fields: Vec<FieldPath>,
}

// ==================== SELECT ====================

/// SELECT clause - what fields to return.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectClause {
    /// SELECT * - return all fields
    All,

    /// SELECT field1, field2 - return specific fields
    Fields(Vec<FieldSelector>),

    /// SELECT with aggregate functions (COUNT, SUM, AVG, etc.)
    Aggregates(Vec<AggregateExpr>),
}

/// A single field selector with optional alias.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSelector {
    /// Field path (e.g., metadata.status)
    pub path: FieldPath,

    /// Optional alias (e.g., AS status)
    pub alias: Option<String>,
}

/// Aggregate function expression
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateExpr {
    /// The aggregate function
    pub function: AggregateFunction,
    /// Optional alias (e.g., AS total_count)
    pub alias: Option<String>,
}

/// Supported aggregate functions
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateFunction {
    /// COUNT(*) or COUNT(field)
    Count(Option<FieldPath>),
    /// SUM(field)
    Sum(FieldPath),
    /// AVG(field)
    Avg(FieldPath),
    /// MIN(field)
    Min(FieldPath),
    /// MAX(field)
    Max(FieldPath),
}

// ==================== FROM ====================

/// FROM clause - which table to query.
#[derive(Debug, Clone, PartialEq)]
pub struct FromClause {
    /// Table name (or slug)
    pub table: String,
}

// ==================== WHERE ====================

/// WHERE clause - filter conditions.
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// The condition tree
    pub condition: Condition,
}

/// A condition in the WHERE clause.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// A single comparison (field op value)
    Comparison(Comparison),

    /// AND of two conditions
    And(Box<Condition>, Box<Condition>),

    /// OR of two conditions
    Or(Box<Condition>, Box<Condition>),

    /// NOT of a condition
    Not(Box<Condition>),
}

/// A comparison expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Left side (field path)
    pub left: FieldPath,

    /// Operator
    pub operator: ComparisonOp,

    /// Right side (value)
    pub right: Value,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    /// Equal (=)
    Eq,
    /// Not equal (!=)
    Ne,
    /// Less than (<)
    Lt,
    /// Greater than (>)
    Gt,
    /// Less than or equal (<=)
    Le,
    /// Greater than or equal (>=)
    Ge,
    /// Pattern match (LIKE)
    Like,
    /// Value in field (IN)
    In,
    /// Field contains all values (CONTAINS ALL)
    ContainsAll,
    /// Field contains any value (CONTAINS ANY)
    ContainsAny,
    /// Field is null (IS NULL)
    IsNull,
    /// Field is not null (IS NOT NULL)
    IsNotNull,
}

impl fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Ne => write!(f, "!="),
            Self::Lt => write!(f, "<"),
            Self::Gt => write!(f, ">"),
            Self::Le => write!(f, "<="),
            Self::Ge => write!(f, ">="),
            Self::Like => write!(f, "LIKE"),
            Self::In => write!(f, "IN"),
            Self::ContainsAll => write!(f, "CONTAINS ALL"),
            Self::ContainsAny => write!(f, "CONTAINS ANY"),
            Self::IsNull => write!(f, "IS NULL"),
            Self::IsNotNull => write!(f, "IS NOT NULL"),
        }
    }
}

// ==================== Field Path ====================

/// A field path like `metadata.contract_type` or `tags[0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPath {
    /// Path segments
    pub segments: Vec<PathSegment>,
}

impl FieldPath {
    /// Parse a field path from a string.
    pub fn parse(s: &str) -> Self {
        let mut segments = Vec::new();
        let mut current = String::new();

        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                '.' => {
                    if !current.is_empty() {
                        segments.push(PathSegment::Field(current.clone()));
                        current.clear();
                    }
                }
                '[' => {
                    if !current.is_empty() {
                        segments.push(PathSegment::Field(current.clone()));
                        current.clear();
                    }
                    // Parse index
                    i += 1;
                    let mut idx_str = String::new();
                    while i < chars.len() && chars[i] != ']' {
                        idx_str.push(chars[i]);
                        i += 1;
                    }
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        segments.push(PathSegment::Index(idx));
                    }
                }
                ']' => {
                    // Skip closing bracket
                }
                c => {
                    current.push(c);
                }
            }
            i += 1;
        }

        if !current.is_empty() {
            segments.push(PathSegment::Field(current));
        }

        Self { segments }
    }

    /// Check if this is a simple field (single segment, no array access).
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1 && matches!(self.segments[0], PathSegment::Field(_))
    }

    /// Get the first segment as a field name.
    pub fn first_field(&self) -> Option<&str> {
        match self.segments.first() {
            Some(PathSegment::Field(s)) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for seg in &self.segments {
            match seg {
                PathSegment::Field(name) => {
                    if !first {
                        write!(f, ".")?;
                    }
                    write!(f, "{}", name)?;
                }
                PathSegment::Index(idx) => {
                    write!(f, "[{}]", idx)?;
                }
            }
            first = false;
        }
        Ok(())
    }
}

/// A segment in a field path.
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// Field name (e.g., `metadata`)
    Field(String),
    /// Array index (e.g., `[0]`)
    Index(usize),
}

// ==================== Values ====================

/// A literal value in an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Numeric value
    Number(f64),
    /// String value
    String(String),
    /// Array of values
    Array(Vec<Value>),
}

impl Value {
    /// Convert to serde_json::Value.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Number(n) => serde_json::json!(*n),
            Self::String(s) => serde_json::Value::String(s.clone()),
            Self::Array(arr) => serde_json::Value::Array(arr.iter().map(|v| v.to_json()).collect()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "NULL"),
            Self::Bool(b) => write!(f, "{}", b),
            Self::Number(n) => write!(f, "{}", n),
            Self::String(s) => write!(f, "'{}'", s),
            Self::Array(arr) => {
                write!(f, "(")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, ")")
            }
        }
    }
}

// ==================== SEARCH ====================

/// SEARCH clause - BM25 full-text search.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchClause {
    /// The search query string
    pub query: String,
}

// ==================== REASON ====================

/// REASON clause - LLM semantic search with answer extraction.
#[derive(Debug, Clone, PartialEq)]
pub struct ReasonClause {
    /// The natural language query
    pub query: String,
    /// Minimum confidence threshold (0.0 - 1.0)
    pub min_confidence: Option<f32>,
}

// ==================== RELATED ====================

/// RELATED clause - filter by document relationships.
///
/// # Examples
///
/// ```sql
/// -- Get all documents related to a specific document
/// SELECT * FROM contracts RELATED TO 'doc_123'
///
/// -- Get documents with a specific relationship type
/// SELECT * FROM contracts RELATED TO 'doc_123' AS references
///
/// -- Get documents referenced by a document
/// SELECT * FROM contracts REFERENCES 'doc_123'
///
/// -- Get documents that supersede a document
/// SELECT * FROM contracts SUPERSEDES 'doc_123'
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedClause {
    /// The document ID to find relationships for
    pub document_id: String,
    /// Optional filter by relationship type
    pub relation_type: Option<RelationFilter>,
}

/// Filter for relationship types in RELATED clause.
#[derive(Debug, Clone, PartialEq)]
pub enum RelationFilter {
    /// Any relationship type
    Any,
    /// Only REFERENCES relationships
    References,
    /// Only REFERENCED_BY relationships
    ReferencedBy,
    /// Only FOLLOWS_UP relationships
    FollowsUp,
    /// Only FOLLOWED_UP_BY relationships
    FollowedUpBy,
    /// Only SUPERSEDES relationships
    Supersedes,
    /// Only SUPERSEDED_BY relationships
    SupersededBy,
    /// Only PARENT_OF relationships
    ParentOf,
    /// Only CHILD_OF relationships
    ChildOf,
    /// Custom relationship type
    Custom(String),
}

// ==================== ORDER BY ====================

/// ORDER BY clause.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderByClause {
    /// Field to sort by
    pub field: FieldPath,
    /// Sort direction
    pub direction: SortDirection,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    /// Ascending order
    #[default]
    Asc,
    /// Descending order
    Desc,
}

// ==================== LIMIT ====================

/// LIMIT clause.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitClause {
    /// Maximum number of results
    pub count: usize,
    /// Offset (skip first N results)
    pub offset: Option<usize>,
}
