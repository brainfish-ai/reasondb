//! Document extraction using Microsoft MarkItDown
//!
//! Uses the `markitdown` CLI tool to convert various document formats to Markdown.
//! Supports: PDF, Word, PowerPoint, Excel, Images (OCR), Audio, HTML, CSV, JSON, XML,
//! ZIP files, YouTube URLs, EPubs, and more.
//!
//! ## Prerequisites
//!
//! Install MarkItDown with all dependencies:
//! ```bash
//! pip install 'markitdown[all]'
//! ```
//!
//! See: https://github.com/microsoft/markitdown

use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::error::{IngestError, Result};

/// Supported document types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    /// PDF documents
    Pdf,
    /// Microsoft Word (.docx)
    Word,
    /// Microsoft PowerPoint (.pptx)
    PowerPoint,
    /// Microsoft Excel (.xlsx)
    Excel,
    /// HTML files
    Html,
    /// Plain text
    Text,
    /// CSV files
    Csv,
    /// JSON files
    Json,
    /// XML files
    Xml,
    /// Images (JPEG, PNG, etc.) - uses OCR
    Image,
    /// Audio files (WAV, MP3) - uses transcription
    Audio,
    /// EPUB ebooks
    Epub,
    /// ZIP archives
    Zip,
    /// YouTube URLs
    YouTube,
    /// Outlook messages
    Outlook,
    /// Unknown/Other
    Unknown,
}

impl DocumentType {
    /// Detect document type from file extension
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        
        // Check for YouTube URL
        if let Some(s) = path.to_str() {
            if s.contains("youtube.com") || s.contains("youtu.be") {
                return Self::YouTube;
            }
        }

        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
            Some("pdf") => Self::Pdf,
            Some("docx") | Some("doc") => Self::Word,
            Some("pptx") | Some("ppt") => Self::PowerPoint,
            Some("xlsx") | Some("xls") => Self::Excel,
            Some("html") | Some("htm") => Self::Html,
            Some("txt") | Some("md") | Some("rst") => Self::Text,
            Some("csv") => Self::Csv,
            Some("json") => Self::Json,
            Some("xml") => Self::Xml,
            Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("bmp") | Some("webp") => Self::Image,
            Some("wav") | Some("mp3") | Some("m4a") | Some("ogg") | Some("flac") => Self::Audio,
            Some("epub") => Self::Epub,
            Some("zip") => Self::Zip,
            Some("msg") | Some("eml") => Self::Outlook,
            _ => Self::Unknown,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Word => "Word",
            Self::PowerPoint => "PowerPoint",
            Self::Excel => "Excel",
            Self::Html => "HTML",
            Self::Text => "Text",
            Self::Csv => "CSV",
            Self::Json => "JSON",
            Self::Xml => "XML",
            Self::Image => "Image",
            Self::Audio => "Audio",
            Self::Epub => "EPUB",
            Self::Zip => "ZIP",
            Self::YouTube => "YouTube",
            Self::Outlook => "Outlook",
            Self::Unknown => "Unknown",
        }
    }
}

/// Result of document extraction
#[derive(Debug)]
pub struct ExtractionResult {
    /// Document title (from metadata or filename)
    pub title: String,
    /// Extracted markdown content
    pub markdown: String,
    /// Detected document type
    pub doc_type: DocumentType,
    /// Total character count
    pub char_count: usize,
    /// Source path or URL
    pub source: String,
}

/// Document extractor using MarkItDown
pub struct MarkItDownExtractor {
    /// Path to markitdown executable (default: "markitdown")
    markitdown_path: String,
    /// Enable plugins
    enable_plugins: bool,
    /// Azure Document Intelligence endpoint (for enhanced PDF/image processing)
    doc_intel_endpoint: Option<String>,
}

impl Default for MarkItDownExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkItDownExtractor {
    /// Create a new extractor
    pub fn new() -> Self {
        Self {
            markitdown_path: "markitdown".to_string(),
            enable_plugins: false,
            doc_intel_endpoint: None,
        }
    }

    /// Set custom path to markitdown executable
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.markitdown_path = path.into();
        self
    }

    /// Enable MarkItDown plugins
    pub fn with_plugins(mut self, enabled: bool) -> Self {
        self.enable_plugins = enabled;
        self
    }

    /// Set Azure Document Intelligence endpoint for enhanced processing
    pub fn with_doc_intelligence(mut self, endpoint: impl Into<String>) -> Self {
        self.doc_intel_endpoint = Some(endpoint.into());
        self
    }

    /// Check if MarkItDown is available
    pub fn is_available(&self) -> bool {
        Command::new(&self.markitdown_path)
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Extract content from a file path or URL
    pub fn extract<P: AsRef<Path>>(&self, path: P) -> Result<ExtractionResult> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();
        
        info!("Extracting with MarkItDown: {}", path_str);
        
        let doc_type = DocumentType::from_path(path);
        debug!("Detected document type: {}", doc_type.name());

        // Get title from filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Build command
        let mut cmd = Command::new(&self.markitdown_path);
        cmd.arg(&path_str);

        if self.enable_plugins {
            cmd.arg("--use-plugins");
        }

        if let Some(ref endpoint) = self.doc_intel_endpoint {
            cmd.arg("-d");
            cmd.arg("-e").arg(endpoint);
        }

        // Execute
        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                IngestError::TextExtraction(format!(
                    "MarkItDown not found. Install with: pip install 'markitdown[all]'"
                ))
            } else {
                IngestError::TextExtraction(format!("Failed to run markitdown: {}", e))
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IngestError::TextExtraction(format!(
                "MarkItDown failed: {}",
                stderr
            )));
        }

        let markdown = String::from_utf8_lossy(&output.stdout).to_string();
        let char_count = markdown.chars().count();

        info!(
            "Extracted {} chars from {} ({})",
            char_count,
            title,
            doc_type.name()
        );

        Ok(ExtractionResult {
            title,
            markdown,
            doc_type,
            char_count,
            source: path_str,
        })
    }

    /// Extract from raw bytes (writes to temp file, extracts, cleans up)
    pub fn extract_bytes(&self, bytes: &[u8], filename: &str) -> Result<ExtractionResult> {
        use std::io::Write;
        
        let temp_dir = tempfile::tempdir()
            .map_err(|e| IngestError::TextExtraction(format!("Failed to create temp dir: {}", e)))?;
        
        let temp_path = temp_dir.path().join(filename);
        
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| IngestError::TextExtraction(format!("Failed to create temp file: {}", e)))?;
        
        file.write_all(bytes)
            .map_err(|e| IngestError::TextExtraction(format!("Failed to write temp file: {}", e)))?;
        
        drop(file);
        
        self.extract(&temp_path)
    }

    /// Extract from a URL (YouTube, etc.)
    pub fn extract_url(&self, url: &str) -> Result<ExtractionResult> {
        info!("Extracting from URL: {}", url);
        
        let doc_type = if url.contains("youtube.com") || url.contains("youtu.be") {
            DocumentType::YouTube
        } else {
            DocumentType::Html
        };

        // Build command
        let mut cmd = Command::new(&self.markitdown_path);
        cmd.arg(url);

        if self.enable_plugins {
            cmd.arg("--use-plugins");
        }

        let output = cmd.output().map_err(|e| {
            IngestError::TextExtraction(format!("Failed to run markitdown: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IngestError::TextExtraction(format!(
                "MarkItDown failed: {}",
                stderr
            )));
        }

        let markdown = String::from_utf8_lossy(&output.stdout).to_string();
        let char_count = markdown.chars().count();

        // Try to extract title from markdown
        let title = markdown
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        Ok(ExtractionResult {
            title,
            markdown,
            doc_type,
            char_count,
            source: url.to_string(),
        })
    }
}

/// Fallback extractor using native Rust (when MarkItDown is not available)
pub struct NativeExtractor;

impl NativeExtractor {
    /// Extract PDF using native Rust libraries
    pub fn extract_pdf<P: AsRef<Path>>(path: P) -> Result<ExtractionResult> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        
        let text = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| IngestError::PdfParse(e.to_string()))?;
        
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
        
        let char_count = text.chars().count();
        
        Ok(ExtractionResult {
            title,
            markdown: text, // Plain text, not markdown
            doc_type: DocumentType::Pdf,
            char_count,
            source: path.to_string_lossy().to_string(),
        })
    }

    /// Extract plain text file
    pub fn extract_text<P: AsRef<Path>>(path: P) -> Result<ExtractionResult> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
        
        let char_count = content.chars().count();
        let doc_type = DocumentType::from_path(path);
        
        Ok(ExtractionResult {
            title,
            markdown: content,
            doc_type,
            char_count,
            source: path.to_string_lossy().to_string(),
        })
    }
}

/// Smart extractor that tries MarkItDown first, falls back to native
pub struct SmartExtractor {
    markitdown: MarkItDownExtractor,
    use_markitdown: bool,
}

impl Default for SmartExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartExtractor {
    /// Create a new smart extractor
    pub fn new() -> Self {
        let markitdown = MarkItDownExtractor::new();
        let use_markitdown = markitdown.is_available();
        
        if use_markitdown {
            info!("MarkItDown is available - using for document extraction");
        } else {
            warn!("MarkItDown not found - using native extractors (limited format support)");
            warn!("Install MarkItDown for full format support: pip install 'markitdown[all]'");
        }
        
        Self {
            markitdown,
            use_markitdown,
        }
    }

    /// Configure MarkItDown extractor
    pub fn with_markitdown(mut self, extractor: MarkItDownExtractor) -> Self {
        self.use_markitdown = extractor.is_available();
        self.markitdown = extractor;
        self
    }

    /// Extract from file
    pub fn extract<P: AsRef<Path>>(&self, path: P) -> Result<ExtractionResult> {
        let path = path.as_ref();
        let doc_type = DocumentType::from_path(path);

        // Try MarkItDown first
        if self.use_markitdown {
            match self.markitdown.extract(path) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("MarkItDown failed, trying native extractor: {}", e);
                }
            }
        }

        // Fallback to native extractors
        match doc_type {
            DocumentType::Pdf => NativeExtractor::extract_pdf(path),
            DocumentType::Text | DocumentType::Csv | DocumentType::Json | DocumentType::Xml => {
                NativeExtractor::extract_text(path)
            }
            _ => Err(IngestError::TextExtraction(format!(
                "Unsupported format: {}. Install MarkItDown for full format support: pip install 'markitdown[all]'",
                doc_type.name()
            ))),
        }
    }

    /// Extract from URL
    pub fn extract_url(&self, url: &str) -> Result<ExtractionResult> {
        if self.use_markitdown {
            self.markitdown.extract_url(url)
        } else {
            Err(IngestError::TextExtraction(
                "URL extraction requires MarkItDown. Install with: pip install 'markitdown[all]'".to_string()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_type_detection() {
        assert_eq!(DocumentType::from_path("doc.pdf"), DocumentType::Pdf);
        assert_eq!(DocumentType::from_path("doc.docx"), DocumentType::Word);
        assert_eq!(DocumentType::from_path("doc.pptx"), DocumentType::PowerPoint);
        assert_eq!(DocumentType::from_path("doc.xlsx"), DocumentType::Excel);
        assert_eq!(DocumentType::from_path("doc.html"), DocumentType::Html);
        assert_eq!(DocumentType::from_path("doc.jpg"), DocumentType::Image);
        assert_eq!(DocumentType::from_path("doc.mp3"), DocumentType::Audio);
        assert_eq!(DocumentType::from_path("doc.epub"), DocumentType::Epub);
        assert_eq!(DocumentType::from_path("https://youtube.com/watch?v=123"), DocumentType::YouTube);
    }

    #[test]
    fn test_document_type_case_insensitive() {
        assert_eq!(DocumentType::from_path("doc.PDF"), DocumentType::Pdf);
        assert_eq!(DocumentType::from_path("doc.DOCX"), DocumentType::Word);
        assert_eq!(DocumentType::from_path("doc.JPG"), DocumentType::Image);
    }
}
