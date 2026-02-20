//! Rate limiting middleware for axum
//!
//! Provides rate limiting per API key or IP address.

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use reasondb_core::ratelimit::{ClientId, RateLimitResult, RateLimitStore};
use std::net::SocketAddr;
use std::sync::Arc;

/// Rate limit headers
pub mod headers {
    pub const X_RATELIMIT_LIMIT: &str = "X-RateLimit-Limit";
    pub const X_RATELIMIT_REMAINING: &str = "X-RateLimit-Remaining";
    pub const X_RATELIMIT_RESET: &str = "X-RateLimit-Reset";
    pub const RETRY_AFTER: &str = "Retry-After";
}

/// Rate limit error response
#[derive(Debug)]
pub struct RateLimitError {
    pub result: RateLimitResult,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        let retry_after = self.result.retry_after_secs.unwrap_or(60);

        let body = serde_json::json!({
            "error": {
                "code": "RATE_LIMITED",
                "message": format!(
                    "Rate limit exceeded. Try again in {} seconds.",
                    retry_after
                ),
                "retry_after": retry_after,
                "limit": self.result.limit,
            }
        });

        let mut response = (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();

        // Add rate limit headers
        let headers = response.headers_mut();
        headers.insert(
            headers::X_RATELIMIT_LIMIT,
            HeaderValue::from_str(&self.result.limit.to_string()).unwrap(),
        );
        headers.insert(
            headers::X_RATELIMIT_REMAINING,
            HeaderValue::from_str("0").unwrap(),
        );
        headers.insert(
            headers::X_RATELIMIT_RESET,
            HeaderValue::from_str(&self.result.reset_after_secs.to_string()).unwrap(),
        );
        headers.insert(
            headers::RETRY_AFTER,
            HeaderValue::from_str(&retry_after.to_string()).unwrap(),
        );

        response
    }
}

/// Add rate limit headers to a response
pub fn add_rate_limit_headers(headers: &mut HeaderMap, result: &RateLimitResult) {
    if result.limit > 0 {
        if let Ok(v) = HeaderValue::from_str(&result.limit.to_string()) {
            headers.insert(headers::X_RATELIMIT_LIMIT, v);
        }
        if let Ok(v) = HeaderValue::from_str(&result.remaining.to_string()) {
            headers.insert(headers::X_RATELIMIT_REMAINING, v);
        }
        if let Ok(v) = HeaderValue::from_str(&result.reset_after_secs.to_string()) {
            headers.insert(headers::X_RATELIMIT_RESET, v);
        }
    }
}

/// Extract client identifier from request
pub fn extract_client_id(headers: &HeaderMap, addr: Option<SocketAddr>) -> ClientId {
    // Try to get API key from headers
    let api_key = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
        });

    // Get IP address
    let ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("X-Real-IP")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    match api_key {
        Some(key) => ClientId::from_key(key),
        None => ClientId::from_ip(ip),
    }
}

/// Paths exempt from rate limiting (polling and health endpoints)
fn is_rate_limit_exempt(path: &str) -> bool {
    path.starts_with("/v1/jobs") || path == "/health" || path == "/metrics"
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(store): State<Arc<RateLimitStore>>,
    request: Request,
    next: Next,
) -> Response {
    // Skip rate limiting if disabled
    if !store.is_enabled() {
        return next.run(request).await;
    }

    // Skip rate limiting for exempt endpoints
    if is_rate_limit_exempt(request.uri().path()) {
        return next.run(request).await;
    }

    // Extract client identifier
    let headers = request.headers();
    let addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let client_id = extract_client_id(headers, addr);

    // Check rate limit
    let result = store.check(&client_id);

    if !result.allowed {
        return RateLimitError { result }.into_response();
    }

    // Continue with request
    let mut response = next.run(request).await;

    // Add rate limit headers to response
    add_rate_limit_headers(response.headers_mut(), &result);

    response
}


#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_client_id_from_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer rdb_live_test123"),
        );

        let client_id = extract_client_id(&headers, None);
        assert!(matches!(client_id, ClientId::ApiKey(k) if k == "rdb_live_test123"));
    }

    #[test]
    fn test_extract_client_id_from_x_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", HeaderValue::from_static("rdb_test_abc"));

        let client_id = extract_client_id(&headers, None);
        assert!(matches!(client_id, ClientId::ApiKey(k) if k == "rdb_test_abc"));
    }

    #[test]
    fn test_extract_client_id_from_ip() {
        let headers = HeaderMap::new();
        let addr: SocketAddr = "192.168.1.100:12345".parse().unwrap();

        let client_id = extract_client_id(&headers, Some(addr));
        assert!(matches!(client_id, ClientId::IpAddress(ip) if ip == "192.168.1.100"));
    }

    #[test]
    fn test_extract_client_id_from_forwarded_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Forwarded-For",
            HeaderValue::from_static("10.0.0.1, 192.168.1.1"),
        );

        let client_id = extract_client_id(&headers, None);
        assert!(matches!(client_id, ClientId::IpAddress(ip) if ip == "10.0.0.1"));
    }

    #[test]
    fn test_rate_limit_exempt_jobs_path() {
        assert!(is_rate_limit_exempt("/v1/jobs"));
        assert!(is_rate_limit_exempt("/v1/jobs/job_123"));
        assert!(is_rate_limit_exempt("/v1/jobs/job_abc/status"));
    }

    #[test]
    fn test_rate_limit_exempt_health_and_metrics() {
        assert!(is_rate_limit_exempt("/health"));
        assert!(is_rate_limit_exempt("/metrics"));
    }

    #[test]
    fn test_rate_limit_not_exempt_api_paths() {
        assert!(!is_rate_limit_exempt("/v1/tables"));
        assert!(!is_rate_limit_exempt("/v1/documents"));
        assert!(!is_rate_limit_exempt("/v1/ingest/text"));
        assert!(!is_rate_limit_exempt("/v1/search"));
        assert!(!is_rate_limit_exempt("/"));
    }
}
