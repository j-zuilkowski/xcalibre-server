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

const TOTP_LABEL: &[u8] = b"totp-encryption-key";
const TOTP_STEP_SECONDS: u64 = 30;
const TOTP_DIGITS: usize = 6;
const TOTP_SKEW: u8 = 1;
const TOTP_NONCE_BYTES: usize = 12;

pub fn generate_secret_base32() -> String {
    Secret::generate_secret().to_encoded().to_string()
}

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

pub fn validate_code(
    issuer: &str,
    account_name: &str,
    secret_base32: &str,
    code: &str,
) -> Result<bool, AppError> {
    let totp = build_totp(issuer, account_name, secret_base32)?;
    totp.check_current(code).map_err(|_| AppError::Internal)
}

pub fn encrypt_secret(plaintext: &str, jwt_secret: &str) -> Result<String, AppError> {
    let key = derive_key(jwt_secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| AppError::Internal)?;
    let mut nonce_bytes = [0_u8; TOTP_NONCE_BYTES];
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

pub fn decrypt_secret(ciphertext_b64: &str, jwt_secret: &str) -> Result<String, AppError> {
    let payload = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64.as_bytes())
        .map_err(|_| AppError::Internal)?;
    if payload.len() <= TOTP_NONCE_BYTES {
        return Err(AppError::Internal);
    }

    let (nonce_bytes, ciphertext) = payload.split_at(TOTP_NONCE_BYTES);
    let key = derive_key(jwt_secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| AppError::Internal)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Internal)?;
    String::from_utf8(plaintext).map_err(|_| AppError::Internal)
}

pub fn generate_backup_code() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

fn derive_key(jwt_secret: &str) -> Result<[u8; 32], AppError> {
    let hkdf = Hkdf::<Sha256>::new(None, jwt_secret.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(TOTP_LABEL, &mut key)
        .map_err(|_| AppError::Internal)?;
    Ok(key)
}
