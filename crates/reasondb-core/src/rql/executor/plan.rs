//! Query plan building for EXPLAIN
//!
//! This module generates execution plans showing how a query would be executed.

use crate::rql::ast::*;
use crate::store::NodeStore;

use super::types::{PlanStep, QueryPlan};

/// Build a query execution plan for EXPLAIN queries.
pub fn build_query_plan(query: &Query, table_id: &str, store: &NodeStore) -> QueryPlan {
    let mut steps = Vec::new();
    let mut indexes_used = Vec::new();

    // Step 1: Table access
    add_table_scan_step(&mut steps, &mut indexes_used, table_id);

    // Step 2: Search if present
    if let Some(ref search) = query.search {
        add_search_step(&mut steps, &mut indexes_used, &search.query);
    }

    // Step 3: Reason if present
    if let Some(ref reason) = query.reason {
        add_reason_step(&mut steps, &reason.query);
    }

    // Step 4: WHERE filtering
    if let Some(ref wc) = query.where_clause {
        add_filter_step(&mut steps);
        analyze_condition_indexes(&wc.condition, &mut indexes_used);
    }

    // Step 5: GROUP BY
    if let Some(ref group_by) = query.group_by {
        add_group_by_step(&mut steps, group_by);
    }

    // Step 6: Aggregation
    if let SelectClause::Aggregates(ref aggs) = query.select {
        add_aggregate_step(&mut steps, aggs);
    }

    // Step 7: ORDER BY
    if let Some(ref order_by) = query.order_by {
        add_sort_step(&mut steps, order_by);
    }

    // Step 8: LIMIT
    if let Some(ref limit) = query.limit {
        add_limit_step(&mut steps, limit);
    }

    // Estimate total rows
    let estimated_rows = estimate_rows(store, table_id);

    QueryPlan {
        steps,
        estimated_rows,
        indexes_used,
    }
}

// ==================== Step Builders ====================

fn add_table_scan_step(steps: &mut Vec<PlanStep>, indexes: &mut Vec<String>, table_id: &str) {
    steps.push(PlanStep {
        step_type: "TableScan".to_string(),
        description: format!("Scan table '{}'", table_id),
        estimated_cost: 10,
    });
    indexes.push("idx_table_docs".to_string());
}

fn add_search_step(steps: &mut Vec<PlanStep>, indexes: &mut Vec<String>, query: &str) {
    steps.push(PlanStep {
        step_type: "BM25Search".to_string(),
        description: format!("Full-text search for '{}'", query),
        estimated_cost: 20,
    });
    indexes.push("bm25_full_text".to_string());
}

fn add_reason_step(steps: &mut Vec<PlanStep>, query: &str) {
    steps.push(PlanStep {
        step_type: "LLMReason".to_string(),
        description: format!("LLM semantic search for '{}'", query),
        estimated_cost: 80, // LLM is expensive
    });
}

fn add_filter_step(steps: &mut Vec<PlanStep>) {
    steps.push(PlanStep {
        step_type: "Filter".to_string(),
        description: "Apply WHERE conditions".to_string(),
        estimated_cost: 5,
    });
}

fn add_group_by_step(steps: &mut Vec<PlanStep>, group_by: &GroupByClause) {
    let fields: Vec<_> = group_by
        .fields
        .iter()
        .filter_map(|f| f.first_field())
        .collect();
    steps.push(PlanStep {
        step_type: "GroupBy".to_string(),
        description: format!("Group by {}", fields.join(", ")),
        estimated_cost: 15,
    });
}

fn add_aggregate_step(steps: &mut Vec<PlanStep>, aggs: &[AggregateExpr]) {
    let agg_names: Vec<_> = aggs.iter().map(|a| format!("{:?}", a.function)).collect();
    steps.push(PlanStep {
        step_type: "Aggregate".to_string(),
        description: format!("Compute {}", agg_names.join(", ")),
        estimated_cost: 5,
    });
}

fn add_sort_step(steps: &mut Vec<PlanStep>, order_by: &OrderByClause) {
    let field = order_by.field.first_field().unwrap_or("?");
    let dir = if order_by.direction == SortDirection::Desc {
        "DESC"
    } else {
        "ASC"
    };
    steps.push(PlanStep {
        step_type: "Sort".to_string(),
        description: format!("Sort by {} {}", field, dir),
        estimated_cost: 10,
    });
}

fn add_limit_step(steps: &mut Vec<PlanStep>, limit: &LimitClause) {
    steps.push(PlanStep {
        step_type: "Limit".to_string(),
        description: format!(
            "Return {} rows (offset {})",
            limit.count,
            limit.offset.unwrap_or(0)
        ),
        estimated_cost: 1,
    });
}

// ==================== Index Analysis ====================

/// Analyze a condition tree for index usage.
fn analyze_condition_indexes(condition: &Condition, indexes: &mut Vec<String>) {
    match condition {
        Condition::Comparison(comp) => {
            if let Some(field) = comp.left.first_field() {
                match field {
                    "table_id" => indexes.push("idx_table_docs".to_string()),
                    "tags" => indexes.push("idx_tag_docs".to_string()),
                    "author" => indexes.push("idx_author_docs".to_string()),
                    _ if field.starts_with("metadata.") => {
                        indexes.push("idx_metadata".to_string());
                    }
                    _ => {}
                }
            }
        }
        Condition::And(left, right) | Condition::Or(left, right) => {
            analyze_condition_indexes(left, indexes);
            analyze_condition_indexes(right, indexes);
        }
        Condition::Not(inner) => {
            analyze_condition_indexes(inner, indexes);
        }
    }
}

/// Estimate row count for a table.
fn estimate_rows(_store: &NodeStore, _table_id: &str) -> usize {
    // TODO: Get actual table statistics
    100 // Placeholder
}
