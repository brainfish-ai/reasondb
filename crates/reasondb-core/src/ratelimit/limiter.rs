//! Token bucket rate limiter implementation
//!
//! Uses a token bucket algorithm for smooth rate limiting with burst support.

use std::time::{Duration, Instant};

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per minute limit
    pub requests_per_minute: u32,
    /// Requests per hour limit (optional, 0 = unlimited)
    pub requests_per_hour: u32,
    /// Burst capacity (tokens available for burst)
    pub burst_size: u32,
    /// Whether rate limiting is enabled
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,    // 1 req/sec average
            requests_per_hour: 1000,    // ~16 req/min average
            burst_size: 10,             // Allow bursts of 10
            enabled: true,
        }
    }
}

impl RateLimitConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        let enabled = std::env::var("REASONDB_RATE_LIMIT_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true); // Enabled by default

        let requests_per_minute = std::env::var("REASONDB_RATE_LIMIT_RPM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let requests_per_hour = std::env::var("REASONDB_RATE_LIMIT_RPH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        let burst_size = std::env::var("REASONDB_RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        Self {
            requests_per_minute,
            requests_per_hour,
            burst_size,
            enabled,
        }
    }

    /// Disabled rate limiting
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Create a permissive tier (higher limits)
    pub fn permissive() -> Self {
        Self {
            requests_per_minute: 300,
            requests_per_hour: 10000,
            burst_size: 50,
            enabled: true,
        }
    }

    /// Create a strict tier (lower limits)
    pub fn strict() -> Self {
        Self {
            requests_per_minute: 20,
            requests_per_hour: 200,
            burst_size: 5,
            enabled: true,
        }
    }
}

/// Rate limit tiers for different use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateLimitTier {
    /// Free tier - most restrictive
    Free,
    /// Standard tier - default limits
    Standard,
    /// Premium tier - higher limits
    Premium,
    /// Unlimited - no rate limiting (admin)
    Unlimited,
}

impl RateLimitTier {
    /// Get the config for this tier
    pub fn config(&self) -> RateLimitConfig {
        match self {
            RateLimitTier::Free => RateLimitConfig {
                requests_per_minute: 20,
                requests_per_hour: 200,
                burst_size: 5,
                enabled: true,
            },
            RateLimitTier::Standard => RateLimitConfig::default(),
            RateLimitTier::Premium => RateLimitConfig {
                requests_per_minute: 300,
                requests_per_hour: 10000,
                burst_size: 50,
                enabled: true,
            },
            RateLimitTier::Unlimited => RateLimitConfig::disabled(),
        }
    }
}

impl Default for RateLimitTier {
    fn default() -> Self {
        RateLimitTier::Standard
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Current limit (requests per minute)
    pub limit: u32,
    /// Remaining requests in current window
    pub remaining: u32,
    /// Seconds until the rate limit resets
    pub reset_after_secs: u64,
    /// Retry after seconds (only set if not allowed)
    pub retry_after_secs: Option<u64>,
}

impl RateLimitResult {
    /// Create an allowed result
    pub fn allowed(limit: u32, remaining: u32, reset_after_secs: u64) -> Self {
        Self {
            allowed: true,
            limit,
            remaining,
            reset_after_secs,
            retry_after_secs: None,
        }
    }

    /// Create a denied result
    pub fn denied(limit: u32, reset_after_secs: u64, retry_after_secs: u64) -> Self {
        Self {
            allowed: false,
            limit,
            remaining: 0,
            reset_after_secs,
            retry_after_secs: Some(retry_after_secs),
        }
    }

    /// Create an unlimited result (no rate limiting)
    pub fn unlimited() -> Self {
        Self {
            allowed: true,
            limit: 0,
            remaining: u32::MAX,
            reset_after_secs: 0,
            retry_after_secs: None,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Available tokens
    tokens: f64,
    /// Maximum tokens (burst capacity)
    max_tokens: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last time tokens were updated
    last_update: Instant,
    /// Total requests in current hour
    hourly_count: u32,
    /// Hour start time
    hour_start: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(max_tokens: u32, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens as f64,
            max_tokens: max_tokens as f64,
            refill_rate,
            last_update: Instant::now(),
            hourly_count: 0,
            hour_start: Instant::now(),
        }
    }

    /// Restore a bucket from a persisted snapshot.
    pub fn from_snapshot(
        tokens: f64,
        max_tokens: f64,
        refill_rate: f64,
        hourly_count: u32,
        last_update: Instant,
        hour_start: Instant,
    ) -> Self {
        Self {
            tokens,
            max_tokens,
            refill_rate,
            last_update,
            hourly_count,
            hour_start,
        }
    }

    /// Get current raw token count (without refilling).
    pub fn tokens_count(&self) -> f64 {
        self.tokens
    }

    /// Get the hour start instant.
    pub fn hour_start_instant(&self) -> Instant {
        self.hour_start
    }

    /// Try to consume a token, returns remaining tokens if successful
    pub fn try_consume(&mut self, hourly_limit: u32) -> Option<u32> {
        self.refill();

        // Check hourly limit
        if hourly_limit > 0 && self.hourly_count >= hourly_limit {
            return None;
        }

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            self.hourly_count += 1;
            Some(self.tokens.floor() as u32)
        } else {
            None
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        let tokens_to_add = elapsed.as_secs_f64() * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.max_tokens);
        self.last_update = now;

        // Reset hourly count if hour has passed
        if now.duration_since(self.hour_start) >= Duration::from_secs(3600) {
            self.hourly_count = 0;
            self.hour_start = now;
        }
    }

    /// Get current token count
    pub fn current_tokens(&mut self) -> u32 {
        self.refill();
        self.tokens.floor() as u32
    }

    /// Seconds until next token is available
    pub fn seconds_until_token(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let tokens_needed = 1.0 - self.tokens;
        (tokens_needed / self.refill_rate).ceil() as u64
    }

    /// Seconds until rate limit resets (minute window)
    pub fn seconds_until_reset(&self) -> u64 {
        let elapsed = self.last_update.elapsed().as_secs();
        60u64.saturating_sub(elapsed % 60)
    }

    /// Seconds until hourly limit resets
    pub fn seconds_until_hour_reset(&self) -> u64 {
        let elapsed = Instant::now().duration_since(self.hour_start).as_secs();
        3600u64.saturating_sub(elapsed)
    }

    /// Get hourly count
    pub fn hourly_count(&self) -> u32 {
        self.hourly_count
    }
}

/// Rate limiter that manages multiple buckets
#[derive(Debug)]
pub struct RateLimiter {
    /// Default configuration
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self { config }
    }

    /// Check if a request should be allowed and update state
    pub fn check(&self, bucket: &mut TokenBucket) -> RateLimitResult {
        if !self.config.enabled {
            return RateLimitResult::unlimited();
        }

        match bucket.try_consume(self.config.requests_per_hour) {
            Some(remaining) => RateLimitResult::allowed(
                self.config.requests_per_minute,
                remaining,
                bucket.seconds_until_reset(),
            ),
            None => {
                // Determine retry time
                let retry_secs = if self.config.requests_per_hour > 0
                    && bucket.hourly_count() >= self.config.requests_per_hour
                {
                    // Hit hourly limit
                    bucket.seconds_until_hour_reset()
                } else {
                    // Hit per-minute limit
                    bucket.seconds_until_token()
                };

                RateLimitResult::denied(
                    self.config.requests_per_minute,
                    bucket.seconds_until_reset(),
                    retry_secs.max(1),
                )
            }
        }
    }

    /// Create a new bucket for a client
    pub fn create_bucket(&self) -> TokenBucket {
        // refill_rate = requests_per_minute / 60 seconds
        let refill_rate = self.config.requests_per_minute as f64 / 60.0;
        TokenBucket::new(self.config.burst_size, refill_rate)
    }

    /// Get the config
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if rate limiting is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_bucket_allows_burst() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 0,
            burst_size: 5,
            enabled: true,
        });

        let mut bucket = limiter.create_bucket();

        // Should allow burst of 5
        for _ in 0..5 {
            let result = limiter.check(&mut bucket);
            assert!(result.allowed);
        }

        // 6th request should be rate limited
        let result = limiter.check(&mut bucket);
        assert!(!result.allowed);
    }

    #[test]
    fn test_bucket_refills() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_minute: 60, // 1 per second
            requests_per_hour: 0,
            burst_size: 1,
            enabled: true,
        });

        let mut bucket = limiter.create_bucket();

        // Use the token
        let result = limiter.check(&mut bucket);
        assert!(result.allowed);

        // Should be rate limited
        let result = limiter.check(&mut bucket);
        assert!(!result.allowed);

        // Wait for refill (slightly more than 1 second for 1 token)
        sleep(Duration::from_millis(1100));

        // Should be allowed now
        let result = limiter.check(&mut bucket);
        assert!(result.allowed);
    }

    #[test]
    fn test_disabled_limiter() {
        let limiter = RateLimiter::new(RateLimitConfig::disabled());
        let mut bucket = limiter.create_bucket();

        // Should always allow when disabled
        for _ in 0..100 {
            let result = limiter.check(&mut bucket);
            assert!(result.allowed);
        }
    }

    #[test]
    fn test_rate_limit_tiers() {
        let free = RateLimitTier::Free.config();
        let standard = RateLimitTier::Standard.config();
        let premium = RateLimitTier::Premium.config();

        assert!(free.requests_per_minute < standard.requests_per_minute);
        assert!(standard.requests_per_minute < premium.requests_per_minute);
    }
}
