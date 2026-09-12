use knowledge_crypto::MasterKey;
use knowledge_server::{build_router, config::ServerConfig, state::AppState};
use knowledge_store::{DatabaseSettings, Store};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    let database_url = std::env::var("DATABASE_URL")?;
    let encoded_master_key = std::env::var("KNOWLEDGE_MASTER_KEY")?;
    let master_key = MasterKey::from_base64(&encoded_master_key)?;
    let store = Store::connect(&DatabaseSettings::new(database_url, 10)).await?;
    store.migrate().await?;
    let listener = TcpListener::bind(config.listen_address).await?;
    axum::serve(listener, build_router(AppState::new(store, master_key))).await?;
    Ok(())
}
