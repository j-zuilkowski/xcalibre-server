use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub app: AppSection,
    pub database: DatabaseSection,
    pub auth: AuthSection,
    pub llm: LlmSection,
    pub limits: LimitsSection,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("app", &self.app)
            .field("database", &self.database)
            .field("auth", &self.auth.redacted())
            .field("llm", &self.llm)
            .field("limits", &self.limits)
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppSection {
    pub base_url: String,
    pub storage_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DatabaseSection {
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthSection {
    pub jwt_secret: String,
    pub access_token_ttl_mins: u64,
    pub refresh_token_ttl_days: u64,
    pub max_login_attempts: u32,
    pub lockout_duration_mins: u64,
}

impl AuthSection {
    fn redacted(&self) -> RedactedAuthSection {
        RedactedAuthSection
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RedactedAuthSection;

impl fmt::Debug for AuthSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthSection")
            .field("jwt_secret", &"[REDACTED]")
            .field("access_token_ttl_mins", &self.access_token_ttl_mins)
            .field("refresh_token_ttl_days", &self.refresh_token_ttl_days)
            .field("max_login_attempts", &self.max_login_attempts)
            .field("lockout_duration_mins", &self.lockout_duration_mins)
            .finish()
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
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmSection {
    pub enabled: bool,
    pub librarian: LlmRoleSection,
    pub architect: LlmRoleSection,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmRoleSection {
    pub endpoint: String,
    pub model: String,
    pub timeout_secs: u64,
    pub system_prompt: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    Ok(AppConfig::default())
}

