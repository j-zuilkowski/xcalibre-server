use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

const MIN_JWT_SECRET_BYTES: usize = 32;
const MIN_ARGON2_MEMORY_KIB: u32 = 65_536;
const MIN_ARGON2_ITERATIONS: u32 = 3;
const MIN_ARGON2_PARALLELISM: u32 = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub app: AppSection,
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
            header: "X-Remote-User".to_string(),
            email_header: "X-Remote-Email".to_string(),
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSection {
    pub enabled: bool,
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
    validate_llm_endpoints(&config).await?;

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
    if config.auth.proxy.enabled && config.auth.proxy.header.trim().is_empty() {
        anyhow::bail!("auth.proxy.header is required when auth.proxy.enabled is true");
    }
    Ok(())
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
    config.app.storage_path = pick_env("APP_STORAGE_PATH", &config.app.storage_path);
    config.app.calibre_db_path = pick_env("APP_CALIBRE_DB_PATH", &config.app.calibre_db_path);
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
    config.app.storage_path = pick_env("STORAGE_PATH", &config.app.storage_path);
    config.app.calibre_db_path = pick_env("CALIBRE_DB_PATH", &config.app.calibre_db_path);
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

// DNS is resolved once at config load/startup, not on each request.
// If LLM endpoints become runtime-configurable later, they must be validated per request.
async fn validate_llm_endpoints(config: &AppConfig) -> anyhow::Result<()> {
    for endpoint in [
        config.llm.librarian.endpoint.as_str(),
        config.llm.architect.endpoint.as_str(),
    ] {
        if endpoint.trim().is_empty() {
            continue;
        }
        validate_llm_endpoint(endpoint, config.llm.allow_private_endpoints)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
    }
    Ok(())
}

pub async fn validate_llm_endpoint(
    url: &str,
    allow_private_endpoints: bool,
) -> Result<(), crate::AppError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| crate::AppError::BadRequest)?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(crate::AppError::BadRequest),
    }

    let host = parsed.host_str().ok_or(crate::AppError::BadRequest)?;
    let port = parsed
        .port_or_known_default()
        .ok_or(crate::AppError::BadRequest)?;
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| crate::AppError::BadRequest)?;

    let mut private_ip = None;
    for addr in resolved {
        if is_private_or_loopback(addr.ip()) {
            private_ip = Some(addr.ip());
            break;
        }
    }

    if let Some(ip) = private_ip {
        if allow_private_endpoints {
            return Ok(());
        }

        tracing::warn!(
            endpoint = %url,
            resolved_ip = %ip,
            "LLM endpoint resolves to a private/loopback address while llm.allow_private_endpoints=false; continuing startup for local/NAS deployments"
        );
    }

    Ok(())
}

fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.octets()[0] == 10
                || (v4.octets()[0] == 172 && (16..=31).contains(&v4.octets()[1]))
                || (v4.octets()[0] == 192 && v4.octets()[1] == 168)
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254) // link-local / cloud metadata
                || v4 == Ipv4Addr::LOCALHOST
        }
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            v6.is_loopback()
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80) // fe80::/10 link-local
                || (octets[0] & 0xfe == 0xfc) // fc00::/7 ULA
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
