//! PDF text extraction
//!
//! Extracts text content from PDF files, preserving structure where possible.

use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::{IngestError, Result};

/// Represents a page extracted from a PDF
#[derive(Debug, Clone)]
pub struct ExtractedPage {
    /// Page number (1-indexed)
    pub page_number: usize,
    /// Raw text content
    pub text: String,
    /// Estimated character count
    pub char_count: usize,
}

/// Result of PDF extraction
#[derive(Debug)]
pub struct PdfExtraction {
    /// Document title (from metadata or filename)
    pub title: String,
    /// All extracted pages
    pub pages: Vec<ExtractedPage>,
    /// Total character count
    pub total_chars: usize,
    /// Number of pages
    pub page_count: usize,
}

/// PDF text extractor
pub struct PdfExtractor {
    /// Minimum characters per page to consider valid
    min_chars_per_page: usize,
}

impl Default for PdfExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfExtractor {
    /// Create a new PDF extractor
    pub fn new() -> Self {
        Self {
            min_chars_per_page: 50,
        }
    }

    /// Set minimum characters per page threshold
    pub fn with_min_chars(mut self, min_chars: usize) -> Self {
        self.min_chars_per_page = min_chars;
        self
    }

    /// Extract text from a PDF file
    pub fn extract<P: AsRef<Path>>(&self, path: P) -> Result<PdfExtraction> {
        let path = path.as_ref();
        info!("Extracting PDF: {}", path.display());

        // Get title from filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Try pdf-extract first (better text extraction)
        match self.extract_with_pdf_extract(path) {
            Ok(extraction) => {
                info!(
                    "Successfully extracted {} pages, {} chars",
                    extraction.page_count, extraction.total_chars
                );
                return Ok(PdfExtraction { title, ..extraction });
            }
            Err(e) => {
                warn!("pdf-extract failed: {}, trying lopdf fallback", e);
            }
        }

        // Fallback to lopdf
        match self.extract_with_lopdf(path) {
            Ok(extraction) => {
                info!(
                    "Successfully extracted {} pages, {} chars (lopdf fallback)",
                    extraction.page_count, extraction.total_chars
                );
                Ok(PdfExtraction { title, ..extraction })
            }
            Err(e) => Err(IngestError::PdfParse(format!(
                "All extraction methods failed: {}",
                e
            ))),
        }
    }

    /// Extract using pdf-extract crate
    fn extract_with_pdf_extract(&self, path: &Path) -> Result<PdfExtraction> {
        let bytes = std::fs::read(path)?;

        let text = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| IngestError::PdfParse(e.to_string()))?;

        // pdf-extract doesn't give us per-page content easily,
        // so we'll split by form feed or estimate by content
        let pages = self.split_into_pages(&text);

        let total_chars = pages.iter().map(|p| p.char_count).sum();
        let page_count = pages.len();

        Ok(PdfExtraction {
            title: String::new(), // Will be filled by caller
            pages,
            total_chars,
            page_count,
        })
    }

    /// Extract using lopdf crate (lower level, more control)
    fn extract_with_lopdf(&self, path: &Path) -> Result<PdfExtraction> {
        let doc =
            lopdf::Document::load(path).map_err(|e| IngestError::PdfParse(e.to_string()))?;

        let mut pages = Vec::new();
        let page_numbers = doc.get_pages();

        for (page_num, _) in page_numbers.iter() {
            let text = doc
                .extract_text(&[*page_num])
                .unwrap_or_default();

            let char_count = text.chars().count();

            // Skip nearly empty pages
            if char_count >= self.min_chars_per_page {
                pages.push(ExtractedPage {
                    page_number: *page_num as usize,
                    text,
                    char_count,
                });
            } else {
                debug!("Skipping page {} with only {} chars", page_num, char_count);
            }
        }

        let total_chars = pages.iter().map(|p| p.char_count).sum();
        let page_count = pages.len();

        Ok(PdfExtraction {
            title: String::new(),
            pages,
            total_chars,
            page_count,
        })
    }

    /// Split text into pages (heuristic-based)
    fn split_into_pages(&self, text: &str) -> Vec<ExtractedPage> {
        // Try splitting by form feed first
        let parts: Vec<&str> = text.split('\x0C').collect();

        if parts.len() > 1 {
            // Form feed split worked
            return parts
                .into_iter()
                .enumerate()
                .filter_map(|(i, page_text)| {
                    let trimmed = page_text.trim();
                    let char_count = trimmed.chars().count();
                    if char_count >= self.min_chars_per_page {
                        Some(ExtractedPage {
                            page_number: i + 1,
                            text: trimmed.to_string(),
                            char_count,
                        })
                    } else {
                        None
                    }
                })
                .collect();
        }

        // No form feeds - treat as single page or split by size
        let char_count = text.chars().count();
        if char_count < 10000 {
            // Small document, single page
            vec![ExtractedPage {
                page_number: 1,
                text: text.to_string(),
                char_count,
            }]
        } else {
            // Large document, split into ~3000 char chunks as "pages"
            self.split_by_size(text, 3000)
        }
    }

    /// Split text by approximate character count
    fn split_by_size(&self, text: &str, target_size: usize) -> Vec<ExtractedPage> {
        let mut pages = Vec::new();
        let mut current = String::new();
        let mut page_num = 1;

        for line in text.lines() {
            current.push_str(line);
            current.push('\n');

            if current.len() >= target_size {
                let char_count = current.chars().count();
                pages.push(ExtractedPage {
                    page_number: page_num,
                    text: std::mem::take(&mut current),
                    char_count,
                });
                page_num += 1;
            }
        }

        // Don't forget the last chunk
        if !current.is_empty() {
            let char_count = current.chars().count();
            pages.push(ExtractedPage {
                page_number: page_num,
                text: current,
                char_count,
            });
        }

        pages
    }

    /// Extract text from raw bytes (for in-memory PDFs)
    pub fn extract_from_bytes(&self, bytes: &[u8], title: &str) -> Result<PdfExtraction> {
        let text = pdf_extract::extract_text_from_mem(bytes)
            .map_err(|e| IngestError::PdfParse(e.to_string()))?;

        let pages = self.split_into_pages(&text);
        let total_chars = pages.iter().map(|p| p.char_count).sum();
        let page_count = pages.len();

        Ok(PdfExtraction {
            title: title.to_string(),
            pages,
            total_chars,
            page_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_by_size() {
        let extractor = PdfExtractor::new();
        let text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n".repeat(100);

        let pages = extractor.split_by_size(&text, 500);

        assert!(pages.len() > 1);
        for page in &pages {
            assert!(page.char_count > 0);
        }
    }

    #[test]
    fn test_split_with_form_feeds() {
        let extractor = PdfExtractor::new();
        let text = "Page 1 content here with enough text to pass the threshold.\x0CPage 2 content here with enough text to pass the threshold.\x0CPage 3 content here with enough text to pass the threshold.";

        let pages = extractor.split_into_pages(text);

        assert_eq!(pages.len(), 3);
        assert!(pages[0].text.contains("Page 1"));
        assert!(pages[1].text.contains("Page 2"));
        assert!(pages[2].text.contains("Page 3"));
    }
}
