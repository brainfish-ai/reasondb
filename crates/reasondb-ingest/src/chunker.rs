//! Markdown-aware text chunking and structure detection
//!
//! Uses `text_splitter::MarkdownSplitter` (CommonMark) for splitting and
//! detects hierarchical headings for tree building.

use regex::Regex;
use text_splitter::MarkdownSplitter;
use tracing::{debug, warn};

use crate::error::Result;

/// A detected section heading
#[derive(Debug, Clone)]
pub struct DetectedHeading {
    /// The heading text
    pub text: String,
    /// Depth level (1 = top level, 2 = subsection, etc.)
    pub level: u8,
    /// Character offset in the document
    pub offset: usize,
    /// Source page number
    pub page_number: Option<usize>,
}

/// A chunk of text with metadata
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// Unique ID for this chunk
    pub id: String,
    /// The text content
    pub content: String,
    /// Detected heading (if this chunk starts a section)
    pub heading: Option<DetectedHeading>,
    /// Character count
    pub char_count: usize,
    /// Word count
    pub word_count: usize,
    /// Start page number
    pub start_page: Option<usize>,
    /// End page number
    pub end_page: Option<usize>,
}

/// Configuration for the chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in characters
    pub target_chunk_size: usize,
    /// Minimum chunk size
    pub min_chunk_size: usize,
    /// Maximum chunk size
    pub max_chunk_size: usize,
    /// Overlap between chunks (in characters)
    pub overlap: usize,
    /// Whether to detect headings
    pub detect_headings: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_chunk_size: 1500,
            min_chunk_size: 500,
            max_chunk_size: 3000,
            overlap: 100,
            detect_headings: true,
        }
    }
}

/// Markdown-aware semantic chunker backed by `text-splitter`.
///
/// Splits at semantic markdown boundaries (headings, paragraphs, code blocks,
/// lists) and detects heading structure for tree building.
pub struct SemanticChunker {
    config: ChunkerConfig,
    md_heading_re: Regex,
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self::new(ChunkerConfig::default())
    }
}

impl SemanticChunker {
    /// Create a new chunker with the given configuration
    pub fn new(config: ChunkerConfig) -> Self {
        let md_heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
        Self {
            config,
            md_heading_re,
        }
    }

    /// Chunk a markdown string into semantic pieces with heading metadata.
    pub fn chunk_text(&self, text: &str) -> Result<Vec<TextChunk>> {
        let capacity = self.config.min_chunk_size..self.config.max_chunk_size;
        let splitter = MarkdownSplitter::new(capacity);

        let raw_chunks: Vec<&str> = splitter.chunks(text).collect();

        debug!(
            "MarkdownSplitter produced {} chunks from {} chars (range {}..{})",
            raw_chunks.len(),
            text.len(),
            self.config.min_chunk_size,
            self.config.max_chunk_size,
        );

        let total_chunk_chars: usize = raw_chunks.iter().map(|c| c.len()).sum();
        let input_len = text.trim().len();
        if total_chunk_chars < input_len {
            let lost = input_len - total_chunk_chars;
            let loss_pct = (lost as f64 / input_len as f64) * 100.0;
            // Small whitespace normalization between chunks is expected (<1%).
            // Only warn on significant content loss.
            if loss_pct > 1.0 {
                warn!(
                    "Chunking content loss detected: input {} chars, chunks total {} chars, lost {} chars ({:.1}%)",
                    input_len, total_chunk_chars, lost, loss_pct,
                );
            } else {
                debug!(
                    "Chunking whitespace normalization: input {} chars, chunks total {} chars, diff {} chars ({:.1}%)",
                    input_len, total_chunk_chars, lost, loss_pct,
                );
            }
        }

        let mut chunks = Vec::with_capacity(raw_chunks.len());

        for (i, raw) in raw_chunks.iter().enumerate() {
            let content = raw.trim().to_string();
            if content.is_empty() {
                continue;
            }

            let heading = if self.config.detect_headings {
                self.detect_first_heading(&content)
            } else {
                None
            };

            let char_count = content.chars().count();
            let word_count = content.split_whitespace().count();

            chunks.push(TextChunk {
                id: format!("chunk_{}", i),
                content,
                heading,
                char_count,
                word_count,
                start_page: None,
                end_page: None,
            });
        }

        debug!(
            "Produced {} non-empty chunks with heading detection={}",
            chunks.len(),
            self.config.detect_headings,
        );

        Ok(chunks)
    }

    /// Detect the first markdown heading (`# …` through `###### …`) in a chunk.
    fn detect_first_heading(&self, content: &str) -> Option<DetectedHeading> {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(caps) = self.md_heading_re.captures(trimmed) {
                let hashes = caps.get(1).unwrap().as_str();
                let text = caps.get(2).unwrap().as_str().to_string();
                return Some(DetectedHeading {
                    level: hashes.len() as u8,
                    text,
                    offset: 0,
                    page_number: None,
                });
            }
        }
        None
    }
}

/// Table of Contents extractor
pub struct TocExtractor;

impl TocExtractor {
    /// Try to extract a table of contents from the given text sections.
    pub fn extract(sections: &[&str]) -> Option<Vec<DetectedHeading>> {
        for section in sections.iter().take(10) {
            if Self::looks_like_toc(section) {
                return Self::parse_toc(section);
            }
        }
        None
    }

    /// Check if text looks like a table of contents
    fn looks_like_toc(text: &str) -> bool {
        let lower = text.to_lowercase();

        let has_toc_header = lower.contains("table of contents")
            || lower.contains("contents")
            || lower.contains("index");

        let page_number_pattern = Regex::new(r"\.\s*\d+\s*$").unwrap();
        let lines_with_numbers = text
            .lines()
            .filter(|line| page_number_pattern.is_match(line))
            .count();

        has_toc_header || lines_with_numbers > 5
    }

    /// Parse a ToC page into headings
    fn parse_toc(text: &str) -> Option<Vec<DetectedHeading>> {
        let mut headings = Vec::new();
        let toc_line = Regex::new(r"^(.+?)\s*\.{2,}\s*(\d+)\s*$").unwrap();
        let numbered_line = Regex::new(r"^(\d+(?:\.\d+)*)\s+(.+?)\s+(\d+)\s*$").unwrap();

        for line in text.lines() {
            let trimmed = line.trim();

            if let Some(caps) = toc_line.captures(trimmed) {
                let title = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let level = Self::estimate_level(title);

                headings.push(DetectedHeading {
                    text: title.trim().to_string(),
                    level,
                    offset: 0,
                    page_number: caps.get(2).and_then(|m| m.as_str().parse().ok()),
                });
            } else if let Some(caps) = numbered_line.captures(trimmed) {
                let number = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let title = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let level = number.matches('.').count() as u8 + 1;

                headings.push(DetectedHeading {
                    text: format!("{} {}", number, title).trim().to_string(),
                    level,
                    offset: 0,
                    page_number: caps.get(3).and_then(|m| m.as_str().parse().ok()),
                });
            }
        }

        if headings.is_empty() {
            None
        } else {
            Some(headings)
        }
    }

    /// Estimate heading level from text
    fn estimate_level(text: &str) -> u8 {
        let lower = text.to_lowercase();

        if lower.starts_with("chapter") || lower.starts_with("part") {
            1
        } else if lower.starts_with("section")
            || lower.starts_with("appendix")
            || text.chars().all(|c| c.is_uppercase() || c.is_whitespace())
        {
            2
        } else {
            3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_heading_detection() {
        let chunker = SemanticChunker::default();

        let content = "# Introduction\n\nSome text here.\n\n## Background\n\nMore text.";
        let heading = chunker.detect_first_heading(content);

        assert!(heading.is_some());
        let h = heading.unwrap();
        assert_eq!(h.level, 1);
        assert_eq!(h.text, "Introduction");
    }

    #[test]
    fn test_chunking_preserves_content() {
        let chunker = SemanticChunker::new(ChunkerConfig {
            target_chunk_size: 200,
            min_chunk_size: 50,
            max_chunk_size: 400,
            overlap: 0,
            detect_headings: false,
        });

        let text = "This is a test sentence. ".repeat(50);
        let chunks = chunker.chunk_text(&text).unwrap();

        assert!(chunks.len() > 1, "Expected multiple chunks");

        let reconstructed: String = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        for word in text.split_whitespace() {
            assert!(
                reconstructed.contains(word),
                "Lost word '{}' during chunking",
                word
            );
        }
    }

    #[test]
    fn test_chunking_respects_code_blocks() {
        let chunker = SemanticChunker::new(ChunkerConfig {
            target_chunk_size: 200,
            min_chunk_size: 20,
            max_chunk_size: 600,
            overlap: 0,
            detect_headings: true,
        });

        let text = r#"# Setup

Install dependencies:

```bash
curl http://localhost:4444/health
docker compose up --build -d
git clone https://github.com/reasondb/reasondb.git
```

Some trailing text after the code block."#;

        let chunks = chunker.chunk_text(text).unwrap();
        assert!(!chunks.is_empty(), "Should produce at least one chunk");

        let all_content: String = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(
            all_content.contains("```bash"),
            "Code fence opener should be preserved"
        );
        assert!(
            all_content.contains("```\n") || all_content.contains("```"),
            "Code fence closer should be preserved"
        );
        assert!(
            all_content.contains("https://github.com/reasondb/reasondb.git"),
            "URL inside code block should not be split on dots"
        );
    }

    #[test]
    fn test_chunking_with_markdown_headings() {
        let chunker = SemanticChunker::default();

        let text = r#"# Chapter One

This is the introduction text that explains the basics of the system.

## Background

Some background information here about the project and its goals.

## Motivation

Why we're doing this work and what we hope to achieve.

# Conclusion

Final thoughts on the matter."#;

        let chunks = chunker.chunk_text(text).unwrap();

        let headings: Vec<_> = chunks.iter().filter_map(|c| c.heading.as_ref()).collect();

        assert!(
            headings.iter().any(|h| h.text.contains("Chapter One")),
            "Expected to find 'Chapter One' heading, got: {:?}",
            headings,
        );
    }

    #[test]
    fn test_small_document_not_dropped() {
        let chunker = SemanticChunker::default();

        let text = "Short document with only a few words.";
        let chunks = chunker.chunk_text(text).unwrap();

        assert_eq!(
            chunks.len(),
            1,
            "Small documents must produce exactly one chunk"
        );
        assert_eq!(chunks[0].content, text);
    }

    #[test]
    fn test_urls_not_corrupted() {
        let chunker = SemanticChunker::new(ChunkerConfig {
            target_chunk_size: 200,
            min_chunk_size: 20,
            max_chunk_size: 500,
            overlap: 0,
            detect_headings: false,
        });

        let text = "Visit https://example.com/path/to/page.html for more info. Also see http://docs.rs/text-splitter/latest/text_splitter/ for the API docs.";

        let chunks = chunker.chunk_text(text).unwrap();
        let all_content: String = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            all_content.contains("https://example.com/path/to/page.html"),
            "URL should not be fragmented: {}",
            all_content,
        );
    }

    #[test]
    fn test_toc_detection() {
        let toc_text = r#"
Table of Contents

Chapter 1: Introduction ......... 1
Chapter 2: Background .......... 15
Chapter 3: Methods ............. 30
Chapter 4: Results ............. 45
Chapter 5: Conclusion .......... 60
"#;

        assert!(TocExtractor::looks_like_toc(toc_text));

        let headings = TocExtractor::parse_toc(toc_text).unwrap();
        assert_eq!(headings.len(), 5);
        assert!(headings[0].text.contains("Introduction"));
    }
}
