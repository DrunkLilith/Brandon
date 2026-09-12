//! PostgreSQL and object-storage adapters for Knowledge Platform.

pub mod audit;
pub mod groups;
pub mod spaces;
pub mod users;

mod models;
mod pool;

pub use groups::GroupRepository;
pub use pool::{DatabaseSettings, Store, StoreError};
pub use spaces::AccessResolver;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
