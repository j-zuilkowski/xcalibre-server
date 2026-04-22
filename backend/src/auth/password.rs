use crate::{config::AuthSection, AppError};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Algorithm, Argon2, Params, Version,
};

pub fn hash_password(password: &str, auth_config: &AuthSection) -> Result<String, AppError> {
    if password.trim().is_empty() {
        return Err(AppError::BadRequest);
    }
    let params = Params::new(
        auth_config.argon2_memory_kib,
        auth_config.argon2_iterations,
        auth_config.argon2_parallelism,
        None,
    )
    .map_err(|_| AppError::Internal)?;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| AppError::Internal)
        .map(|hash| hash.to_string())
}
