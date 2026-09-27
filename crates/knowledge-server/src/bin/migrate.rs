use knowledge_server::config::DatabaseConfig;
use knowledge_store::{DatabaseSettings, Store};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = DatabaseConfig::from_env()?;
    let store = Store::connect(&DatabaseSettings::new(database.as_url(), 2)).await?;
    store.migrate().await?;
    println!("Database migrations completed");
    Ok(())
}
