use std::time::Duration;

use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;

/// Migrations are compiled into the binary from ./migrations, so the deployed
/// container can migrate itself (`citra-petcare migrate` or AUTO_MIGRATE=true)
/// without shipping .sql files alongside it.
pub static MIGRATOR: Migrator = sqlx::migrate!();

pub async fn connect(config: &Config) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}
