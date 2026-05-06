use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random URL-safe token (32 bytes = 64 hex chars).
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

/// SHA-256 hex digest of the raw token — stored in DB, never the raw value.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}
