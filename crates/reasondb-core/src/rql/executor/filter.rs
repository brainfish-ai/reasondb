//! Document filtering and condition matching
//!
//! This module handles WHERE clause evaluation and document filtering.

use crate::model::Document;
use crate::rql::ast::*;
use crate::store::NodeStore;

/// Check if a document matches a condition.
pub fn matches_condition(store: &NodeStore, doc: &Document, condition: &Condition) -> bool {
    match condition {
        Condition::Comparison(comp) => matches_comparison(store, doc, comp),
        Condition::And(left, right) => {
            matches_condition(store, doc, left) && matches_condition(store, doc, right)
        }
        Condition::Or(left, right) => {
            matches_condition(store, doc, left) || matches_condition(store, doc, right)
        }
        Condition::Not(inner) => !matches_condition(store, doc, inner),
    }
}

/// Check if a document matches a comparison.
fn matches_comparison(store: &NodeStore, doc: &Document, comp: &Comparison) -> bool {
    let field_value = get_field_value(store, doc, &comp.left);

    match comp.operator {
        ComparisonOp::Eq => field_value == Some(comp.right.clone()),
        ComparisonOp::Ne => field_value != Some(comp.right.clone()),
        ComparisonOp::Lt => compare_values(&field_value, &comp.right, |a, b| a < b),
        ComparisonOp::Gt => compare_values(&field_value, &comp.right, |a, b| a > b),
        ComparisonOp::Le => compare_values(&field_value, &comp.right, |a, b| a <= b),
        ComparisonOp::Ge => compare_values(&field_value, &comp.right, |a, b| a >= b),
        ComparisonOp::Like => matches_like(&field_value, &comp.right),
        ComparisonOp::In => matches_in(&field_value, &comp.right),
        ComparisonOp::ContainsAll => matches_contains_all(doc, &comp.left, &comp.right),
        ComparisonOp::ContainsAny => matches_contains_any(doc, &comp.left, &comp.right),
        ComparisonOp::IsNull => field_value.is_none(),
        ComparisonOp::IsNotNull => field_value.is_some(),
    }
}

/// Get a field value from a document.
pub fn get_field_value(_store: &NodeStore, doc: &Document, path: &FieldPath) -> Option<Value> {
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
        "source_url" => doc.source_url.as_ref().map(|u| Value::String(u.clone())),
        "language" => doc.language.as_ref().map(|l| Value::String(l.clone())),
        "version" => doc.version.as_ref().map(|v| Value::String(v.clone())),
        "tags" => Some(Value::Array(
            doc.tags.iter().map(|t| Value::String(t.clone())).collect(),
        )),
        "metadata" => {
            // Handle metadata.field_name with support for deeply nested paths
            if path.segments.len() > 1 {
                if let PathSegment::Field(key) = &path.segments[1] {
                    if let Some(json_value) = doc.metadata.get(key) {
                        // If there are more segments, traverse the nested structure
                        if path.segments.len() > 2 {
                            return traverse_json_path(json_value, &path.segments[2..]);
                        } else {
                            return Some(json_to_value(json_value));
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Traverse a JSON value following a path of segments.
/// Supports nested objects (metadata.employee.department) and arrays (metadata.parties[0].name).
fn traverse_json_path(json: &serde_json::Value, segments: &[PathSegment]) -> Option<Value> {
    if segments.is_empty() {
        return Some(json_to_value(json));
    }

    match &segments[0] {
        PathSegment::Field(key) => {
            // Access object field
            if let serde_json::Value::Object(obj) = json {
                if let Some(value) = obj.get(key) {
                    return traverse_json_path(value, &segments[1..]);
                }
            }
            None
        }
        PathSegment::Index(idx) => {
            // Access array element
            if let serde_json::Value::Array(arr) = json {
                if let Some(value) = arr.get(*idx) {
                    return traverse_json_path(value, &segments[1..]);
                }
            }
            None
        }
    }
}

/// Compare two values with a comparator.
fn compare_values<F>(left: &Option<Value>, right: &Value, cmp: F) -> bool
where
    F: Fn(f64, f64) -> bool,
{
    match (left, right) {
        (Some(Value::Number(a)), Value::Number(b)) => cmp(*a, *b),
        _ => false,
    }
}

/// Check if a value matches a LIKE pattern.
fn matches_like(value: &Option<Value>, pattern: &Value) -> bool {
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
fn matches_in(value: &Option<Value>, list: &Value) -> bool {
    match (value, list) {
        (Some(v), Value::Array(arr)) => arr.contains(v),
        _ => false,
    }
}

/// Check if document field contains all specified values.
fn matches_contains_all(doc: &Document, path: &FieldPath, values: &Value) -> bool {
    let field_name = path.first_field().unwrap_or("");
    match (field_name, values) {
        ("tags", Value::Array(required)) => required.iter().all(|v| match v {
            Value::String(tag) => doc.tags.contains(tag),
            _ => false,
        }),
        _ => false,
    }
}

/// Check if document field contains any of the specified values.
fn matches_contains_any(doc: &Document, path: &FieldPath, values: &Value) -> bool {
    let field_name = path.first_field().unwrap_or("");
    match (field_name, values) {
        ("tags", Value::Array(candidates)) => candidates.iter().any(|v| match v {
            Value::String(tag) => doc.tags.contains(tag),
            _ => false,
        }),
        _ => false,
    }
}

/// Sort documents by a field, including `metadata.*` paths.
pub fn sort_documents(docs: &mut [Document], order_by: &OrderByClause) {
    let desc = order_by.direction == SortDirection::Desc;
    docs.sort_by(|a, b| {
        let cmp = compare_docs_for_order_by(a, b, order_by);
        if desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// Compare two documents according to an ORDER BY clause.
///
/// Supports top-level fields (`title`, `created_at`, `updated_at`) and
/// `metadata.<key>` paths (including deeply nested paths such as
/// `metadata.employee.department`). Numbers are compared numerically,
/// strings lexicographically, booleans by value. `null` / missing values
/// sort last.
pub fn compare_docs_for_order_by(
    a: &Document,
    b: &Document,
    order_by: &OrderByClause,
) -> std::cmp::Ordering {
    let first = order_by.field.first_field().unwrap_or("");
    match first {
        "title" => a.title.cmp(&b.title),
        "created_at" => a.created_at.cmp(&b.created_at),
        "updated_at" => a.updated_at.cmp(&b.updated_at),
        "metadata" => {
            if let Some(PathSegment::Field(key)) = order_by.field.segments.get(1) {
                let va = get_metadata_sort_value(a, key, &order_by.field.segments[2..]);
                let vb = get_metadata_sort_value(b, key, &order_by.field.segments[2..]);
                compare_json_sort_values(va, vb)
            } else {
                std::cmp::Ordering::Equal
            }
        }
        _ => std::cmp::Ordering::Equal,
    }
}

/// Retrieve a metadata value following an optional chain of sub-segments.
fn get_metadata_sort_value<'a>(
    doc: &'a Document,
    key: &str,
    remaining: &[PathSegment],
) -> Option<&'a serde_json::Value> {
    let mut current = doc.metadata.get(key)?;
    for seg in remaining {
        match seg {
            PathSegment::Field(k) => {
                if let serde_json::Value::Object(obj) = current {
                    current = obj.get(k)?;
                } else {
                    return None;
                }
            }
            PathSegment::Index(i) => {
                if let serde_json::Value::Array(arr) = current {
                    current = arr.get(*i)?;
                } else {
                    return None;
                }
            }
        }
    }
    Some(current)
}

/// Compare two optional JSON values for sort ordering. `None` (missing) sorts last.
fn compare_json_sort_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
        (Some(av), Some(bv)) => match (av, bv) {
            (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
                let af = an.as_f64().unwrap_or(0.0);
                let bf = bn.as_f64().unwrap_or(0.0);
                af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
            }
            (serde_json::Value::String(as_), serde_json::Value::String(bs)) => as_.cmp(bs),
            (serde_json::Value::Bool(ab), serde_json::Value::Bool(bb)) => ab.cmp(bb),
            _ => std::cmp::Ordering::Equal,
        },
    }
}

// ==================== Value Conversion ====================

/// Convert serde_json::Value to RQL Value.
pub fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(_) => Value::Null, // Objects not supported as values
    }
}

/// Convert RQL Value to serde_json::Value.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => serde_json::json!(*n),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
    }
}
