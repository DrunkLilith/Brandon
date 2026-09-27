//! Offline/operator master rewrap: one bounded durable batch, no secret argv.
use knowledge_server::{
    config::{DatabaseConfig, load_master_key},
    operations::rewrap_master_once,
    state::AppState,
};
use knowledge_store::{DatabaseSettings, Store};

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("{{\"event\":\"key.master_rewrap_failed\",\"code\":\"operation_failed\"}}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database = DatabaseConfig::from_env()?;
    let store = Store::connect(&DatabaseSettings::new(database.as_url(), 4)).await?;
    let state = AppState::new(store, load_master_key()?, "https://operator.invalid", "")?;
    let changed = rewrap_master_once(&state).await?;
    println!("{{\"rewrapped\":{changed}}}");
    Ok(())
}
