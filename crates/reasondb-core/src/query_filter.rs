//! Query term extraction for smarter BM25 and tree-grep filtering.
//!
//! Strips stop words and stems natural language queries to produce
//! clean search terms that match what Tantivy indexes.

use std::collections::HashSet;

use rust_stemmers::{Algorithm, Stemmer};

/// Extract meaningful search terms from a natural language query.
///
/// Pipeline: tokenize → lowercase → remove stop words → stem → deduplicate.
/// Returned terms are sorted longest-first (longer = more specific = higher signal).
pub fn extract_query_terms(query: &str) -> Vec<String> {
    let stop_words = get_stop_words();
    let stemmer = Stemmer::create(Algorithm::English);

    let mut seen = HashSet::new();
    let mut terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 2 && !stop_words.contains(w.as_str()))
        .map(|w| stemmer.stem(&w).into_owned())
        .filter(|w| w.len() >= 2 && seen.insert(w.clone()))
        .collect();

    terms.sort_by_key(|b| std::cmp::Reverse(b.len()));
    terms
}

/// Check if any of `terms` appear (case-insensitive) in `text`.
/// Returns the number of distinct terms that matched.
pub fn count_term_matches(text: &str, terms: &[String]) -> usize {
    let lower = text.to_lowercase();
    terms.iter().filter(|t| lower.contains(t.as_str())).count()
}

/// Check if `text` contains at least one of the given terms (case-insensitive).
pub fn has_any_term(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    terms.iter().any(|t| lower.contains(t.as_str()))
}

fn get_stop_words() -> HashSet<&'static str> {
    stop_words::get(stop_words::LANGUAGE::English)
        .iter()
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_removes_stop_words() {
        let terms = extract_query_terms("What are the termination clauses?");
        assert!(!terms
            .iter()
            .any(|t| t == "what" || t == "are" || t == "the"));
        assert!(terms.iter().any(|t| t == "termin"));
        assert!(terms.iter().any(|t| t == "claus"));
    }

    #[test]
    fn test_extract_stems_words() {
        let terms = extract_query_terms("running payments termination");
        assert!(terms.iter().any(|t| t == "run"));
        assert!(terms.iter().any(|t| t == "termin"));
        assert!(terms.iter().any(|t| t == "payment"));
    }

    #[test]
    fn test_extract_deduplicates() {
        let terms = extract_query_terms("run running runs runner");
        let run_count = terms.iter().filter(|t| t.as_str() == "run").count();
        assert_eq!(run_count, 1);
    }

    #[test]
    fn test_extract_sorted_by_length() {
        let terms = extract_query_terms("revenue termination deal Q3");
        if terms.len() >= 2 {
            for i in 0..terms.len() - 1 {
                assert!(terms[i].len() >= terms[i + 1].len());
            }
        }
    }

    #[test]
    fn test_extract_empty_query() {
        let terms = extract_query_terms("");
        assert!(terms.is_empty());
    }

    #[test]
    fn test_extract_all_stop_words() {
        let terms = extract_query_terms("what is the");
        assert!(terms.is_empty());
    }

    #[test]
    fn test_count_term_matches() {
        let terms = extract_query_terms("termination clauses");
        let count = count_term_matches("The Termination section covers exit clauses", &terms);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_has_any_term() {
        let terms = extract_query_terms("revenue growth");
        assert!(has_any_term("Revenue was up 15%", &terms));
        assert!(!has_any_term("The company expanded", &terms));
    }
}
