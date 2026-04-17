use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub app: AppSection,
    pub database: DatabaseSection,
    pub auth: AuthSection,
    pub llm: LlmSection,
    pub limits: LimitsSection,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSection {
    pub base_url: String,
    pub storage_path: String,
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
}

impl Default for AuthSection {
    fn default() -> Self {
        Self {
            jwt_secret: String::new(),
            access_token_ttl_mins: 15,
            refresh_token_ttl_days: 30,
            max_login_attempts: 10,
            lockout_duration_mins: 15,
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
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSection {
    pub enabled: bool,
    pub librarian: LlmRoleSection,
    pub architect: LlmRoleSection,
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
                base_url: "http://localhost:8083".to_string(),
                storage_path: "./storage".to_string(),
            },
            database: DatabaseSection {
                url: "sqlite://library.db".to_string(),
            },
            auth: AuthSection::default(),
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
    config.database.url = pick_env("APP_DATABASE_URL", &config.database.url);
    config.auth.jwt_secret = pick_env("APP_JWT_SECRET", &config.auth.jwt_secret);
    config.auth.access_token_ttl_mins =
        pick_env_u64("APP_ACCESS_TOKEN_TTL_MINS", config.auth.access_token_ttl_mins);
    config.auth.refresh_token_ttl_days =
        pick_env_u64("APP_REFRESH_TOKEN_TTL_DAYS", config.auth.refresh_token_ttl_days);
    config.auth.max_login_attempts =
        pick_env_u32("APP_MAX_LOGIN_ATTEMPTS", config.auth.max_login_attempts);
    config.auth.lockout_duration_mins =
        pick_env_u64("APP_LOCKOUT_DURATION_MINS", config.auth.lockout_duration_mins);

    config.llm.enabled = pick_env_bool("APP_LLM_ENABLED", config.llm.enabled);
    config.llm.librarian.endpoint =
        pick_env("APP_LLM_LIBRARIAN_ENDPOINT", &config.llm.librarian.endpoint);
    config.llm.librarian.model = pick_env("APP_LLM_LIBRARIAN_MODEL", &config.llm.librarian.model);
    config.llm.librarian.timeout_secs =
        pick_env_u64("APP_LLM_LIBRARIAN_TIMEOUT_SECS", config.llm.librarian.timeout_secs);
    config.llm.librarian.system_prompt =
        pick_env("APP_LLM_LIBRARIAN_SYSTEM_PROMPT", &config.llm.librarian.system_prompt);
    config.llm.architect.endpoint =
        pick_env("APP_LLM_ARCHITECT_ENDPOINT", &config.llm.architect.endpoint);
    config.llm.architect.model = pick_env("APP_LLM_ARCHITECT_MODEL", &config.llm.architect.model);
    config.llm.architect.timeout_secs =
        pick_env_u64("APP_LLM_ARCHITECT_TIMEOUT_SECS", config.llm.architect.timeout_secs);
    config.llm.architect.system_prompt =
        pick_env("APP_LLM_ARCHITECT_SYSTEM_PROMPT", &config.llm.architect.system_prompt);

    config.limits.upload_max_bytes =
        pick_env_u64("APP_UPLOAD_MAX_BYTES", config.limits.upload_max_bytes);
    config.limits.rate_limit_per_ip =
        pick_env_u32("APP_RATE_LIMIT_PER_IP", config.limits.rate_limit_per_ip);

    config.app.base_url = pick_env("BASE_URL", &config.app.base_url);
    config.app.storage_path = pick_env("STORAGE_PATH", &config.app.storage_path);
    config.database.url = pick_env("DATABASE_URL", &config.database.url);
    config.auth.jwt_secret = pick_env("JWT_SECRET", &config.auth.jwt_secret);
    config.llm.enabled = pick_env_bool("ENABLE_LLM_FEATURES", config.llm.enabled);
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
