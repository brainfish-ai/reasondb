//! Document extraction via plugins
//!
//! All file and URL extraction is handled by the plugin system.
//! Install the built-in `markitdown` plugin (ships in the Docker image) or
//! register your own extractor plugins under `$REASONDB_PLUGINS_DIR`.

use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

use reasondb_plugin::PluginManager;

use crate::error::{IngestError, Result};

/// Supported document types (used for logging / metadata)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    Pdf,
    Word,
    PowerPoint,
    Excel,
    Html,
    Text,
    Csv,
    Json,
    Xml,
    Image,
    Audio,
    Epub,
    Zip,
    YouTube,
    Outlook,
    Unknown,
}

impl DocumentType {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        if let Some(s) = path.to_str() {
            if s.contains("youtube.com") || s.contains("youtu.be") {
                return Self::YouTube;
            }
        }

        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref()
        {
            Some("pdf") => Self::Pdf,
            Some("docx") | Some("doc") => Self::Word,
            Some("pptx") | Some("ppt") => Self::PowerPoint,
            Some("xlsx") | Some("xls") => Self::Excel,
            Some("html") | Some("htm") => Self::Html,
            Some("txt") | Some("md") | Some("rst") => Self::Text,
            Some("csv") => Self::Csv,
            Some("json") => Self::Json,
            Some("xml") => Self::Xml,
            Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("bmp") | Some("webp") => {
                Self::Image
            }
            Some("wav") | Some("mp3") | Some("m4a") | Some("ogg") | Some("flac") => Self::Audio,
            Some("epub") => Self::Epub,
            Some("zip") => Self::Zip,
            Some("msg") | Some("eml") => Self::Outlook,
            _ => Self::Unknown,
        }
    }

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
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub title: String,
    pub markdown: String,
    pub doc_type: DocumentType,
    pub char_count: usize,
    pub source: String,
}

/// Plugin-backed document extractor.
///
/// File and URL extraction is delegated entirely to registered extractor
/// plugins. If no plugin handles the given format/URL, an error is returned
/// telling the user to install an appropriate plugin.
#[derive(Clone)]
pub struct SmartExtractor {
    plugin_manager: Option<Arc<PluginManager>>,
}

impl Default for SmartExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartExtractor {
    pub fn new() -> Self {
        Self {
            plugin_manager: None,
        }
    }

    pub fn with_plugin_manager(mut self, manager: Arc<PluginManager>) -> Self {
        self.plugin_manager = Some(manager);
        self
    }

    /// Extract content from a file via plugins.
    pub fn extract<P: AsRef<Path>>(&self, path: P) -> Result<ExtractionResult> {
        let path = path.as_ref();
        let doc_type = DocumentType::from_path(path);
        let path_str = path.to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if let Some(ref pm) = self.plugin_manager {
            if pm.has_extractor_for_format(ext) {
                match pm.extract_file(&path_str) {
                    Ok(result) => {
                        let char_count = result.markdown.len();
                        info!(
                            "Plugin extracted {} chars from {} ({})",
                            char_count,
                            path_str,
                            doc_type.name()
                        );
                        return Ok(ExtractionResult {
                            title: result.title,
                            markdown: result.markdown,
                            doc_type,
                            char_count,
                            source: path_str,
                        });
                    }
                    Err(e) => {
                        warn!("Plugin extraction failed for {}: {}", path_str, e);
                        return Err(IngestError::TextExtraction(format!(
                            "Plugin extraction failed for '{}': {}",
                            path_str, e
                        )));
                    }
                }
            }
        }

        Err(IngestError::TextExtraction(format!(
            "No extractor plugin registered for '.{}' files. \
             Install the markitdown plugin or add a custom extractor plugin.",
            ext
        )))
    }

    /// Extract content from a URL via plugins.
    pub fn extract_url(&self, url: &str) -> Result<ExtractionResult> {
        if let Some(ref pm) = self.plugin_manager {
            if pm.has_extractor_for_url(url) {
                match pm.extract_url(url) {
                    Ok(result) => {
                        let doc_type = if url.contains("youtube.com") || url.contains("youtu.be") {
                            DocumentType::YouTube
                        } else {
                            DocumentType::Html
                        };
                        let char_count = result.markdown.len();
                        info!("Plugin extracted {} chars from URL: {}", char_count, url);
                        return Ok(ExtractionResult {
                            title: result.title,
                            markdown: result.markdown,
                            doc_type,
                            char_count,
                            source: url.to_string(),
                        });
                    }
                    Err(e) => {
                        warn!("Plugin URL extraction failed for {}: {}", url, e);
                        return Err(IngestError::TextExtraction(format!(
                            "Plugin URL extraction failed for '{}': {}",
                            url, e
                        )));
                    }
                }
            }
        }

        Err(IngestError::TextExtraction(
            "No extractor plugin registered for this URL. \
             Install the markitdown plugin or add a custom extractor plugin."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_type_detection() {
        assert_eq!(DocumentType::from_path("doc.pdf"), DocumentType::Pdf);
        assert_eq!(DocumentType::from_path("doc.docx"), DocumentType::Word);
        assert_eq!(
            DocumentType::from_path("doc.pptx"),
            DocumentType::PowerPoint
        );
        assert_eq!(DocumentType::from_path("doc.xlsx"), DocumentType::Excel);
        assert_eq!(DocumentType::from_path("doc.html"), DocumentType::Html);
        assert_eq!(DocumentType::from_path("doc.jpg"), DocumentType::Image);
        assert_eq!(DocumentType::from_path("doc.mp3"), DocumentType::Audio);
        assert_eq!(DocumentType::from_path("doc.epub"), DocumentType::Epub);
        assert_eq!(
            DocumentType::from_path("https://youtube.com/watch?v=123"),
            DocumentType::YouTube
        );
    }

    #[test]
    fn test_document_type_case_insensitive() {
        assert_eq!(DocumentType::from_path("doc.PDF"), DocumentType::Pdf);
        assert_eq!(DocumentType::from_path("doc.DOCX"), DocumentType::Word);
        assert_eq!(DocumentType::from_path("doc.JPG"), DocumentType::Image);
    }

    #[test]
    fn test_no_plugin_returns_error() {
        let extractor = SmartExtractor::new();
        let result = extractor.extract("test.pdf");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No extractor plugin"));
    }

    #[test]
    fn test_no_plugin_url_returns_error() {
        let extractor = SmartExtractor::new();
        let result = extractor.extract_url("https://example.com");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No extractor plugin"));
    }
}
