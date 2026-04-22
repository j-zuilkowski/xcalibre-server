use crate::config::AppConfig;
use ldap3::{LdapConnAsync, Scope, SearchEntry};

#[derive(Debug, Clone)]
pub struct LdapUser {
    pub username: String,
    pub email: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LdapError {
    #[error("ldap connection failure")]
    Connection(#[from] ldap3::LdapError),
    #[error("ldap user missing email")]
    MissingEmail,
}

pub async fn authenticate_ldap(
    config: &AppConfig,
    username: &str,
    password: &str,
) -> Result<Option<LdapUser>, LdapError> {
    if !config.ldap.enabled {
        return Ok(None);
    }

    let (conn, mut ldap) = LdapConnAsync::new(&config.ldap.url).await?;
    ldap3::drive!(conn);

    ldap.simple_bind(&config.ldap.bind_dn, &config.ldap.bind_pw)
        .await?
        .success()?;

    let filter = format!(
        "({}={})",
        config.ldap.uid_attr,
        escape_filter_value(username)
    );
    let attrs = vec![
        config.ldap.uid_attr.as_str(),
        config.ldap.email_attr.as_str(),
    ];
    let (results, _res) = ldap
        .search(&config.ldap.search_base, Scope::Subtree, &filter, attrs)
        .await?
        .success()?;
    let Some(entry) = results.into_iter().next() else {
        return Ok(None);
    };
    let entry = SearchEntry::construct(entry);
    let user_dn = entry.dn;
    let ldap_username = entry
        .attrs
        .get(&config.ldap.uid_attr)
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_else(|| username.to_string());
    let email = entry
        .attrs
        .get(&config.ldap.email_attr)
        .and_then(|values| values.first())
        .cloned()
        .ok_or(LdapError::MissingEmail)?;

    let (conn, mut ldap) = LdapConnAsync::new(&config.ldap.url).await?;
    ldap3::drive!(conn);

    match ldap.simple_bind(&user_dn, password).await?.success() {
        Ok(_) => Ok(Some(LdapUser {
            username: ldap_username,
            email,
        })),
        Err(_) => Ok(None),
    }
}

fn escape_filter_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '*' => escaped.push_str("\\2a"),
            '(' => escaped.push_str("\\28"),
            ')' => escaped.push_str("\\29"),
            '\\' => escaped.push_str("\\5c"),
            '\0' => escaped.push_str("\\00"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
