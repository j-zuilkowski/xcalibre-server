pub mod models;
pub mod queries;

use anyhow::Context;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::{str::FromStr, sync::OnceLock};

pub async fn connect_sqlite_pool(
    database_url: &str,
    max_connections: u32,
) -> anyhow::Result<SqlitePool> {
    maybe_register_sqlite_vec()?;

    let options = SqliteConnectOptions::from_str(database_url)
        .with_context(|| format!("invalid sqlite url: {database_url}"))?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to connect sqlite pool: {database_url}"))?;

    Ok(pool)
}

#[cfg(feature = "sqlite-vec")]
fn maybe_register_sqlite_vec() -> anyhow::Result<()> {
    static REGISTER_RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = REGISTER_RESULT.get_or_init(|| {
        let entry_point: unsafe extern "C" fn() =
            unsafe { std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ()) };
        let rc = unsafe { sqlite3_auto_extension(Some(entry_point)) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!(
                "sqlite3_auto_extension(sqlite-vec) failed with rc={rc}"
            ))
        }
    });

    match result {
        Ok(()) => Ok(()),
        Err(message) => anyhow::bail!("{message}"),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
fn maybe_register_sqlite_vec() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(feature = "sqlite-vec")]
unsafe extern "C" {
    fn sqlite3_auto_extension(x_entry_point: Option<unsafe extern "C" fn()>)
        -> std::os::raw::c_int;
}
