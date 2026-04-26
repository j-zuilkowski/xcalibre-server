//! TOTP (Time-based One-Time Password) support and symmetric secret encryption.
//!
//! # Key derivation
//! AES-256-GCM encryption keys are **never stored directly**.  Instead, a key is
//! derived on demand from `jwt_secret` using HKDF-SHA256:
//! - TOTP secrets: salt = `b"xcalibre-server-totp-v1"`, label = `b"totp-encryption-key"`.
//! - Webhook secrets: salt = `b"xcalibre-server-webhook-v1"`, label = `b"webhook-encryption-key"`.
//!
//! Separate salts and labels ensure that the derived keys for TOTP and webhook
//! secrets are cryptographically independent even though they share the same IKM.
//!
//! # `totp_secret` storage invariant
//! The raw TOTP base32 secret is **never stored in plaintext**.  Only the
//! AES-256-GCM ciphertext (nonce || ciphertext, base64-encoded) is persisted in
//! the database.  Decryption happens in memory at verification time.
//!
//! # Key rotation deployment note
//! IMPORTANT: If `jwt_secret` changes in a running deployment that has TOTP or
//! webhook secrets stored, all encrypted secrets become unreadable.  Before rotating
//! the JWT secret, run the key rotation migration documented in docs/DEPLOY.md.
//! (See the TODO comment on the constants below for the runtime reminder.)
//!
//! # Backup codes
//! [`generate_backup_code`] uses [`OsRng`] (system CSPRNG) to generate 8-character
//! alphanumeric codes.  `OsRng` is used for consistency with other security-critical
//! random generation in the codebase (Phase 17 Stage 14).

use crate::AppError;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use hkdf::Hkdf;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng, RngCore};
use sha2::Sha256;
use totp_rs::{Algorithm, Secret, TOTP};
use urlencoding::encode;

pub const TOTP_HKDF_SALT: &[u8] = b"xcalibre-server-totp-v1";
pub const WEBHOOK_HKDF_SALT: &[u8] = b"xcalibre-server-webhook-v1";

// TODO: If upgrading an existing deployment with TOTP or webhook data,
// run the key rotation migration before deploying this change.
// See docs/DEPLOY.md — Key Rotation section.
const TOTP_LABEL: &[u8] = b"totp-encryption-key";
const WEBHOOK_LABEL: &[u8] = b"webhook-encryption-key";
const TOTP_STEP_SECONDS: u64 = 30;
const TOTP_DIGITS: usize = 6;
const TOTP_SKEW: u8 = 1;
const TOTP_NONCE_BYTES: usize = 12;

/// Generate a new random TOTP base32 secret using the `totp-rs` crate's CSPRNG.
pub fn generate_secret_base32() -> String {
    Secret::generate_secret().to_encoded().to_string()
}

/// Build an `otpauth://totp/...` URI for QR code generation in authenticator apps.
///
/// Encodes issuer and account name with percent-encoding.  Parameters match the
/// TOTP configuration: SHA1, 6 digits, 30-second period.
pub fn build_otpauth_uri(issuer: &str, account_name: &str, secret_base32: &str) -> String {
    let issuer = encode(issuer);
    let account_name = encode(account_name);
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        issuer, account_name, secret_base32, issuer
    )
}

pub fn build_totp(issuer: &str, account_name: &str, secret_base32: &str) -> Result<TOTP, AppError> {
    let secret = Secret::Encoded(secret_base32.to_string());
    let secret_bytes = secret.to_bytes().map_err(|_| AppError::Internal)?;
    TOTP::new(
        Algorithm::SHA1,
        TOTP_DIGITS,
        TOTP_SKEW,
        TOTP_STEP_SECONDS,
        secret_bytes,
        Some(issuer.to_string()),
        account_name.to_string(),
    )
    .map_err(|_| AppError::Internal)
}

/// Verify a 6-digit TOTP code against the stored base32 secret.
///
/// Allows 1-step skew (±30 seconds) to tolerate mild clock drift between
/// the server and the user's authenticator device.
///
/// # Errors
/// Returns `AppError::Internal` if the TOTP object cannot be constructed (invalid
/// secret bytes).  Returns `Ok(false)` for incorrect codes rather than an error.
pub fn validate_code(
    issuer: &str,
    account_name: &str,
    secret_base32: &str,
    code: &str,
) -> Result<bool, AppError> {
    let totp = build_totp(issuer, account_name, secret_base32)?;
    totp.check_current(code).map_err(|_| AppError::Internal)
}

/// Encrypt a TOTP base32 secret with AES-256-GCM.
///
/// Key is derived from `jwt_secret` using HKDF with `TOTP_HKDF_SALT`.
/// Returns a base64-encoded blob of `nonce || ciphertext`.
pub fn encrypt_secret(plaintext: &str, jwt_secret: &str) -> Result<String, AppError> {
    let key = derive_key(jwt_secret, TOTP_HKDF_SALT)?;
    encrypt_with_key(plaintext, &key)
}

/// Encrypt a webhook HMAC secret with AES-256-GCM.
///
/// Uses a separate HKDF salt (`WEBHOOK_HKDF_SALT`) from TOTP encryption so the
/// two purposes produce independent keys even with the same `jwt_secret`.
pub fn encrypt_webhook_secret(plaintext: &str, jwt_secret: &str) -> Result<String, AppError> {
    let key = derive_webhook_key(jwt_secret, WEBHOOK_HKDF_SALT)?;
    encrypt_with_key(plaintext, &key)
}

/// Decrypt a TOTP secret previously encrypted with [`encrypt_secret`].
///
/// # Errors
/// Returns `AppError::Internal` on base64 decode failure, ciphertext too short,
/// AES-GCM authentication failure (tampered ciphertext), or invalid UTF-8 plaintext.
pub fn decrypt_secret(ciphertext_b64: &str, jwt_secret: &str) -> Result<String, AppError> {
    let key = derive_key(jwt_secret, TOTP_HKDF_SALT)?;
    decrypt_with_key(ciphertext_b64, &key)
}

/// Decrypt a webhook secret previously encrypted with [`encrypt_webhook_secret`].
///
/// # Errors
/// Same error conditions as [`decrypt_secret`].
pub fn decrypt_webhook_secret(ciphertext_b64: &str, jwt_secret: &str) -> Result<String, AppError> {
    let key = derive_webhook_key(jwt_secret, WEBHOOK_HKDF_SALT)?;
    decrypt_with_key(ciphertext_b64, &key)
}

fn encrypt_with_key(plaintext: &str, key: &[u8; 32]) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| AppError::Internal)?;
    let mut nonce_bytes = [0_u8; TOTP_NONCE_BYTES];
    // OsRng provides a cryptographically secure 96-bit random nonce.
    // A fresh nonce is generated for every encryption call — never reused.
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| AppError::Internal)?;
    let mut payload = Vec::with_capacity(TOTP_NONCE_BYTES + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(payload))
}

fn decrypt_with_key(ciphertext_b64: &str, key: &[u8; 32]) -> Result<String, AppError> {
    let payload = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64.as_bytes())
        .map_err(|_| AppError::Internal)?;
    if payload.len() <= TOTP_NONCE_BYTES {
        return Err(AppError::Internal);
    }

    let (nonce_bytes, ciphertext) = payload.split_at(TOTP_NONCE_BYTES);
    let cipher = Aes256Gcm::new_from_slice(&key[..]).map_err(|_| AppError::Internal)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Internal)?;
    String::from_utf8(plaintext).map_err(|_| AppError::Internal)
}

/// Generate a single 8-character alphanumeric TOTP backup code using `OsRng`.
///
/// Uses [`OsRng`] (system CSPRNG) rather than a thread-local RNG to ensure
/// security-grade entropy even in newly spawned threads (Phase 17 Stage 14).
pub fn generate_backup_code() -> String {
    let rng = OsRng;
    rng
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

/// Derive a 32-byte AES key from `jwt_secret` using HKDF-SHA256 with the TOTP salt and label.
///
/// Exposed for use by migration tooling that needs to re-encrypt secrets.
pub fn derive_key(jwt_secret: &str, salt: &[u8]) -> Result<[u8; 32], AppError> {
    derive_key_with_label(jwt_secret, salt, TOTP_LABEL)
}

fn derive_webhook_key(jwt_secret: &str, salt: &[u8]) -> Result<[u8; 32], AppError> {
    derive_key_with_label(jwt_secret, salt, WEBHOOK_LABEL)
}

fn derive_key_with_label(
    jwt_secret: &str,
    salt: &[u8],
    label: &[u8],
) -> Result<[u8; 32], AppError> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), jwt_secret.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(label, &mut key)
        .map_err(|_| AppError::Internal)?;
    Ok(key)
}
