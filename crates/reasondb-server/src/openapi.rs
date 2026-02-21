//! OpenAPI documentation for ReasonDB API
//!
//! Provides Swagger UI at `/swagger-ui` and OpenAPI spec at `/api-docs/openapi.json`.

use utoipa::OpenApi;

use crate::routes::{documents, ingest, search};

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "ReasonDB API",
        version = "0.1.0",
        description = "A reasoning-native database API for AI agents. \
            ReasonDB uses LLM-guided tree traversal for intelligent document search.",
        license(name = "MIT OR Apache-2.0"),
        contact(
            name = "ReasonDB Contributors",
            url = "https://github.com/reasondb/reasondb"
        )
    ),
    servers(
        (url = "http://localhost:4444", description = "Local development server")
    ),
    tags(
        (name = "ingestion", description = "Document ingestion endpoints"),
        (name = "search", description = "Search and retrieval endpoints"),
        (name = "documents", description = "Document management endpoints"),
        (name = "health", description = "Health check endpoints")
    ),
    paths(
        ingest::ingest_file,
        ingest::ingest_text,
        ingest::ingest_batch,
        ingest::ingest_url,
        search::search,
        documents::list_documents,
        documents::get_document,
        documents::delete_document,
        documents::get_document_nodes,
        documents::get_document_tree,
    ),
    components(
        schemas(
            // Ingestion
            ingest::IngestResponse,
            ingest::IngestStats,
            ingest::IngestTextRequest,
            ingest::BatchIngestItem,
            ingest::BatchIngestRequest,
            ingest::BatchIngestResponse,
            ingest::IngestUrlRequest,
            // Search
            search::SearchRequest,
            search::SearchResponse,
            search::SearchResult,
            search::PathNode,
            search::SearchStats,
            // Documents
            documents::DocumentSummary,
            documents::DocumentDetail,
            documents::NodeSummary,
            documents::TreeNode,
            // Errors
            crate::error::ErrorResponse,
            crate::error::ErrorDetail,
        )
    )
)]
pub struct ApiDoc;
