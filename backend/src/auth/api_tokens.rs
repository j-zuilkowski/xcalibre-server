use crate::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TokenScope {
    Read,
    #[default]
    Write,
    Admin,
}

impl TokenScope {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenScope::Read => "read",
            TokenScope::Write => "write",
            TokenScope::Admin => "admin",
        }
    }
}

pub fn require_write_scope(scope: TokenScope) -> Result<(), AppError> {
    match scope {
        TokenScope::Read => Err(AppError::Forbidden("token scope insufficient".into())),
        _ => Ok(()),
    }
}

pub fn require_admin_scope(scope: TokenScope) -> Result<(), AppError> {
    match scope {
        TokenScope::Admin => Ok(()),
        _ => Err(AppError::Forbidden("token scope insufficient".into())),
    }
}
