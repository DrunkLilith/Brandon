//! PostgreSQL and object-storage adapters for Knowledge Platform.

mod models;
mod pool;

pub use pool::{DatabaseSettings, Store, StoreError};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
