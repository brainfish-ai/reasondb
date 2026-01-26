//! Text chunking and structure detection
//!
//! Splits documents into semantic chunks and detects hierarchical structure.

use regex::Regex;
use tracing::debug;
use unicode_segmentation::UnicodeSegmentation;

use crate::error::Result;
use crate::pdf::ExtractedPage;

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

/// Semantic text chunker with heading detection
pub struct SemanticChunker {
    config: ChunkerConfig,
    heading_patterns: Vec<HeadingPattern>,
}

struct HeadingPattern {
    regex: Regex,
    level: u8,
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self::new(ChunkerConfig::default())
    }
}

impl SemanticChunker {
    /// Create a new chunker with the given configuration
    pub fn new(config: ChunkerConfig) -> Self {
        let heading_patterns = Self::build_heading_patterns();
        Self {
            config,
            heading_patterns,
        }
    }

    /// Build regex patterns for heading detection
    fn build_heading_patterns() -> Vec<HeadingPattern> {
        vec![
            // Chapter headings: "Chapter 1", "CHAPTER ONE", etc.
            HeadingPattern {
                regex: Regex::new(r"(?i)^(?:chapter|part)\s+(?:\d+|[ivxlc]+|one|two|three|four|five|six|seven|eight|nine|ten)[:\.\s]").unwrap(),
                level: 1,
            },
            // Numbered sections: "1.", "1.1", "1.1.1", etc.
            HeadingPattern {
                regex: Regex::new(r"^(\d+\.)+\s+[A-Z]").unwrap(),
                level: 2,
            },
            // All caps headings (likely section titles)
            HeadingPattern {
                regex: Regex::new(r"^[A-Z][A-Z\s]{10,50}$").unwrap(),
                level: 2,
            },
            // Title case lines that are short (likely headings)
            HeadingPattern {
                regex: Regex::new(r"^(?:[A-Z][a-z]+\s+){1,6}[A-Z][a-z]+$").unwrap(),
                level: 3,
            },
            // Lettered sections: "A.", "A.1", "(a)", etc.
            HeadingPattern {
                regex: Regex::new(r"^(?:[A-Z]\.|\([a-z]\))\s+[A-Z]").unwrap(),
                level: 3,
            },
            // Roman numeral sections
            HeadingPattern {
                regex: Regex::new(r"^(?:[IVXLC]+\.)\s+[A-Z]").unwrap(),
                level: 2,
            },
        ]
    }

    /// Chunk pages into semantic chunks
    pub fn chunk_pages(&self, pages: &[ExtractedPage]) -> Result<Vec<TextChunk>> {
        // Combine all pages into one document with page markers
        let mut full_text = String::new();
        let mut page_offsets: Vec<(usize, usize)> = Vec::new(); // (offset, page_number)

        for page in pages {
            page_offsets.push((full_text.len(), page.page_number));
            full_text.push_str(&page.text);
            full_text.push('\n');
        }

        // Detect headings first
        let headings = if self.config.detect_headings {
            self.detect_headings(&full_text, &page_offsets)
        } else {
            Vec::new()
        };

        debug!("Detected {} headings", headings.len());

        // Create chunks based on headings and size
        self.create_chunks(&full_text, &headings, &page_offsets)
    }

    /// Chunk a single text string
    pub fn chunk_text(&self, text: &str) -> Result<Vec<TextChunk>> {
        let headings = if self.config.detect_headings {
            self.detect_headings(text, &[])
        } else {
            Vec::new()
        };

        self.create_chunks(text, &headings, &[])
    }

    /// Detect headings in the text
    fn detect_headings(&self, text: &str, page_offsets: &[(usize, usize)]) -> Vec<DetectedHeading> {
        let mut headings = Vec::new();
        let mut offset = 0;

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                offset += line.len() + 1;
                continue;
            }

            // Check against heading patterns
            for pattern in &self.heading_patterns {
                if pattern.regex.is_match(trimmed) {
                    let page_number = self.find_page_for_offset(offset, page_offsets);

                    headings.push(DetectedHeading {
                        text: trimmed.to_string(),
                        level: pattern.level,
                        offset,
                        page_number,
                    });
                    break;
                }
            }

            offset += line.len() + 1;
        }

        headings
    }

    /// Find which page contains a given offset
    fn find_page_for_offset(&self, offset: usize, page_offsets: &[(usize, usize)]) -> Option<usize> {
        for (i, &(page_offset, page_num)) in page_offsets.iter().enumerate() {
            let next_offset = page_offsets
                .get(i + 1)
                .map(|&(o, _)| o)
                .unwrap_or(usize::MAX);

            if offset >= page_offset && offset < next_offset {
                return Some(page_num);
            }
        }
        None
    }

    /// Create chunks from text, respecting headings
    fn create_chunks(
        &self,
        text: &str,
        headings: &[DetectedHeading],
        page_offsets: &[(usize, usize)],
    ) -> Result<Vec<TextChunk>> {
        let mut chunks = Vec::new();
        let mut chunk_id = 0;

        // If we have headings, use them as split points
        if !headings.is_empty() {
            let mut heading_iter = headings.iter().peekable();

            while let Some(heading) = heading_iter.next() {
                let start = heading.offset;
                let end = heading_iter
                    .peek()
                    .map(|h| h.offset)
                    .unwrap_or(text.len());

                let section_text = &text[start..end];

                // Split large sections into smaller chunks
                let section_chunks = self.split_section(section_text, Some(heading.clone()));

                for mut chunk in section_chunks {
                    chunk.id = format!("chunk_{}", chunk_id);
                    chunk.start_page = self.find_page_for_offset(start, page_offsets);
                    chunk.end_page = self.find_page_for_offset(
                        start + chunk.content.len().min(section_text.len()),
                        page_offsets,
                    );
                    chunks.push(chunk);
                    chunk_id += 1;
                }
            }

            // Handle text before first heading
            if let Some(first_heading) = headings.first() {
                if first_heading.offset > 0 {
                    let preamble = &text[..first_heading.offset];
                    if preamble.trim().len() >= self.config.min_chunk_size {
                        let preamble_chunks = self.split_section(preamble, None);
                        for mut chunk in preamble_chunks {
                            chunk.id = format!("chunk_preamble_{}", chunk_id);
                            chunks.insert(0, chunk);
                            chunk_id += 1;
                        }
                    }
                }
            }
        } else {
            // No headings - split by size only
            let size_chunks = self.split_by_size(text);
            for (i, mut chunk) in size_chunks.into_iter().enumerate() {
                chunk.id = format!("chunk_{}", i);
                chunks.push(chunk);
            }
        }

        Ok(chunks)
    }

    /// Split a section that may be too large
    fn split_section(&self, text: &str, heading: Option<DetectedHeading>) -> Vec<TextChunk> {
        let trimmed = text.trim();
        let char_count = trimmed.chars().count();

        if char_count <= self.config.max_chunk_size {
            // Section fits in one chunk
            return vec![TextChunk {
                id: String::new(),
                content: trimmed.to_string(),
                heading,
                char_count,
                word_count: trimmed.unicode_words().count(),
                start_page: None,
                end_page: None,
            }];
        }

        // Section is too large, split it
        let mut chunks = Vec::new();
        let sub_chunks = self.split_by_size(trimmed);

        for (i, mut chunk) in sub_chunks.into_iter().enumerate() {
            if i == 0 {
                chunk.heading = heading.clone();
            }
            chunks.push(chunk);
        }

        chunks
    }

    /// Split text by size, trying to break at sentence boundaries
    fn split_by_size(&self, text: &str) -> Vec<TextChunk> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_word_count = 0;

        // Split into sentences (roughly)
        let sentences: Vec<&str> = text
            .split(|c| c == '.' || c == '!' || c == '?')
            .collect();

        for sentence in sentences {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }

            let sentence_with_punct = format!("{}. ", sentence);
            let sentence_chars = sentence_with_punct.chars().count();

            if current.chars().count() + sentence_chars > self.config.target_chunk_size
                && current.chars().count() >= self.config.min_chunk_size
            {
                // Current chunk is big enough, save it
                let char_count = current.chars().count();
                chunks.push(TextChunk {
                    id: String::new(),
                    content: current.trim().to_string(),
                    heading: None,
                    char_count,
                    word_count: current_word_count,
                    start_page: None,
                    end_page: None,
                });

                // Start new chunk with overlap
                let overlap_text = self.get_overlap_text(&current);
                current = overlap_text;
                current_word_count = current.unicode_words().count();
            }

            current.push_str(&sentence_with_punct);
            current_word_count += sentence.unicode_words().count();
        }

        // Don't forget the last chunk
        if !current.trim().is_empty() && current.chars().count() >= self.config.min_chunk_size {
            let char_count = current.chars().count();
            chunks.push(TextChunk {
                id: String::new(),
                content: current.trim().to_string(),
                heading: None,
                char_count,
                word_count: current_word_count,
                start_page: None,
                end_page: None,
            });
        }

        chunks
    }

    /// Get overlap text from the end of a chunk
    fn get_overlap_text(&self, text: &str) -> String {
        if self.config.overlap == 0 {
            return String::new();
        }

        let chars: Vec<char> = text.chars().collect();
        if chars.len() <= self.config.overlap {
            return text.to_string();
        }

        let start = chars.len() - self.config.overlap;
        chars[start..].iter().collect()
    }
}

/// Table of Contents extractor
pub struct TocExtractor;

impl TocExtractor {
    /// Try to extract a table of contents from pages
    pub fn extract(pages: &[ExtractedPage]) -> Option<Vec<DetectedHeading>> {
        // Look for a page that looks like a ToC
        for page in pages.iter().take(10) {
            // ToC usually in first 10 pages
            if Self::looks_like_toc(&page.text) {
                return Self::parse_toc(&page.text);
            }
        }
        None
    }

    /// Check if text looks like a table of contents
    fn looks_like_toc(text: &str) -> bool {
        let lower = text.to_lowercase();

        // Check for ToC indicators
        let has_toc_header = lower.contains("table of contents")
            || lower.contains("contents")
            || lower.contains("index");

        // Check for page number patterns (common in ToC)
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

            // Try dotted leader format: "Chapter One ......... 5"
            if let Some(caps) = toc_line.captures(trimmed) {
                let title = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let level = Self::estimate_level(title);

                headings.push(DetectedHeading {
                    text: title.trim().to_string(),
                    level,
                    offset: 0, // Will be resolved later
                    page_number: caps.get(2).and_then(|m| m.as_str().parse().ok()),
                });
            }
            // Try numbered format: "1.2 Some Section 15"
            else if let Some(caps) = numbered_line.captures(trimmed) {
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
    fn test_heading_detection() {
        let chunker = SemanticChunker::default();

        let text = r#"
Chapter 1: Introduction

This is the introduction text that explains the basics.

1.1 Background

Some background information here.

1.2 Motivation

Why we're doing this work.

CONCLUSION

Final thoughts.
"#;

        let headings = chunker.detect_headings(text, &[]);

        // Should detect at least "Chapter 1:" heading
        assert!(headings.len() >= 1, "Expected at least 1 heading, got {}", headings.len());
        assert!(
            headings.iter().any(|h| h.text.contains("Chapter 1")),
            "Expected to find Chapter 1 heading"
        );
    }

    #[test]
    fn test_chunking() {
        let chunker = SemanticChunker::new(ChunkerConfig {
            target_chunk_size: 200,
            min_chunk_size: 50,
            max_chunk_size: 400,
            overlap: 20,
            detect_headings: false,
        });

        let text = "This is a test sentence. ".repeat(50);
        let chunks = chunker.chunk_text(&text).unwrap();

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.char_count >= 50);
            assert!(chunk.char_count <= 400);
        }
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
