//! Application configuration: TOML deserialization, env var overrides, and startup validation.
//!
//! # Config file
//! Loaded from `config.toml` by default, or the path in the `CONFIG_PATH` env var.
//! If the file does not exist, defaults are used.  File permissions are checked at
//! startup on Unix; world-readable configs log a warning.
//!
//! # Env var overrides
//! Every config field has a corresponding env var (see [`apply_env_overrides`]).
//! Two sets of names are supported: `APP_*` (canonical) and legacy bare names
//! (e.g. `BASE_URL`, `DATABASE_URL`, `JWT_SECRET`, `ENABLE_LLM_FEATURES`).
//!
//! # JWT secret auto-generation
//! If `auth.jwt_secret` is blank at startup, a fresh 32-byte random secret is
//! generated, base64-encoded, written back to `config.toml`, and logged as a warning.
//! The generated secret is stable across restarts (it is persisted to disk).
//!
//! # LLM endpoint SSRF protection
//! LLM endpoints are validated against private/loopback ranges **at startup** via
//! [`validate_llm_endpoints`]. Private endpoints log a warning and do not block
//! startup so home NAS deployments can intentionally point at LAN-hosted model
//! servers. Runtime callers can still use [`validate_llm_endpoint`] for strict
//! rejection during config edits.
//!
//! # Security invariants enforced at startup
//! - `jwt_secret` must decode to ≥ 32 bytes.
//! - `argon2_memory_kib` ≥ 65,536; `argon2_iterations` ≥ 3; `argon2_parallelism` ≥ 4.
//! - `ldap.uid_attr` and `ldap.email_attr` required when LDAP is enabled.
//! - HTTP `base_url` + `https_only = false` logs a security warning.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    net::ToSocketAddrs,
    path::{Path, PathBuf},
};

const MIN_JWT_SECRET_BYTES: usize = 32;
const MIN_ARGON2_MEMORY_KIB: u32 = 65_536;
const MIN_ARGON2_ITERATIONS: u32 = 3;
const MIN_ARGON2_PARALLELISM: u32 = 4;

/// Top-level application configuration deserialized from `config.toml`.
///
/// All sections implement `Default`; the default config is valid for local development
/// but production deployments must set at minimum `database.url`, `app.storage_path`,
/// and `auth.jwt_secret` (or allow auto-generation on first run).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub app: AppSection,
    pub server: ServerSection,
    pub storage: StorageSection,
    pub database: DatabaseSection,
    pub auth: AuthSection,
    pub oauth: OauthSection,
    pub ldap: LdapSection,
    pub metadata: MetadataLookupSection,
    pub meilisearch: MeilisearchSection,
    pub llm: LlmSection,
    pub limits: LimitsSection,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSection {
    pub library_name: String,
    pub base_url: String,
    pub storage_path: String,
    pub calibre_db_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSection {
    pub https_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageSection {
    pub backend: String,
    pub s3: S3Section,
}

impl Default for StorageSection {
    fn default() -> Self {
        Self {
            backend: "local".to_string(),
            s3: S3Section::default(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3Section {
    pub bucket: String,
    pub region: String,
    pub endpoint_url: String,
    pub access_key: String,
    pub secret_key: String,
    pub key_prefix: String,
}

impl Default for S3Section {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            region: "us-east-1".to_string(),
            endpoint_url: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            key_prefix: String::new(),
        }
    }
}

impl std::fmt::Debug for S3Section {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Section")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint_url", &self.endpoint_url)
            .field("access_key", &self.access_key)
            .field("secret_key", &"[REDACTED]")
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseSection {
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthSection {
    pub jwt_secret: String,
    pub access_token_ttl_mins: u64,
    pub refresh_token_ttl_days: u64,
    pub max_login_attempts: u32,
    pub lockout_duration_mins: u64,
    pub argon2_memory_kib: u32,
    pub argon2_iterations: u32,
    pub argon2_parallelism: u32,
    pub proxy: ProxyAuthSection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyAuthSection {
    pub enabled: bool,
    pub header: String,
    pub email_header: String,
    pub trusted_cidrs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OauthSection {
    pub google: OauthProviderSection,
    pub github: OauthProviderSection,
}

impl Default for OauthSection {
    fn default() -> Self {
        Self {
            google: OauthProviderSection {
                authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".to_string(),
                email_url: String::new(),
                scope: "openid email profile".to_string(),
                ..OauthProviderSection::default()
            },
            github: OauthProviderSection {
                authorization_url: "https://github.com/login/oauth/authorize".to_string(),
                token_url: "https://github.com/login/oauth/access_token".to_string(),
                userinfo_url: "https://api.github.com/user".to_string(),
                email_url: "https://api.github.com/user/emails".to_string(),
                scope: "read:user user:email".to_string(),
                ..OauthProviderSection::default()
            },
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OauthProviderSection {
    pub client_id: String,
    pub client_secret: String,
    pub authorization_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub email_url: String,
    pub scope: String,
}

impl std::fmt::Debug for OauthProviderSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OauthProviderSection")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("authorization_url", &self.authorization_url)
            .field("token_url", &self.token_url)
            .field("userinfo_url", &self.userinfo_url)
            .field("email_url", &self.email_url)
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LdapSection {
    pub enabled: bool,
    pub url: String,
    pub bind_dn: String,
    pub bind_pw: String,
    pub search_base: String,
    pub uid_attr: String,
    pub email_attr: String,
}

impl std::fmt::Debug for LdapSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LdapSection")
            .field("enabled", &self.enabled)
            .field("url", &self.url)
            .field("bind_dn", &self.bind_dn)
            .field("bind_pw", &"[REDACTED]")
            .field("search_base", &self.search_base)
            .field("uid_attr", &self.uid_attr)
            .field("email_attr", &self.email_attr)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataLookupSection {
    pub openlibrary_base_url: String,
    pub googlebooks_base_url: String,
}

impl Default for MetadataLookupSection {
    fn default() -> Self {
        Self {
            openlibrary_base_url: "https://openlibrary.org".to_string(),
            googlebooks_base_url: "https://www.googleapis.com".to_string(),
        }
    }
}

impl Default for AuthSection {
    fn default() -> Self {
        Self {
            jwt_secret: String::new(),
            access_token_ttl_mins: 15,
            refresh_token_ttl_days: 30,
            max_login_attempts: 10,
            lockout_duration_mins: 15,
            argon2_memory_kib: MIN_ARGON2_MEMORY_KIB,
            argon2_iterations: MIN_ARGON2_ITERATIONS,
            argon2_parallelism: MIN_ARGON2_PARALLELISM,
            proxy: ProxyAuthSection::default(),
        }
    }
}

impl Default for ProxyAuthSection {
    fn default() -> Self {
        Self {
            enabled: false,
            header: "x-remote-user".to_string(),
            email_header: "X-Remote-Email".to_string(),
            trusted_cidrs: vec!["127.0.0.1/32".to_string(), "::1/128".to_string()],
        }
    }
}

impl std::fmt::Debug for AuthSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthSection")
            .field("jwt_secret", &"[REDACTED]")
            .field("access_token_ttl_mins", &self.access_token_ttl_mins)
            .field("refresh_token_ttl_days", &self.refresh_token_ttl_days)
            .field("max_login_attempts", &self.max_login_attempts)
            .field("lockout_duration_mins", &self.lockout_duration_mins)
            .field("argon2_memory_kib", &self.argon2_memory_kib)
            .field("argon2_iterations", &self.argon2_iterations)
            .field("argon2_parallelism", &self.argon2_parallelism)
            .field("proxy", &self.proxy)
            .finish()
    }
}

/// LLM feature configuration.
///
/// `enabled` defaults to `false`; opt-in via `ENABLE_LLM_FEATURES=true`.
/// `allow_private_endpoints` must be `true` to use LM Studio / Ollama on localhost.
/// Default is `false` to block SSRF via RFC 1918 / loopback addresses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSection {
    pub enabled: bool,
    /// Set to true to allow LLM endpoints on private/loopback addresses.
    /// Required for local model servers (LM Studio, Ollama, etc.).
    /// Default: false (rejects RFC 1918, loopback, link-local).
    pub allow_private_endpoints: bool,
    pub librarian: LlmRoleSection,
    pub architect: LlmRoleSection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MeilisearchSection {
    pub enabled: bool,
    pub url: String,
    pub api_key: String,
}

impl Default for MeilisearchSection {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "http://meilisearch:7700".to_string(),
            api_key: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmRoleSection {
    pub endpoint: String,
    pub model: String,
    pub timeout_secs: u64,
    pub system_prompt: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsSection {
    pub upload_max_bytes: u64,
    pub rate_limit_per_ip: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app: AppSection {
                library_name: String::new(),
                base_url: "http://localhost:8083".to_string(),
                storage_path: "./storage".to_string(),
                calibre_db_path: "./calibre/metadata.db".to_string(),
            },
            server: ServerSection::default(),
            storage: StorageSection::default(),
            database: DatabaseSection {
                url: "sqlite://library.db".to_string(),
            },
            auth: AuthSection::default(),
            oauth: OauthSection::default(),
            ldap: LdapSection::default(),
            metadata: MetadataLookupSection::default(),
            meilisearch: MeilisearchSection::default(),
            llm: LlmSection::default(),
            limits: LimitsSection {
                upload_max_bytes: 524_288_000,
                rate_limit_per_ip: 200,
            },
        }
    }
}

/// Load and validate the application configuration.
///
/// 1. Reads `config.toml` (or `CONFIG_PATH` env var) if it exists; falls back to defaults.
/// 2. Checks Unix file permissions (world-readable = warning).
/// 3. Applies env var overrides via [`apply_env_overrides`].
/// 4. Validates required fields and security minimums.
/// 5. Auto-generates and persists `jwt_secret` if blank.
/// 6. Validates LLM endpoints for SSRF (private/loopback host check).
///
/// # Errors
/// Returns an error for missing required fields, sub-minimum argon2 parameters,
/// invalid base64 or too-short JWT secret, unsupported storage backend, or
/// LLM endpoint pointing at a private address without `allow_private_endpoints`.
pub async fn load_config() -> anyhow::Result<AppConfig> {
    let path = config_path();
    if path.exists() {
        warn_if_world_readable(&path)?;
    }

    let mut config = if path.exists() {
        let contents = fs::read_to_string(&path)?;
        toml::from_str::<AppConfig>(&contents)?
    } else {
        AppConfig::default()
    };

    apply_env_overrides(&mut config);
    validate_required_fields(&config)?;

    if config.auth.jwt_secret.trim().is_empty() {
        config.auth.jwt_secret = generate_jwt_secret();
        tracing::warn!(
            path = %path.display(),
            "jwt_secret was blank; generated a new secret and wrote it back"
        );
        write_config(&path, &config)?;
    }

    validate_jwt_secret(&config.auth.jwt_secret)?;
    validate_llm_endpoints(&config)?;

    Ok(config)
}

fn config_path() -> PathBuf {
    std::env::var_os("CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

fn validate_required_fields(config: &AppConfig) -> anyhow::Result<()> {
    if config.database.url.trim().is_empty() {
        anyhow::bail!("database.url is required");
    }
    if config.app.storage_path.trim().is_empty() {
        anyhow::bail!("app.storage_path is required");
    }
    if config.app.base_url.trim().is_empty() {
        anyhow::bail!("app.base_url is required");
    }

    let storage_backend = config.storage.backend.trim().to_ascii_lowercase();
    match storage_backend.as_str() {
        "local" => {
            tracing::info!(
                path = %config.app.storage_path,
                "Storage backend: local filesystem"
            );
        }
        "s3" => {
            if config.storage.s3.bucket.trim().is_empty() {
                anyhow::bail!("storage.s3.bucket is required when storage.backend = \"s3\"");
            }
            if config.storage.s3.access_key.trim().is_empty() {
                anyhow::bail!("storage.s3.access_key is required when storage.backend = \"s3\"");
            }
            if config.storage.s3.secret_key.trim().is_empty() {
                anyhow::bail!("storage.s3.secret_key is required when storage.backend = \"s3\"");
            }
            tracing::info!(
                bucket = %config.storage.s3.bucket,
                region = %config.storage.s3.region,
                "Storage backend: S3"
            );
        }
        other => {
            anyhow::bail!("storage.backend must be \"local\" or \"s3\", got \"{other}\"");
        }
    }

    if config.auth.argon2_memory_kib < MIN_ARGON2_MEMORY_KIB {
        anyhow::bail!(
            "auth.argon2_memory_kib must be >= {MIN_ARGON2_MEMORY_KIB}, got {}",
            config.auth.argon2_memory_kib
        );
    }
    if config.auth.argon2_iterations < MIN_ARGON2_ITERATIONS {
        anyhow::bail!(
            "auth.argon2_iterations must be >= {MIN_ARGON2_ITERATIONS}, got {}",
            config.auth.argon2_iterations
        );
    }
    if config.auth.argon2_parallelism < MIN_ARGON2_PARALLELISM {
        anyhow::bail!(
            "auth.argon2_parallelism must be >= {MIN_ARGON2_PARALLELISM}, got {}",
            config.auth.argon2_parallelism
        );
    }
    if config.ldap.enabled
        && (config.ldap.uid_attr.trim().is_empty() || config.ldap.email_attr.trim().is_empty())
    {
        anyhow::bail!("ldap.uid_attr and ldap.email_attr are required when ldap.enabled is true");
    }
    if config.auth.proxy.enabled {
        if config.auth.proxy.trusted_cidrs.is_empty() {
            tracing::warn!(
                "auth.proxy.enabled = true but trusted_cidrs is empty - proxy auth is disabled until trusted_cidrs is configured."
            );
        }
        if config.auth.proxy.header.trim().is_empty() {
            anyhow::bail!("auth.proxy.header is required when auth.proxy.enabled is true");
        }
    }
    if should_warn_http_base_url_without_https_only(config) {
        tracing::warn!(
            "SECURITY: base_url is HTTP and https_only is false. \
             Session cookies will not have the Secure flag. \
             Set server.https_only = true or use an HTTPS base_url in production."
        );
    }
    Ok(())
}

pub(crate) fn should_warn_http_base_url_without_https_only(config: &AppConfig) -> bool {
    config.app.base_url.starts_with("http://") && !config.server.https_only
}

fn generate_jwt_secret() -> String {
    let mut secret = [0u8; 32];
    secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret)
}

fn write_config(path: &Path, config: &AppConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let rendered = toml::to_string_pretty(config)?;
    fs::write(path, rendered)?;
    Ok(())
}

fn apply_env_overrides(config: &mut AppConfig) {
    config.app.base_url = pick_env("APP_BASE_URL", &config.app.base_url);
    config.server.https_only = pick_env_bool("APP_SERVER_HTTPS_ONLY", config.server.https_only);
    config.app.storage_path = pick_env("APP_STORAGE_PATH", &config.app.storage_path);
    config.app.calibre_db_path = pick_env("APP_CALIBRE_DB_PATH", &config.app.calibre_db_path);
    config.storage.backend = pick_env("APP_STORAGE_BACKEND", &config.storage.backend);
    config.storage.s3.bucket = pick_env("APP_STORAGE_S3_BUCKET", &config.storage.s3.bucket);
    config.storage.s3.region = pick_env("APP_STORAGE_S3_REGION", &config.storage.s3.region);
    config.storage.s3.endpoint_url = pick_env(
        "APP_STORAGE_S3_ENDPOINT_URL",
        &config.storage.s3.endpoint_url,
    );
    config.storage.s3.access_key =
        pick_env("APP_STORAGE_S3_ACCESS_KEY", &config.storage.s3.access_key);
    config.storage.s3.secret_key =
        pick_env("APP_STORAGE_S3_SECRET_KEY", &config.storage.s3.secret_key);
    config.storage.s3.key_prefix =
        pick_env("APP_STORAGE_S3_KEY_PREFIX", &config.storage.s3.key_prefix);
    config.database.url = pick_env("APP_DATABASE_URL", &config.database.url);
    config.auth.jwt_secret = pick_env("APP_JWT_SECRET", &config.auth.jwt_secret);
    config.auth.access_token_ttl_mins = pick_env_u64(
        "APP_ACCESS_TOKEN_TTL_MINS",
        config.auth.access_token_ttl_mins,
    );
    config.auth.refresh_token_ttl_days = pick_env_u64(
        "APP_REFRESH_TOKEN_TTL_DAYS",
        config.auth.refresh_token_ttl_days,
    );
    config.auth.max_login_attempts =
        pick_env_u32("APP_MAX_LOGIN_ATTEMPTS", config.auth.max_login_attempts);
    config.auth.lockout_duration_mins = pick_env_u64(
        "APP_LOCKOUT_DURATION_MINS",
        config.auth.lockout_duration_mins,
    );
    config.auth.argon2_memory_kib =
        pick_env_u32("APP_ARGON2_MEMORY_KIB", config.auth.argon2_memory_kib);
    config.auth.argon2_iterations =
        pick_env_u32("APP_ARGON2_ITERATIONS", config.auth.argon2_iterations);
    config.auth.argon2_parallelism =
        pick_env_u32("APP_ARGON2_PARALLELISM", config.auth.argon2_parallelism);
    config.auth.proxy.enabled = pick_env_bool("APP_AUTH_PROXY_ENABLED", config.auth.proxy.enabled);
    config.auth.proxy.header = pick_env("APP_AUTH_PROXY_HEADER", &config.auth.proxy.header);
    config.auth.proxy.email_header = pick_env(
        "APP_AUTH_PROXY_EMAIL_HEADER",
        &config.auth.proxy.email_header,
    );

    config.meilisearch.enabled =
        pick_env_bool("APP_MEILISEARCH_ENABLED", config.meilisearch.enabled);
    config.meilisearch.url = pick_env("APP_MEILISEARCH_URL", &config.meilisearch.url);
    config.meilisearch.api_key = pick_env("APP_MEILISEARCH_API_KEY", &config.meilisearch.api_key);

    config.llm.enabled = pick_env_bool("APP_LLM_ENABLED", config.llm.enabled);
    config.llm.allow_private_endpoints = pick_env_bool(
        "APP_LLM_ALLOW_PRIVATE_ENDPOINTS",
        config.llm.allow_private_endpoints,
    );
    config.llm.librarian.endpoint =
        pick_env("APP_LLM_LIBRARIAN_ENDPOINT", &config.llm.librarian.endpoint);
    config.llm.librarian.model = pick_env("APP_LLM_LIBRARIAN_MODEL", &config.llm.librarian.model);
    config.llm.librarian.timeout_secs = pick_env_u64(
        "APP_LLM_LIBRARIAN_TIMEOUT_SECS",
        config.llm.librarian.timeout_secs,
    );
    config.llm.librarian.system_prompt = pick_env(
        "APP_LLM_LIBRARIAN_SYSTEM_PROMPT",
        &config.llm.librarian.system_prompt,
    );
    config.llm.architect.endpoint =
        pick_env("APP_LLM_ARCHITECT_ENDPOINT", &config.llm.architect.endpoint);
    config.llm.architect.model = pick_env("APP_LLM_ARCHITECT_MODEL", &config.llm.architect.model);
    config.llm.architect.timeout_secs = pick_env_u64(
        "APP_LLM_ARCHITECT_TIMEOUT_SECS",
        config.llm.architect.timeout_secs,
    );
    config.llm.architect.system_prompt = pick_env(
        "APP_LLM_ARCHITECT_SYSTEM_PROMPT",
        &config.llm.architect.system_prompt,
    );

    config.limits.upload_max_bytes =
        pick_env_u64("APP_UPLOAD_MAX_BYTES", config.limits.upload_max_bytes);
    config.limits.rate_limit_per_ip =
        pick_env_u32("APP_RATE_LIMIT_PER_IP", config.limits.rate_limit_per_ip);

    config.app.base_url = pick_env("BASE_URL", &config.app.base_url);
    config.server.https_only = pick_env_bool("SERVER_HTTPS_ONLY", config.server.https_only);
    config.app.storage_path = pick_env("STORAGE_PATH", &config.app.storage_path);
    config.app.calibre_db_path = pick_env("CALIBRE_DB_PATH", &config.app.calibre_db_path);
    config.storage.backend = pick_env("STORAGE_BACKEND", &config.storage.backend);
    config.storage.s3.bucket = pick_env("STORAGE_S3_BUCKET", &config.storage.s3.bucket);
    config.storage.s3.region = pick_env("STORAGE_S3_REGION", &config.storage.s3.region);
    config.storage.s3.endpoint_url =
        pick_env("STORAGE_S3_ENDPOINT_URL", &config.storage.s3.endpoint_url);
    config.storage.s3.access_key = pick_env("STORAGE_S3_ACCESS_KEY", &config.storage.s3.access_key);
    config.storage.s3.secret_key = pick_env("STORAGE_S3_SECRET_KEY", &config.storage.s3.secret_key);
    config.storage.s3.key_prefix = pick_env("STORAGE_S3_KEY_PREFIX", &config.storage.s3.key_prefix);
    config.database.url = pick_env("DATABASE_URL", &config.database.url);
    config.auth.jwt_secret = pick_env("JWT_SECRET", &config.auth.jwt_secret);
    config.auth.proxy.enabled = pick_env_bool("AUTH_PROXY_ENABLED", config.auth.proxy.enabled);
    config.auth.proxy.header = pick_env("AUTH_PROXY_HEADER", &config.auth.proxy.header);
    config.auth.proxy.email_header =
        pick_env("AUTH_PROXY_EMAIL_HEADER", &config.auth.proxy.email_header);
    config.meilisearch.enabled = pick_env_bool("MEILISEARCH_ENABLED", config.meilisearch.enabled);
    config.meilisearch.url = pick_env("MEILISEARCH_URL", &config.meilisearch.url);
    config.meilisearch.api_key = pick_env("MEILISEARCH_API_KEY", &config.meilisearch.api_key);
    config.llm.enabled = pick_env_bool("ENABLE_LLM_FEATURES", config.llm.enabled);
    config.llm.allow_private_endpoints = pick_env_bool(
        "LLM_ALLOW_PRIVATE_ENDPOINTS",
        config.llm.allow_private_endpoints,
    );
}

fn pick_env(key: &str, current: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| current.to_string())
}

fn pick_env_bool(key: &str, current: bool) -> bool {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(current)
}

fn pick_env_u64(key: &str, current: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(current)
}

fn pick_env_u32(key: &str, current: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(current)
}

fn validate_jwt_secret(secret: &str) -> anyhow::Result<()> {
    let trimmed = secret.trim();
    let decoded = decode_base64_secret(trimmed).map_err(|err| {
        tracing::error!(error = %err, "jwt_secret is not valid base64");
        anyhow::anyhow!("jwt_secret must be base64-encoded")
    })?;

    if decoded.len() < MIN_JWT_SECRET_BYTES {
        tracing::error!(
            decoded_len = decoded.len(),
            min_len = MIN_JWT_SECRET_BYTES,
            "jwt_secret is too short"
        );
        anyhow::bail!("jwt_secret must decode to at least {MIN_JWT_SECRET_BYTES} bytes");
    }

    Ok(())
}

fn decode_base64_secret(secret: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(secret)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(secret))
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(secret))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(secret))
}

// LLM endpoints are validated at config load/startup, not on each request.
// Private/loopback endpoints are warned about at startup but not blocked so local
// model servers remain usable by default.
fn validate_llm_endpoints(config: &AppConfig) -> anyhow::Result<()> {
    for endpoint in llm_endpoints(config) {
        if endpoint.trim().is_empty() {
            continue;
        }
        let endpoint_info = inspect_llm_endpoint(endpoint)?;
        if endpoint_info.private_or_loopback && !config.llm.allow_private_endpoints {
            tracing::warn!(
                endpoint = %endpoint,
                resolved_ips = ?endpoint_info.resolved_ips,
                "LLM endpoint resolves to a private/loopback address; startup continues because llm.allow_private_endpoints = false"
            );
        }
    }
    Ok(())
}

fn llm_endpoints(config: &AppConfig) -> [&str; 2] {
    [
        config.llm.librarian.endpoint.as_str(),
        config.llm.architect.endpoint.as_str(),
    ]
}

/// Validate a single LLM endpoint URL for use in runtime API handlers.
///
/// Called when an admin updates the LLM config via the API (as opposed to startup
/// validation which uses [`validate_llm_endpoints`] over all configured endpoints).
/// Returns [`crate::AppError::BadRequest`] for invalid URLs and
/// [`crate::AppError::SsrfBlocked`] for private/loopback addresses when those are
/// not explicitly allowed.
pub fn validate_llm_endpoint(
    url: &str,
    allow_private_endpoints: bool,
) -> Result<(), crate::AppError> {
    let endpoint_info = inspect_llm_endpoint(url).map_err(|_| crate::AppError::BadRequest)?;
    if endpoint_info.private_or_loopback && !allow_private_endpoints {
        return Err(crate::AppError::SsrfBlocked);
    }

    Ok(())
}

struct LlmEndpointInfo {
    private_or_loopback: bool,
    resolved_ips: Vec<IpAddr>,
}

fn inspect_llm_endpoint(endpoint: &str) -> anyhow::Result<LlmEndpointInfo> {
    let parsed = reqwest::Url::parse(endpoint)
        .map_err(|e| anyhow::anyhow!("invalid LLM endpoint URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => anyhow::bail!("invalid LLM endpoint URL: only http and https are allowed"),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("LLM endpoint URL must include a host"))?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(LlmEndpointInfo {
            private_or_loopback: is_private_or_loopback(ip),
            resolved_ips: vec![ip],
        });
    }

    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("LLM endpoint URL must include a port or known scheme"))?;
    let resolved_ips = (host, port)
        .to_socket_addrs()
        .map_err(|err| anyhow::anyhow!("failed to resolve LLM endpoint host {host}: {err}"))?
        .map(|addr| addr.ip())
        .collect::<Vec<_>>();

    if resolved_ips.is_empty() {
        anyhow::bail!("failed to resolve LLM endpoint host {host}");
    }

    Ok(LlmEndpointInfo {
        private_or_loopback: resolved_ips.iter().copied().any(is_private_or_loopback),
        resolved_ips,
    })
}

/// Returns `true` for RFC 1918 private, loopback, link-local, and documentation ranges.
///
/// Used to block SSRF via LLM endpoints and webhook target URLs.
pub(crate) fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_loopback()
                || (octets[0] == 10)
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || is_link_local(ip)
                || is_documentation(ip)
        }
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            v6.is_loopback()
                || (octets[0] == 0xfc || octets[0] == 0xfd)
                || is_link_local(ip)
                || is_documentation(ip)
        }
    }
}

fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.octets()[0] == 169 && v4.octets()[1] == 254,
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80
        }
    }
}

fn is_documentation(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x0d && octets[3] == 0xb8
        }
    }
}

#[cfg(unix)]
fn warn_if_world_readable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    if metadata.permissions().mode() & 0o004 != 0 {
        tracing::warn!(path = %path.display(), "config file is world-readable");
    }
    Ok(())
}

#[cfg(not(unix))]
fn warn_if_world_readable(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_base_url_with_https_only_disabled_triggers_warning_condition() {
        let mut config = AppConfig::default();
        config.app.base_url = "http://myserver.com".to_string();
        config.server.https_only = false;

        assert!(should_warn_http_base_url_without_https_only(&config));
    }
}
