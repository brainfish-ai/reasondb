//! LLM-based summarization for document nodes
//!
//! Generates summaries for each node in the document tree,
//! working bottom-up from leaves to root.

use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info};

use reasondb_core::llm::{ReasoningEngine, SummarizationContext};
use reasondb_core::model::PageNode;

use crate::error::{IngestError, Result};

/// Configuration for summarization
#[derive(Debug, Clone)]
pub struct SummarizerConfig {
    /// Maximum content length to send to LLM
    pub max_content_length: usize,
    /// Whether to include child summaries in parent summarization
    pub include_child_summaries: bool,
    /// Maximum concurrent summarization requests
    pub max_concurrent: usize,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            max_content_length: 8000,
            include_child_summaries: true,
            max_concurrent: 5,
        }
    }
}

/// Summarizer that uses an LLM to generate node summaries
pub struct NodeSummarizer<'a, R: ReasoningEngine> {
    reasoner: &'a R,
    config: SummarizerConfig,
}

impl<'a, R: ReasoningEngine> NodeSummarizer<'a, R> {
    /// Create a new summarizer with the given reasoning engine
    pub fn new(reasoner: &'a R) -> Self {
        Self {
            reasoner,
            config: SummarizerConfig::default(),
        }
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: SummarizerConfig) -> Self {
        self.config = config;
        self
    }

    /// Summarize all nodes in a document tree (bottom-up, concurrent per depth level)
    pub async fn summarize_tree(&self, nodes: &mut [PageNode]) -> Result<()> {
        info!(
            "Summarizing {} nodes (max_concurrent: {})",
            nodes.len(),
            self.config.max_concurrent
        );

        let node_map: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();

        let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));

        for depth in (0..=max_depth).rev() {
            let indices_at_depth: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.depth == depth)
                .map(|(i, _)| i)
                .collect();

            debug!(
                "Summarizing {} nodes at depth {} (concurrent)",
                indices_at_depth.len(),
                depth
            );

            // Collect all inputs for this depth level (immutable reads)
            let work_items: Vec<(usize, String, SummarizationContext)> = indices_at_depth
                .iter()
                .map(|&idx| {
                    let node = &nodes[idx];
                    let content = self.get_summarization_content(node, nodes, &node_map);
                    let parent_summary = node
                        .parent_id
                        .as_ref()
                        .and_then(|pid| node_map.get(pid).map(|&i| nodes[i].summary.clone()));
                    let context = SummarizationContext {
                        title: Some(node.title.clone()),
                        parent_summary,
                        depth: node.depth,
                        is_leaf: node.is_leaf(),
                    };
                    (idx, content, context)
                })
                .collect();

            // Run LLM calls concurrently, bounded by semaphore
            let mut futures = FuturesUnordered::new();

            for (idx, content, context) in work_items {
                let permit = semaphore.clone();
                futures.push(async move {
                    let _permit = permit.acquire().await.unwrap();
                    if content.is_empty() {
                        return Ok((
                            idx,
                            format!(
                                "Section: {}",
                                context.title.as_deref().unwrap_or("Untitled")
                            ),
                        ));
                    }
                    let truncated: String = content
                        .chars()
                        .take(self.config.max_content_length)
                        .collect();
                    let summary = self
                        .reasoner
                        .summarize(&truncated, &context)
                        .await
                        .map_err(|e| IngestError::Summarization(e.to_string()))?;
                    Ok::<(usize, String), IngestError>((idx, summary))
                });
            }

            // Collect results and write back
            while let Some(result) = futures.next().await {
                let (idx, summary) = result?;
                nodes[idx].summary = summary;
            }
        }

        info!("Completed summarization");
        Ok(())
    }

    /// Get the content to summarize for a node
    fn get_summarization_content(
        &self,
        node: &PageNode,
        all_nodes: &[PageNode],
        node_map: &HashMap<String, usize>,
    ) -> String {
        if node.is_leaf() {
            node.content.clone().unwrap_or_default()
        } else if self.config.include_child_summaries {
            self.combine_child_summaries(node, all_nodes, node_map)
        } else {
            node.title.clone()
        }
    }

    /// Combine child summaries for a parent node
    fn combine_child_summaries(
        &self,
        node: &PageNode,
        all_nodes: &[PageNode],
        node_map: &HashMap<String, usize>,
    ) -> String {
        let mut parts = Vec::new();

        for child_id in &node.children_ids {
            if let Some(&idx) = node_map.get(child_id) {
                let child = &all_nodes[idx];
                if !child.summary.is_empty() {
                    parts.push(format!("- {}: {}", child.title, child.summary));
                } else {
                    parts.push(format!("- {}", child.title));
                }
            }
        }

        if parts.is_empty() {
            node.title.clone()
        } else {
            format!("Contains sections:\n{}", parts.join("\n"))
        }
    }
}

/// A mock summarizer for testing (doesn't use LLM)
pub struct MockSummarizer;

impl MockSummarizer {
    /// Generate mock summaries for all nodes
    pub fn summarize_tree(nodes: &mut [PageNode]) {
        for node in nodes {
            node.summary = if node.is_leaf() {
                // For leaves, use first 100 chars of content
                node.content
                    .as_ref()
                    .map(|c| {
                        let preview: String = c.chars().take(100).collect();
                        format!("Content preview: {}...", preview)
                    })
                    .unwrap_or_else(|| format!("Section: {}", node.title))
            } else {
                // For non-leaves, list children
                if node.children_ids.is_empty() {
                    format!("Section: {}", node.title)
                } else {
                    format!(
                        "Section containing {} sub-sections",
                        node.children_ids.len()
                    )
                }
            };
        }
    }
}

/// Batch summarization that sends multiple nodes per LLM request.
///
/// Reduces API round-trips from N (one per node) to ceil(N / batch_size)
/// per depth level. Bottom-up ordering is preserved so parent nodes
/// always have access to child summaries. Within each depth level, all
/// batches are dispatched concurrently (bounded by `max_concurrent`).
pub struct BatchSummarizer<'a, R: ReasoningEngine> {
    reasoner: &'a R,
    batch_size: usize,
    max_concurrent: usize,
}

impl<'a, R: ReasoningEngine> BatchSummarizer<'a, R> {
    /// Create a new batch summarizer.
    /// `batch_size` controls how many nodes are sent per LLM request.
    /// `max_concurrent` caps the number of in-flight LLM calls per depth level.
    pub fn new(reasoner: &'a R, batch_size: usize, max_concurrent: usize) -> Self {
        Self {
            reasoner,
            batch_size,
            max_concurrent: max_concurrent.max(1),
        }
    }

    /// Summarize all nodes in a document tree using batched LLM requests.
    ///
    /// Depth levels are processed sequentially (bottom-up) so parent nodes always
    /// see completed child summaries. Within each depth level, all batches are
    /// dispatched in parallel bounded by `max_concurrent`.
    pub async fn summarize_batch(&self, nodes: &mut [PageNode]) -> Result<()> {
        let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);

        let node_map: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();

        info!(
            "Batch-summarizing {} nodes (batch_size: {}, max_concurrent: {})",
            nodes.len(),
            self.batch_size,
            self.max_concurrent
        );

        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

        for depth in (0..=max_depth).rev() {
            let indices: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.depth == depth)
                .map(|(i, _)| i)
                .collect();

            debug!(
                "Batch-summarizing {} nodes at depth {} ({} concurrent)",
                indices.len(),
                depth,
                self.max_concurrent
            );

            // Build all batch inputs upfront (immutable reads from nodes).
            // Empty-content nodes are handled immediately; the rest are collected
            // into batches that will be dispatched concurrently below.
            #[allow(clippy::type_complexity)]
            let mut all_batches: Vec<(
                Vec<(usize, String)>,
                Vec<(String, String, SummarizationContext)>,
            )> = Vec::new();

            for batch_indices in indices.chunks(self.batch_size) {
                let mut llm_items: Vec<(usize, String, String, SummarizationContext)> = Vec::new();

                for &idx in batch_indices {
                    let node = &nodes[idx];
                    let content = self.get_content_for_node(node, nodes, &node_map);

                    if content.is_empty() {
                        nodes[idx].summary = format!("Section: {}", nodes[idx].title);
                        continue;
                    }

                    let parent_summary = node
                        .parent_id
                        .as_ref()
                        .and_then(|pid| node_map.get(pid).map(|&i| nodes[i].summary.clone()));
                    let context = SummarizationContext {
                        title: Some(node.title.clone()),
                        parent_summary,
                        depth: node.depth,
                        is_leaf: node.is_leaf(),
                    };
                    llm_items.push((idx, node.id.clone(), content, context));
                }

                if llm_items.is_empty() {
                    continue;
                }

                let batch_input: Vec<(String, String, SummarizationContext)> = llm_items
                    .iter()
                    .map(|(_, id, content, ctx)| (id.clone(), content.clone(), ctx.clone()))
                    .collect();

                let idx_map: Vec<(usize, String)> = llm_items
                    .iter()
                    .map(|(idx, id, _, _)| (*idx, id.clone()))
                    .collect();

                all_batches.push((idx_map, batch_input));
            }

            // Dispatch all batches for this depth level concurrently.
            let mut futures = FuturesUnordered::new();
            for (idx_map, batch_input) in all_batches {
                let permit = semaphore.clone();
                futures.push(async move {
                    let _permit = permit.acquire().await.unwrap();
                    let summaries = self
                        .reasoner
                        .summarize_batch(&batch_input)
                        .await
                        .map_err(|e| IngestError::Summarization(e.to_string()))?;
                    Ok::<(Vec<(usize, String)>, HashMap<String, String>), IngestError>((
                        idx_map,
                        summaries.into_iter().collect(),
                    ))
                });
            }

            // Collect results and write summaries back.
            while let Some(result) = futures.next().await {
                let (idx_map, summary_map) = result?;
                for (idx, node_id) in idx_map {
                    if let Some(summary) = summary_map.get(&node_id) {
                        nodes[idx].summary = summary.clone();
                    } else {
                        nodes[idx].summary = format!("Section: {}", nodes[idx].title);
                    }
                }
            }
        }

        info!("Completed batch summarization");
        Ok(())
    }

    fn get_content_for_node(
        &self,
        node: &PageNode,
        all_nodes: &[PageNode],
        node_map: &HashMap<String, usize>,
    ) -> String {
        if node.is_leaf() {
            node.content.clone().unwrap_or_default()
        } else {
            let mut parts = Vec::new();
            for child_id in &node.children_ids {
                if let Some(&idx) = node_map.get(child_id) {
                    let child = &all_nodes[idx];
                    if !child.summary.is_empty() {
                        parts.push(format!("- {}: {}", child.title, child.summary));
                    }
                }
            }
            parts.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_nodes() -> Vec<PageNode> {
        let mut root = PageNode::new_root("doc1".to_string(), "Document".to_string());
        root.id = "root".to_string();
        root.children_ids = vec!["ch1".to_string(), "ch2".to_string()];

        let mut ch1 = PageNode::new_leaf(
            "doc1".to_string(),
            "Chapter 1".to_string(),
            "This is chapter 1 content about introduction.".to_string(),
            String::new(),
            1,
        );
        ch1.id = "ch1".to_string();
        ch1.parent_id = Some("root".to_string());

        let mut ch2 = PageNode::new_leaf(
            "doc1".to_string(),
            "Chapter 2".to_string(),
            "This is chapter 2 content about methods.".to_string(),
            String::new(),
            1,
        );
        ch2.id = "ch2".to_string();
        ch2.parent_id = Some("root".to_string());

        vec![root, ch1, ch2]
    }

    #[test]
    fn test_mock_summarizer() {
        let mut nodes = create_test_nodes();
        MockSummarizer::summarize_tree(&mut nodes);

        // Root should have children count
        assert!(nodes[0].summary.contains("2 sub-sections"));

        // Leaves should have content preview
        assert!(nodes[1].summary.contains("Content preview"));
        assert!(nodes[2].summary.contains("Content preview"));
    }
}
