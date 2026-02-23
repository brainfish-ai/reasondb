//! API Key model and generation

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::hash_api_key;
use super::permissions::Permissions;

/// Unique identifier for an API key
pub type ApiKeyId = String;

/// API key prefix indicating environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyPrefix {
    /// Production key: `rdb_live_`
    Live,
    /// Test/development key: `rdb_test_`
    Test,
}

impl KeyPrefix {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyPrefix::Live => "rdb_live_",
            KeyPrefix::Test => "rdb_test_",
        }
    }
}

impl std::fmt::Display for KeyPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// API key stored in the database
///
/// Note: We never store the raw key, only its hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique key identifier (e.g., "key_abc123")
    pub id: ApiKeyId,

    /// Human-readable name for the key
    pub name: String,

    /// SHA-256 hash of the API key (never store raw keys!)
    pub key_hash: String,

    /// First 12 characters of the key for identification (e.g., "rdb_live_abc")
    pub key_prefix_hint: String,

    /// Whether this is a live or test key
    pub environment: KeyPrefix,

    /// Permissions granted to this key
    pub permissions: Permissions,

    /// Optional description
    pub description: Option<String>,

    /// Organization or user this key belongs to
    pub owner_id: Option<String>,

    /// Rate limit: requests per minute (None = unlimited)
    pub rate_limit_rpm: Option<u32>,

    /// Rate limit: requests per day (None = unlimited)
    pub rate_limit_rpd: Option<u32>,

    /// Timestamp when this key was created (Unix millis)
    pub created_at: i64,

    /// Timestamp when this key was last used (Unix millis)
    pub last_used_at: Option<i64>,

    /// Timestamp when this key expires (None = never)
    pub expires_at: Option<i64>,

    /// Whether this key is active
    pub is_active: bool,

    /// Number of times this key has been used
    pub usage_count: u64,
}

impl ApiKey {
    /// Generate a new API key
    ///
    /// Returns (ApiKey, raw_key) - the raw key is only available at creation time!
    pub fn generate(
        name: String,
        environment: KeyPrefix,
        permissions: Permissions,
    ) -> (Self, String) {
        let id = format!("key_{}", generate_id(12));
        let raw_key = generate_raw_key(environment);
        let key_hash = hash_api_key(&raw_key);
        let key_prefix_hint = raw_key[..12].to_string();

        let key = Self {
            id,
            name,
            key_hash,
            key_prefix_hint,
            environment,
            permissions,
            description: None,
            owner_id: None,
            rate_limit_rpm: Some(60),    // Default: 60 requests/minute
            rate_limit_rpd: Some(10000), // Default: 10k requests/day
            created_at: chrono::Utc::now().timestamp_millis(),
            last_used_at: None,
            expires_at: None,
            is_active: true,
            usage_count: 0,
        };

        (key, raw_key)
    }

    /// Check if this key has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now().timestamp_millis() > expires_at
        } else {
            false
        }
    }

    /// Check if this key is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        self.is_active && !self.is_expired()
    }

    /// Verify a raw key against this stored key
    pub fn verify(&self, raw_key: &str) -> bool {
        let hash = hash_api_key(raw_key);
        self.key_hash == hash && self.is_valid()
    }

    /// Update last used timestamp
    pub fn mark_used(&mut self) {
        self.last_used_at = Some(chrono::Utc::now().timestamp_millis());
        self.usage_count += 1;
    }
}

/// Metadata returned when listing keys (excludes sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyMetadata {
    pub id: ApiKeyId,
    pub name: String,
    pub key_prefix_hint: String,
    pub environment: KeyPrefix,
    pub permissions: Permissions,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub rate_limit_rpm: Option<u32>,
    pub rate_limit_rpd: Option<u32>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_active: bool,
    pub usage_count: u64,
}

impl From<&ApiKey> for ApiKeyMetadata {
    fn from(key: &ApiKey) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            key_prefix_hint: key.key_prefix_hint.clone(),
            environment: key.environment,
            permissions: key.permissions.clone(),
            description: key.description.clone(),
            owner_id: key.owner_id.clone(),
            rate_limit_rpm: key.rate_limit_rpm,
            rate_limit_rpd: key.rate_limit_rpd,
            created_at: key.created_at,
            last_used_at: key.last_used_at,
            expires_at: key.expires_at,
            is_active: key.is_active,
            usage_count: key.usage_count,
        }
    }
}

/// Generate a random ID of specified length
fn generate_id(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Generate a raw API key with the specified prefix
fn generate_raw_key(prefix: KeyPrefix) -> String {
    // Format: rdb_live_<32 random chars><3 char checksum>
    let random_part = generate_id(32);
    let full = format!("{}{}", prefix.as_str(), random_part);

    // Add simple checksum (last 3 chars based on hash)
    let checksum = &hash_api_key(&full)[..3];
    format!("{}{}", full, checksum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::permissions::Permission;

    #[test]
    fn test_generate_api_key() {
        let perms = Permissions::new(vec![Permission::Read]);
        let (key, raw) = ApiKey::generate("Test Key".into(), KeyPrefix::Test, perms);

        assert!(raw.starts_with("rdb_test_"));
        assert_eq!(raw.len(), 44);
        assert!(key.verify(&raw));
        assert!(key.is_valid());
    }

    #[test]
    fn test_key_verification() {
        let perms = Permissions::new(vec![Permission::Read, Permission::Write]);
        let (key, raw) = ApiKey::generate("My Key".into(), KeyPrefix::Live, perms);

        // Correct key should verify
        assert!(key.verify(&raw));

        // Wrong key should not verify
        assert!(!key.verify("rdb_live_wrongkeywrongkeywrongkey12345abc"));
    }

    #[test]
    fn test_key_expiration() {
        let perms = Permissions::new(vec![Permission::Read]);
        let (mut key, _) = ApiKey::generate("Expired Key".into(), KeyPrefix::Test, perms);

        // Not expired by default
        assert!(!key.is_expired());
        assert!(key.is_valid());

        // Set expiration in the past
        key.expires_at = Some(chrono::Utc::now().timestamp_millis() - 1000);
        assert!(key.is_expired());
        assert!(!key.is_valid());
    }
}
