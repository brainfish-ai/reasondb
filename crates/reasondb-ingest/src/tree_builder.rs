//! Tree builder for converting chunks into a hierarchical structure
//!
//! Builds a tree of PageNodes from text chunks, respecting detected headings.

use tracing::debug;
use uuid::Uuid;

use reasondb_core::model::{Document, NodeMetadata, PageNode};

use crate::chunker::{DetectedHeading, TextChunk};
use crate::error::Result;

/// A node in the building tree (before converting to PageNode)
#[derive(Debug)]
struct BuildNode {
    id: String,
    title: String,
    content: String,
    depth: u8,
    children: Vec<String>,
    parent_id: Option<String>,
    page_number: Option<u32>,
    is_leaf: bool,
}

/// Builder for constructing hierarchical document trees
pub struct TreeBuilder {
    /// Maximum depth allowed
    max_depth: u8,
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeBuilder {
    /// Create a new tree builder
    pub fn new() -> Self {
        Self { max_depth: 10 }
    }

    /// Set maximum tree depth
    pub fn with_max_depth(mut self, depth: u8) -> Self {
        self.max_depth = depth;
        self
    }

    /// Build a tree from chunks
    ///
    /// The `table_id` must reference an existing table in the database.
    pub fn build(
        &self,
        document_title: &str,
        table_id: &str,
        chunks: Vec<TextChunk>,
    ) -> Result<(Document, Vec<PageNode>)> {
        if chunks.is_empty() {
            return Ok(self.create_empty_document(document_title, table_id));
        }

        // Create document in the specified table
        let mut document = Document::new(document_title.to_string(), table_id);
        let document_id = document.id.clone();

        // Build the tree structure
        let (root_id, nodes) = self.build_tree(&document_id, document_title, &chunks);

        // Convert to PageNodes
        let page_nodes: Vec<PageNode> = nodes
            .into_iter()
            .map(|n| self.to_page_node(n, &document_id))
            .collect();

        let max_depth = page_nodes.iter().map(|n| n.depth).max().unwrap_or(0);

        // Update document
        document.root_node_id = root_id;
        document.total_nodes = page_nodes.len();
        document.max_depth = max_depth;

        debug!(
            "Built tree with {} nodes, max depth {}",
            page_nodes.len(),
            max_depth
        );

        Ok((document, page_nodes))
    }

    /// Build tree structure from chunks
    fn build_tree(
        &self,
        document_id: &str,
        document_title: &str,
        chunks: &[TextChunk],
    ) -> (String, Vec<BuildNode>) {
        let root_id = format!("{}_root", document_id);
        let mut nodes = Vec::new();
        let mut level_stack: Vec<(u8, String)> = vec![(0, root_id.clone())];

        // Create root node
        nodes.push(BuildNode {
            id: root_id.clone(),
            title: document_title.to_string(),
            content: String::new(), // Root has no content
            depth: 0,
            children: Vec::new(),
            parent_id: None,
            page_number: None,
            is_leaf: false,
        });

        // Process each chunk
        for chunk in chunks {
            let chunk_level = chunk
                .heading
                .as_ref()
                .map(|h| h.level)
                .unwrap_or(self.max_depth);

            let chunk_title = chunk
                .heading
                .as_ref()
                .map(|h| h.text.clone())
                .unwrap_or_else(|| self.generate_title(&chunk.content));

            // Find parent at appropriate level
            while level_stack.len() > 1 && level_stack.last().unwrap().0 >= chunk_level {
                level_stack.pop();
            }

            let parent_id = level_stack.last().unwrap().1.clone();
            let node_id = format!("{}_{}", document_id, Uuid::new_v4());

            // Create the node
            let node = BuildNode {
                id: node_id.clone(),
                title: chunk_title,
                content: chunk.content.clone(),
                depth: level_stack.len() as u8,
                children: Vec::new(),
                parent_id: Some(parent_id.clone()),
                page_number: chunk.start_page.map(|p| p as u32),
                is_leaf: true, // Will update if children are added
            };

            nodes.push(node);

            // Update parent's children
            if let Some(parent) = nodes.iter_mut().find(|n| n.id == parent_id) {
                parent.children.push(node_id.clone());
                parent.is_leaf = false;
            }

            // Push this node onto the stack if it has a heading
            if chunk.heading.is_some() {
                level_stack.push((chunk_level, node_id));
            }
        }

        (root_id, nodes)
    }

    /// Generate a title from content
    fn generate_title(&self, content: &str) -> String {
        // Take first line or first N characters
        let first_line = content.lines().next().unwrap_or(content);
        let trimmed = first_line.trim();

        if trimmed.len() <= 60 {
            trimmed.to_string()
        } else {
            format!("{}...", &trimmed[..57])
        }
    }

    /// Convert BuildNode to PageNode
    fn to_page_node(&self, node: BuildNode, document_id: &str) -> PageNode {
        let mut page_node = if node.is_leaf && !node.content.is_empty() {
            PageNode::new_leaf(
                document_id.to_string(),
                node.title.clone(),
                node.content.clone(),
                String::new(), // Summary will be filled by summarizer
                node.depth,
            )
        } else {
            PageNode::new(
                document_id.to_string(),
                node.title.clone(),
                None,
                node.depth,
            )
        };

        // Override the ID to use our generated one
        page_node.id = node.id;
        page_node.parent_id = node.parent_id;
        page_node.children_ids = node.children;
        page_node.start_index = 0;
        page_node.end_index = node.content.len();

        if let Some(page_num) = node.page_number {
            page_node.metadata = NodeMetadata::default().with_page(page_num);
        }

        page_node
    }

    /// Create an empty document with just a root node
    fn create_empty_document(&self, title: &str, table_id: &str) -> (Document, Vec<PageNode>) {
        let mut document = Document::new(title.to_string(), table_id);
        let root_node = PageNode::new_root(document.id.clone(), title.to_string());

        document.root_node_id = root_node.id.clone();
        document.total_nodes = 1;
        document.max_depth = 0;

        (document, vec![root_node])
    }

    /// Build tree from pre-detected ToC headings
    /// Build tree from pre-detected ToC headings
    ///
    /// The `table_id` must reference an existing table in the database.
    pub fn build_from_toc(
        &self,
        document_title: &str,
        table_id: &str,
        toc_headings: &[DetectedHeading],
        full_text: &str,
    ) -> Result<(Document, Vec<PageNode>)> {
        if toc_headings.is_empty() {
            return Ok(self.create_empty_document(document_title, table_id));
        }

        // Split text by ToC headings
        let chunks = self.split_by_headings(toc_headings, full_text);

        self.build(document_title, table_id, chunks)
    }

    /// Split text according to ToC headings
    fn split_by_headings(&self, headings: &[DetectedHeading], text: &str) -> Vec<TextChunk> {
        let mut chunks = Vec::new();

        for (i, heading) in headings.iter().enumerate() {
            // Find heading in text
            if let Some(start) = text.find(&heading.text) {
                let end = if i + 1 < headings.len() {
                    text.find(&headings[i + 1].text).unwrap_or(text.len())
                } else {
                    text.len()
                };

                let content = text[start..end].to_string();
                let char_count = content.chars().count();

                chunks.push(TextChunk {
                    id: format!("toc_chunk_{}", i),
                    content,
                    heading: Some(heading.clone()),
                    char_count,
                    word_count: char_count / 5, // Rough estimate
                    start_page: heading.page_number,
                    end_page: headings.get(i + 1).and_then(|h| h.page_number),
                });
            }
        }

        chunks
    }
}

/// Helper to build a flat list into a tree based on heading levels
pub struct HierarchyBuilder;

impl HierarchyBuilder {
    /// Organize flat chunks into a hierarchy based on heading levels
    pub fn organize(chunks: &[TextChunk]) -> Vec<(usize, Vec<usize>)> {
        // Returns: (chunk_index, child_indices)
        let mut result: Vec<(usize, Vec<usize>)> = chunks
            .iter()
            .enumerate()
            .map(|(i, _)| (i, Vec::new()))
            .collect();

        // Build parent-child relationships
        let mut stack: Vec<(u8, usize)> = Vec::new(); // (level, index)

        for (i, chunk) in chunks.iter().enumerate() {
            let level = chunk.heading.as_ref().map(|h| h.level).unwrap_or(255);

            // Pop items from stack that are at same or higher level
            while !stack.is_empty() && stack.last().unwrap().0 >= level {
                stack.pop();
            }

            // Current item's parent is the last item on stack
            if let Some(&(_, parent_idx)) = stack.last() {
                result[parent_idx].1.push(i);
            }

            // Push current item if it has a heading
            if chunk.heading.is_some() {
                stack.push((level, i));
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::DetectedHeading;

    #[test]
    fn test_tree_building() {
        let builder = TreeBuilder::new();

        let chunks = vec![
            TextChunk {
                id: "1".to_string(),
                content: "Introduction content here.".to_string(),
                heading: Some(DetectedHeading {
                    text: "Chapter 1: Introduction".to_string(),
                    level: 1,
                    offset: 0,
                    page_number: Some(1),
                }),
                char_count: 26,
                word_count: 3,
                start_page: Some(1),
                end_page: Some(5),
            },
            TextChunk {
                id: "2".to_string(),
                content: "Background information.".to_string(),
                heading: Some(DetectedHeading {
                    text: "1.1 Background".to_string(),
                    level: 2,
                    offset: 100,
                    page_number: Some(2),
                }),
                char_count: 23,
                word_count: 2,
                start_page: Some(2),
                end_page: Some(3),
            },
            TextChunk {
                id: "3".to_string(),
                content: "Methods content here.".to_string(),
                heading: Some(DetectedHeading {
                    text: "Chapter 2: Methods".to_string(),
                    level: 1,
                    offset: 200,
                    page_number: Some(10),
                }),
                char_count: 21,
                word_count: 3,
                start_page: Some(10),
                end_page: Some(20),
            },
        ];

        let (doc, nodes) = builder
            .build("Test Document", "test-table", chunks)
            .unwrap();

        assert_eq!(doc.title, "Test Document");
        assert_eq!(doc.table_id, "test-table");
        assert!(nodes.len() >= 3); // At least root + 3 chunks

        // Check that we have a root node
        let root = nodes.iter().find(|n| n.parent_id.is_none()).unwrap();
        assert!(!root.children_ids.is_empty());
    }

    #[test]
    fn test_empty_document() {
        let builder = TreeBuilder::new();
        let (doc, nodes) = builder.build("Empty", "test-table", vec![]).unwrap();

        assert_eq!(doc.title, "Empty");
        assert_eq!(doc.total_nodes, 1);
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_hierarchy_organization() {
        let chunks = vec![
            TextChunk {
                id: "1".to_string(),
                content: "Chapter content".to_string(),
                heading: Some(DetectedHeading {
                    text: "Chapter 1".to_string(),
                    level: 1,
                    offset: 0,
                    page_number: None,
                }),
                char_count: 15,
                word_count: 2,
                start_page: None,
                end_page: None,
            },
            TextChunk {
                id: "2".to_string(),
                content: "Section content".to_string(),
                heading: Some(DetectedHeading {
                    text: "Section 1.1".to_string(),
                    level: 2,
                    offset: 100,
                    page_number: None,
                }),
                char_count: 15,
                word_count: 2,
                start_page: None,
                end_page: None,
            },
            TextChunk {
                id: "3".to_string(),
                content: "Another chapter".to_string(),
                heading: Some(DetectedHeading {
                    text: "Chapter 2".to_string(),
                    level: 1,
                    offset: 200,
                    page_number: None,
                }),
                char_count: 15,
                word_count: 2,
                start_page: None,
                end_page: None,
            },
        ];

        let hierarchy = HierarchyBuilder::organize(&chunks);

        // Chapter 1 should have Section 1.1 as child
        assert!(hierarchy[0].1.contains(&1));
        // Chapter 2 should have no children
        assert!(hierarchy[2].1.is_empty());
    }
}
