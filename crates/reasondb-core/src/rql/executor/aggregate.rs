//! Aggregate function computation
//!
//! This module handles COUNT, SUM, AVG, MIN, MAX and GROUP BY.

use std::collections::HashMap;

use crate::rql::ast::*;
use crate::store::NodeStore;

use super::filter::{get_field_value, value_to_json};
use super::types::{AggregateResult, AggregateValue, DocumentMatch};

/// Compute aggregate results.
pub fn compute_aggregates(
    store: &NodeStore,
    matches: &[DocumentMatch],
    aggs: &[AggregateExpr],
    group_by: Option<&GroupByClause>,
) -> Vec<AggregateResult> {
    if let Some(group_by) = group_by {
        compute_grouped_aggregates(store, matches, aggs, group_by)
    } else {
        compute_simple_aggregates(store, matches, aggs)
    }
}

/// Compute aggregates without GROUP BY.
fn compute_simple_aggregates(
    store: &NodeStore,
    matches: &[DocumentMatch],
    aggs: &[AggregateExpr],
) -> Vec<AggregateResult> {
    let doc_refs: Vec<&DocumentMatch> = matches.iter().collect();
    aggs.iter()
        .map(|agg| compute_single_aggregate(store, agg, &doc_refs))
        .collect()
}

/// Compute aggregates with GROUP BY.
fn compute_grouped_aggregates(
    store: &NodeStore,
    matches: &[DocumentMatch],
    aggs: &[AggregateExpr],
    group_by: &GroupByClause,
) -> Vec<AggregateResult> {
    // Group documents by the GROUP BY fields
    let mut groups: HashMap<Vec<(String, serde_json::Value)>, Vec<&DocumentMatch>> = HashMap::new();

    for m in matches {
        let key: Vec<(String, serde_json::Value)> = group_by
            .fields
            .iter()
            .filter_map(|f| {
                let field_name = f.first_field()?;
                let value = get_field_value(store, &m.document, f)?;
                Some((field_name.to_string(), value_to_json(&value)))
            })
            .collect();
        groups.entry(key).or_default().push(m);
    }

    // Compute aggregates for each group
    let mut results = Vec::new();
    for (group_key, group_docs) in groups {
        for agg in aggs {
            let result = compute_single_aggregate(store, agg, &group_docs);
            results.push(AggregateResult {
                name: result.name,
                value: result.value,
                group_key: Some(group_key.clone()),
            });
        }
    }
    results
}

/// Compute a single aggregate function.
fn compute_single_aggregate(
    store: &NodeStore,
    agg: &AggregateExpr,
    docs: &[&DocumentMatch],
) -> AggregateResult {
    let name = get_aggregate_name(agg);
    let value = match &agg.function {
        AggregateFunction::Count(field) => compute_count(store, docs, field.as_ref()),
        AggregateFunction::Sum(field) => compute_sum(store, docs, field),
        AggregateFunction::Avg(field) => compute_avg(store, docs, field),
        AggregateFunction::Min(field) => compute_min(store, docs, field),
        AggregateFunction::Max(field) => compute_max(store, docs, field),
    };

    AggregateResult {
        name,
        value,
        group_key: None,
    }
}

/// Get the display name for an aggregate.
fn get_aggregate_name(agg: &AggregateExpr) -> String {
    agg.alias.clone().unwrap_or_else(|| match &agg.function {
        AggregateFunction::Count(_) => "count".to_string(),
        AggregateFunction::Sum(f) => format!("sum_{}", f.first_field().unwrap_or("?")),
        AggregateFunction::Avg(f) => format!("avg_{}", f.first_field().unwrap_or("?")),
        AggregateFunction::Min(f) => format!("min_{}", f.first_field().unwrap_or("?")),
        AggregateFunction::Max(f) => format!("max_{}", f.first_field().unwrap_or("?")),
    })
}

// ==================== Individual Aggregate Functions ====================

/// COUNT(*) or COUNT(field)
fn compute_count(
    store: &NodeStore,
    docs: &[&DocumentMatch],
    field: Option<&FieldPath>,
) -> AggregateValue {
    if let Some(f) = field {
        // COUNT(field) - count non-null values
        let count = docs
            .iter()
            .filter(|m| get_field_value(store, &m.document, f).is_some())
            .count();
        AggregateValue::Count(count)
    } else {
        // COUNT(*) - count all rows
        AggregateValue::Count(docs.len())
    }
}

/// SUM(field)
fn compute_sum(store: &NodeStore, docs: &[&DocumentMatch], field: &FieldPath) -> AggregateValue {
    let sum: f64 = docs
        .iter()
        .filter_map(|m| extract_number(store, &m.document, field))
        .sum();
    AggregateValue::Float(sum)
}

/// AVG(field)
fn compute_avg(store: &NodeStore, docs: &[&DocumentMatch], field: &FieldPath) -> AggregateValue {
    let values: Vec<f64> = docs
        .iter()
        .filter_map(|m| extract_number(store, &m.document, field))
        .collect();

    if values.is_empty() {
        AggregateValue::Null
    } else {
        let sum: f64 = values.iter().sum();
        AggregateValue::Float(sum / values.len() as f64)
    }
}

/// MIN(field)
fn compute_min(store: &NodeStore, docs: &[&DocumentMatch], field: &FieldPath) -> AggregateValue {
    let min = docs
        .iter()
        .filter_map(|m| extract_number(store, &m.document, field))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    match min {
        Some(v) => AggregateValue::Float(v),
        None => AggregateValue::Null,
    }
}

/// MAX(field)
fn compute_max(store: &NodeStore, docs: &[&DocumentMatch], field: &FieldPath) -> AggregateValue {
    let max = docs
        .iter()
        .filter_map(|m| extract_number(store, &m.document, field))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    match max {
        Some(v) => AggregateValue::Float(v),
        None => AggregateValue::Null,
    }
}

/// Helper to extract a numeric value from a document field.
fn extract_number(
    store: &NodeStore,
    doc: &crate::model::Document,
    field: &FieldPath,
) -> Option<f64> {
    if let Some(Value::Number(n)) = get_field_value(store, doc, field) {
        Some(n)
    } else {
        None
    }
}
