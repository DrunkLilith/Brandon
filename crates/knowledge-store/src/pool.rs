use sqlx::{PgPool, postgres::PgPoolOptions};
use thiserror::Error;

use crate::MIGRATOR;

pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

impl DatabaseSettings {
    #[must_use]
    pub fn new(url: impl Into<String>, max_connections: u32) -> Self {
        Self {
            url: url.into(),
            max_connections,
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    pub async fn connect(settings: &DatabaseSettings) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(settings.max_connections)
            .connect(&settings.url)
            .await?;
        Ok(Self { pool })
    }

    #[must_use]
    pub const fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("stored data is malformed: {0}")]
    CorruptData(&'static str),
}
