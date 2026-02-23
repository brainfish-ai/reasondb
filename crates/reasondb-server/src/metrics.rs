//! Metrics and observability for ReasonDB
//!
//! Provides Prometheus metrics at `/metrics` endpoint and OpenTelemetry tracing.
//!
//! ## Metrics Exposed
//!
//! ### HTTP Metrics
//! - `reasondb_http_requests_total` - Total HTTP requests by method, path, status
//! - `reasondb_http_request_duration_seconds` - Request latency histogram
//! - `reasondb_http_requests_in_flight` - Currently processing requests
//!
//! ### Database Metrics
//! - `reasondb_documents_total` - Total documents stored
//! - `reasondb_tables_total` - Total tables
//! - `reasondb_nodes_total` - Total nodes across all documents
//!
//! ### Search Metrics
//! - `reasondb_search_requests_total` - Total search requests
//! - `reasondb_search_duration_seconds` - Search latency
//! - `reasondb_llm_calls_total` - LLM API calls
//! - `reasondb_llm_tokens_total` - Total tokens used
//!
//! ### Cluster Metrics
//! - `reasondb_cluster_nodes` - Number of cluster nodes
//! - `reasondb_cluster_leader` - Current leader (gauge)
//! - `reasondb_cluster_term` - Current Raft term

use axum::{
    body::Body,
    extract::MatchedPath,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;

/// Prometheus metrics handle
static METRICS_HANDLE: std::sync::OnceLock<PrometheusHandle> = std::sync::OnceLock::new();

/// Initialize the Prometheus metrics exporter
pub fn init_metrics() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    // Register default metrics
    register_default_metrics();

    METRICS_HANDLE.get_or_init(|| handle.clone());
    handle
}

/// Get the metrics handle (if initialized)
pub fn get_metrics_handle() -> Option<&'static PrometheusHandle> {
    METRICS_HANDLE.get()
}

/// Register default metrics with initial values
fn register_default_metrics() {
    // Initialize counters
    counter!("reasondb_http_requests_total", "method" => "GET", "path" => "/health", "status" => "200").absolute(0);
    counter!("reasondb_search_requests_total").absolute(0);
    counter!("reasondb_llm_calls_total", "provider" => "unknown").absolute(0);
    counter!("reasondb_llm_tokens_total", "type" => "input").absolute(0);
    counter!("reasondb_llm_tokens_total", "type" => "output").absolute(0);

    // Initialize gauges
    gauge!("reasondb_documents_total").set(0.0);
    gauge!("reasondb_tables_total").set(0.0);
    gauge!("reasondb_nodes_total").set(0.0);
    gauge!("reasondb_http_requests_in_flight").set(0.0);
    gauge!("reasondb_cluster_nodes").set(0.0);
    gauge!("reasondb_cluster_term").set(0.0);
    gauge!("reasondb_cluster_is_leader").set(0.0);
}

/// Middleware to track HTTP request metrics
pub async fn metrics_middleware(request: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    // Track in-flight requests
    gauge!("reasondb_http_requests_in_flight").increment(1.0);

    let response = next.run(request).await;

    // Record metrics
    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    counter!("reasondb_http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status
    )
    .increment(1);

    histogram!("reasondb_http_request_duration_seconds",
        "method" => method,
        "path" => path
    )
    .record(duration);

    gauge!("reasondb_http_requests_in_flight").decrement(1.0);

    response
}

/// Handler for the /metrics endpoint
pub async fn metrics_handler() -> impl IntoResponse {
    match get_metrics_handle() {
        Some(handle) => {
            let metrics = handle.render();
            (
                StatusCode::OK,
                [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
                metrics,
            )
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain")],
            "Metrics not initialized".to_string(),
        ),
    }
}

// =============================================================================
// Metric Recording Functions
// =============================================================================

/// Record a search request
pub fn record_search(duration_secs: f64, results_count: usize) {
    counter!("reasondb_search_requests_total").increment(1);
    histogram!("reasondb_search_duration_seconds").record(duration_secs);
    histogram!("reasondb_search_results_count").record(results_count as f64);
}

/// Record an LLM call
pub fn record_llm_call(provider: &str, duration_secs: f64, input_tokens: u64, output_tokens: u64) {
    counter!("reasondb_llm_calls_total", "provider" => provider.to_string()).increment(1);
    histogram!("reasondb_llm_call_duration_seconds", "provider" => provider.to_string())
        .record(duration_secs);
    counter!("reasondb_llm_tokens_total", "type" => "input").increment(input_tokens);
    counter!("reasondb_llm_tokens_total", "type" => "output").increment(output_tokens);
}

/// Record ingestion metrics
pub fn record_ingestion(doc_type: &str, duration_secs: f64, nodes_created: usize) {
    counter!("reasondb_ingestion_total", "type" => doc_type.to_string()).increment(1);
    histogram!("reasondb_ingestion_duration_seconds", "type" => doc_type.to_string())
        .record(duration_secs);
    counter!("reasondb_nodes_created_total").increment(nodes_created as u64);
}

/// Update database stats
pub fn update_db_stats(documents: u64, tables: u64, nodes: u64) {
    gauge!("reasondb_documents_total").set(documents as f64);
    gauge!("reasondb_tables_total").set(tables as f64);
    gauge!("reasondb_nodes_total").set(nodes as f64);
}

/// Update cluster metrics
pub fn update_cluster_metrics(node_count: usize, term: u64, is_leader: bool) {
    gauge!("reasondb_cluster_nodes").set(node_count as f64);
    gauge!("reasondb_cluster_term").set(term as f64);
    gauge!("reasondb_cluster_is_leader").set(if is_leader { 1.0 } else { 0.0 });
}

/// Record rate limit event
pub fn record_rate_limit(client_id: &str) {
    counter!("reasondb_rate_limit_exceeded_total", "client" => client_id.to_string()).increment(1);
}

/// Record authentication event
pub fn record_auth_event(success: bool) {
    if success {
        counter!("reasondb_auth_success_total").increment(1);
    } else {
        counter!("reasondb_auth_failure_total").increment(1);
    }
}

/// Record cache metrics
pub fn record_cache_hit(cache_type: &str) {
    counter!("reasondb_cache_hits_total", "cache" => cache_type.to_string()).increment(1);
}

pub fn record_cache_miss(cache_type: &str) {
    counter!("reasondb_cache_misses_total", "cache" => cache_type.to_string()).increment(1);
}

// =============================================================================
// OpenTelemetry Setup (requires "telemetry" feature)
// =============================================================================

#[cfg(feature = "telemetry")]
mod otel {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

    /// Initialize OpenTelemetry tracing
    pub fn init_tracing(
        service_name: &str,
        otlp_endpoint: Option<&str>,
        verbose: bool,
        json_logs: bool,
    ) -> anyhow::Result<()> {
        use tracing::Level;
        use tracing_subscriber::EnvFilter;

        let filter = if verbose {
            EnvFilter::from_default_env()
                .add_directive(Level::DEBUG.into())
                .add_directive("hyper=info".parse()?)
                .add_directive("tower_http=debug".parse()?)
        } else {
            EnvFilter::from_default_env()
                .add_directive(Level::INFO.into())
                .add_directive("hyper=warn".parse()?)
        };

        let subscriber = tracing_subscriber::registry().with(filter);

        let fmt_layer = if json_logs {
            tracing_subscriber::fmt::layer().json().boxed()
        } else {
            tracing_subscriber::fmt::layer().pretty().boxed()
        };

        if let Some(endpoint) = otlp_endpoint {
            let tracer = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(
                    opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(endpoint),
                )
                .with_trace_config(
                    sdktrace::Config::default().with_resource(Resource::new(vec![
                        opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                    ])),
                )
                .install_batch(runtime::Tokio)?;

            let otel_layer =
                tracing_opentelemetry::layer().with_tracer(tracer.tracer(service_name.to_string()));

            subscriber.with(fmt_layer).with(otel_layer).init();
        } else {
            subscriber.with(fmt_layer).init();
        }

        Ok(())
    }

    /// Shutdown OpenTelemetry (call on server shutdown)
    pub fn shutdown_tracing() {
        opentelemetry::global::shutdown_tracer_provider();
    }
}

#[cfg(feature = "telemetry")]
pub use otel::{init_tracing, shutdown_tracing};

#[cfg(test)]
mod tests {
    #[test]
    fn test_metrics_initialization() {
        // This would require a separate test binary to avoid conflicts
        // with other tests that might initialize metrics
    }
}
