//! In-memory rate limit store
//!
//! Stores token buckets per client identifier (API key or IP).

use super::limiter::{RateLimitConfig, RateLimitResult, RateLimiter, TokenBucket};
use crate::store::rate_limits::RateLimitSnapshot;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Client identifier for rate limiting
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientId {
    /// API key identifier
    ApiKey(String),
    /// IP address (for unauthenticated requests)
    IpAddress(String),
    /// Combined key and IP (for additional security)
    KeyAndIp { key: String, ip: String },
}

impl ClientId {
    /// Create from API key
    pub fn from_key(key: impl Into<String>) -> Self {
        ClientId::ApiKey(key.into())
    }

    /// Create from IP address
    pub fn from_ip(ip: impl Into<String>) -> Self {
        ClientId::IpAddress(ip.into())
    }

    /// Create from both key and IP
    pub fn from_key_and_ip(key: impl Into<String>, ip: impl Into<String>) -> Self {
        ClientId::KeyAndIp {
            key: key.into(),
            ip: ip.into(),
        }
    }
}

/// Entry in the rate limit store
struct RateLimitEntry {
    bucket: TokenBucket,
    last_access: Instant,
    config: RateLimitConfig,
}

/// Thread-safe rate limit store
pub struct RateLimitStore {
    /// Buckets per client
    buckets: RwLock<HashMap<ClientId, RateLimitEntry>>,
    /// Default rate limiter
    default_limiter: RateLimiter,
    /// Custom configs per client
    custom_configs: RwLock<HashMap<ClientId, RateLimitConfig>>,
    /// Cleanup interval
    #[allow(dead_code)]
    cleanup_interval: Duration,
    /// Entry expiry time
    entry_expiry: Duration,
}

impl RateLimitStore {
    /// Create a new rate limit store
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            default_limiter: RateLimiter::new(config),
            custom_configs: RwLock::new(HashMap::new()),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            entry_expiry: Duration::from_secs(3600),    // 1 hour
        }
    }

    /// Check rate limit for a client
    pub fn check(&self, client: &ClientId) -> RateLimitResult {
        if !self.default_limiter.is_enabled() {
            return RateLimitResult::unlimited();
        }

        let mut buckets = self.buckets.write().unwrap();

        // Get or create entry
        let entry = buckets.entry(client.clone()).or_insert_with(|| {
            let config = self.get_config(client);
            let limiter = RateLimiter::new(config.clone());
            RateLimitEntry {
                bucket: limiter.create_bucket(),
                last_access: Instant::now(),
                config,
            }
        });

        entry.last_access = Instant::now();

        // Check with the entry's specific limiter
        let limiter = RateLimiter::new(entry.config.clone());
        limiter.check(&mut entry.bucket)
    }

    /// Get config for a client (custom or default)
    fn get_config(&self, client: &ClientId) -> RateLimitConfig {
        let custom = self.custom_configs.read().unwrap();
        custom
            .get(client)
            .cloned()
            .unwrap_or_else(|| self.default_limiter.config().clone())
    }

    /// Set custom rate limit config for a client
    pub fn set_custom_config(&self, client: ClientId, config: RateLimitConfig) {
        let mut custom = self.custom_configs.write().unwrap();
        custom.insert(client.clone(), config.clone());

        // Update existing bucket if present
        let mut buckets = self.buckets.write().unwrap();
        if let Some(entry) = buckets.get_mut(&client) {
            entry.config = config.clone();
            let limiter = RateLimiter::new(config);
            entry.bucket = limiter.create_bucket();
        }
    }

    /// Remove custom config for a client
    pub fn remove_custom_config(&self, client: &ClientId) {
        let mut custom = self.custom_configs.write().unwrap();
        custom.remove(client);

        // Reset bucket to default config
        let mut buckets = self.buckets.write().unwrap();
        if let Some(entry) = buckets.get_mut(client) {
            entry.config = self.default_limiter.config().clone();
            entry.bucket = self.default_limiter.create_bucket();
        }
    }

    /// Get current status for a client without consuming tokens
    pub fn status(&self, client: &ClientId) -> RateLimitResult {
        let buckets = self.buckets.read().unwrap();

        if let Some(entry) = buckets.get(client) {
            let config = &entry.config;
            // Clone the bucket to check without modifying
            let mut bucket_clone = entry.bucket.clone();
            let remaining = bucket_clone.current_tokens();

            RateLimitResult::allowed(
                config.requests_per_minute,
                remaining,
                bucket_clone.seconds_until_reset(),
            )
        } else {
            // No entry yet, return default limits
            let config = self.default_limiter.config();
            RateLimitResult::allowed(config.requests_per_minute, config.burst_size, 60)
        }
    }

    /// Clean up expired entries
    pub fn cleanup(&self) {
        let mut buckets = self.buckets.write().unwrap();
        let now = Instant::now();

        buckets.retain(|_, entry| now.duration_since(entry.last_access) < self.entry_expiry);
    }

    /// Get the number of tracked clients
    pub fn client_count(&self) -> usize {
        self.buckets.read().unwrap().len()
    }

    /// Get all client IDs
    pub fn clients(&self) -> Vec<ClientId> {
        self.buckets.read().unwrap().keys().cloned().collect()
    }

    /// Check if rate limiting is enabled
    pub fn is_enabled(&self) -> bool {
        self.default_limiter.is_enabled()
    }

    /// Get default config
    pub fn default_config(&self) -> &RateLimitConfig {
        self.default_limiter.config()
    }

    /// Export current state as serializable snapshots for persistence.
    pub fn export_snapshots(&self) -> Vec<(String, RateLimitSnapshot)> {
        let buckets = self.buckets.read().unwrap();
        let now_epoch = chrono::Utc::now().timestamp();

        buckets
            .iter()
            .map(|(client_id, entry)| {
                let key = match client_id {
                    ClientId::ApiKey(k) => format!("key:{}", k),
                    ClientId::IpAddress(ip) => format!("ip:{}", ip),
                    ClientId::KeyAndIp { key, ip } => format!("keyip:{}:{}", key, ip),
                };
                let elapsed_secs = entry.last_access.elapsed().as_secs() as i64;
                let hour_elapsed_secs = Instant::now()
                    .duration_since(entry.bucket.hour_start_instant())
                    .as_secs() as i64;

                let snapshot = RateLimitSnapshot {
                    tokens: entry.bucket.tokens_count(),
                    max_tokens: entry.config.burst_size as f64,
                    refill_rate: entry.config.requests_per_minute as f64 / 60.0,
                    hourly_count: entry.bucket.hourly_count(),
                    last_access_epoch: now_epoch - elapsed_secs,
                    hour_start_epoch: now_epoch - hour_elapsed_secs,
                };
                (key, snapshot)
            })
            .collect()
    }

    /// Restore state from persisted snapshots.
    pub fn import_snapshots(&self, snapshots: &[(String, RateLimitSnapshot)]) {
        let mut buckets = self.buckets.write().unwrap();
        let now = Instant::now();
        let now_epoch = chrono::Utc::now().timestamp();

        for (key, snapshot) in snapshots {
            let client_id = if let Some(k) = key.strip_prefix("key:") {
                ClientId::ApiKey(k.to_string())
            } else if let Some(ip) = key.strip_prefix("ip:") {
                ClientId::IpAddress(ip.to_string())
            } else if let Some(rest) = key.strip_prefix("keyip:") {
                if let Some((k, ip)) = rest.split_once(':') {
                    ClientId::KeyAndIp {
                        key: k.to_string(),
                        ip: ip.to_string(),
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let secs_since_access = (now_epoch - snapshot.last_access_epoch).max(0) as u64;
            let secs_since_hour = (now_epoch - snapshot.hour_start_epoch).max(0) as u64;

            // Skip entries that are too old (> 1 hour)
            if secs_since_access > 3600 {
                continue;
            }

            let config = self.get_config(&client_id);
            let bucket = TokenBucket::from_snapshot(
                snapshot.tokens,
                snapshot.max_tokens,
                snapshot.refill_rate,
                snapshot.hourly_count,
                now.checked_sub(Duration::from_secs(secs_since_access))
                    .unwrap_or(now),
                now.checked_sub(Duration::from_secs(secs_since_hour))
                    .unwrap_or(now),
            );

            buckets.insert(
                client_id,
                RateLimitEntry {
                    bucket,
                    last_access: now.checked_sub(Duration::from_secs(secs_since_access))
                        .unwrap_or(now),
                    config,
                },
            );
        }
    }
}

impl Default for RateLimitStore {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

/// Thread-safe wrapper for RateLimitStore
pub type SharedRateLimitStore = Arc<RateLimitStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_tracks_clients() {
        let store = RateLimitStore::default();
        let client1 = ClientId::from_key("key1");
        let client2 = ClientId::from_ip("192.168.1.1");

        // Access both clients
        store.check(&client1);
        store.check(&client2);

        assert_eq!(store.client_count(), 2);
    }

    #[test]
    fn test_custom_config() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 10,
            requests_per_hour: 0,
            burst_size: 2,
            enabled: true,
        });

        let client = ClientId::from_key("premium-key");

        // Set premium limits
        store.set_custom_config(
            client.clone(),
            RateLimitConfig {
                requests_per_minute: 100,
                requests_per_hour: 0,
                burst_size: 20,
                enabled: true,
            },
        );

        // Should be able to make more requests
        for _ in 0..15 {
            let result = store.check(&client);
            assert!(result.allowed, "Request should be allowed with premium limits");
        }
    }

    #[test]
    fn test_separate_client_limits() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 3,
            enabled: true,
        });

        let client1 = ClientId::from_key("key1");
        let client2 = ClientId::from_key("key2");

        // Exhaust client1's burst
        for _ in 0..3 {
            store.check(&client1);
        }
        let result1 = store.check(&client1);
        assert!(!result1.allowed);

        // client2 should still have full burst
        for _ in 0..3 {
            let result = store.check(&client2);
            assert!(result.allowed);
        }
    }

    #[test]
    fn test_export_snapshots() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        });

        store.check(&ClientId::from_key("api_key_1"));
        store.check(&ClientId::from_ip("192.168.1.1"));

        let snapshots = store.export_snapshots();
        assert_eq!(snapshots.len(), 2);

        let keys: Vec<&str> = snapshots.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"key:api_key_1"));
        assert!(keys.contains(&"ip:192.168.1.1"));

        for (_, snap) in &snapshots {
            assert!(snap.tokens >= 0.0);
            assert!((snap.max_tokens - 10.0).abs() < f64::EPSILON);
            assert!(snap.last_access_epoch > 0);
        }
    }

    #[test]
    fn test_import_snapshots_restores_state() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        });

        // Consume some tokens
        let client = ClientId::from_key("test_key");
        for _ in 0..5 {
            store.check(&client);
        }

        // Export
        let snapshots = store.export_snapshots();
        assert_eq!(snapshots.len(), 1);

        // Import into a fresh store
        let store2 = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        });

        let snapshot_refs: Vec<(String, RateLimitSnapshot)> = snapshots;
        let refs: Vec<_> = snapshot_refs.iter().map(|(k, v)| (k.as_str().to_string(), v.clone())).collect();
        let import_refs: Vec<_> = refs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        store2.import_snapshots(&import_refs);

        assert_eq!(store2.client_count(), 1);

        // The restored client should have ~5 remaining tokens (burst 10 - 5 consumed)
        let status = store2.status(&client);
        assert!(status.remaining <= 5, "Remaining tokens should be <= 5 after restoring state, got {}", status.remaining);
    }

    #[test]
    fn test_import_snapshots_key_formats() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        });

        let now_epoch = chrono::Utc::now().timestamp();
        let snapshots = vec![
            ("key:my_api_key".to_string(), RateLimitSnapshot {
                tokens: 8.0, max_tokens: 10.0, refill_rate: 1.0,
                hourly_count: 2, last_access_epoch: now_epoch, hour_start_epoch: now_epoch,
            }),
            ("ip:10.0.0.1".to_string(), RateLimitSnapshot {
                tokens: 5.0, max_tokens: 10.0, refill_rate: 1.0,
                hourly_count: 5, last_access_epoch: now_epoch, hour_start_epoch: now_epoch,
            }),
            ("keyip:combo_key:10.0.0.2".to_string(), RateLimitSnapshot {
                tokens: 3.0, max_tokens: 10.0, refill_rate: 1.0,
                hourly_count: 7, last_access_epoch: now_epoch, hour_start_epoch: now_epoch,
            }),
        ];

        store.import_snapshots(&snapshots);
        assert_eq!(store.client_count(), 3);
    }

    #[test]
    fn test_import_skips_expired_entries() {
        let store = RateLimitStore::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        });

        let old_epoch = chrono::Utc::now().timestamp() - 7200; // 2 hours ago
        let snapshots = vec![
            ("key:expired_key".to_string(), RateLimitSnapshot {
                tokens: 8.0, max_tokens: 10.0, refill_rate: 1.0,
                hourly_count: 2, last_access_epoch: old_epoch, hour_start_epoch: old_epoch,
            }),
        ];

        store.import_snapshots(&snapshots);
        assert_eq!(store.client_count(), 0, "Expired entries should be skipped");
    }

    #[test]
    fn test_export_import_roundtrip() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 10,
            enabled: true,
        };

        let store1 = RateLimitStore::new(config.clone());

        // Generate some state
        store1.check(&ClientId::from_key("k1"));
        store1.check(&ClientId::from_key("k1"));
        store1.check(&ClientId::from_ip("1.2.3.4"));

        let exported = store1.export_snapshots();
        assert_eq!(exported.len(), 2);

        // Import into fresh store
        let store2 = RateLimitStore::new(config);
        store2.import_snapshots(&exported);
        assert_eq!(store2.client_count(), 2);
    }
}
