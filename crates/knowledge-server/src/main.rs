use knowledge_crypto::MasterKey;
use knowledge_server::{
    build_router,
    config::{DatabaseConfig, S3Config, ServerConfig, load_required_secret},
    state::AppState,
};
use knowledge_store::{DatabaseSettings, Store};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    let objects = std::sync::Arc::new(S3Config::from_env()?.build()?);
    let attachment_retention_days = std::env::var("KNOWLEDGE_ATTACHMENT_RETENTION_DAYS")
        .unwrap_or_else(|_| "30".into())
        .parse::<u32>()
        .map_err(|_| "invalid attachment retention configuration")?;
    let database = DatabaseConfig::from_env()?;
    let encoded_master_key = load_required_secret("KNOWLEDGE_MASTER_KEY")?;
    let master_key = MasterKey::from_base64(&encoded_master_key)?;
    let public_origin = std::env::var("KNOWLEDGE_PUBLIC_ORIGIN")
        .map_err(|_| "KNOWLEDGE_PUBLIC_ORIGIN must be set")?;
    let trusted_proxy_cidrs = std::env::var("KNOWLEDGE_TRUSTED_PROXY_CIDRS").unwrap_or_default();
    let store = Store::connect(&DatabaseSettings::new(database.as_url(), 10)).await?;
    let listener = TcpListener::bind(config.listen_address).await?;
    let state = AppState::new(store, master_key, &public_origin, &trusted_proxy_cidrs)?
        .with_object_store(objects)
        .with_attachment_retention(attachment_retention_days)?;
    let scheduler = tokio::spawn(knowledge_server::operations::run_scheduler(state.clone()));
    let result = axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await;
    scheduler.abort();
    result?;
    Ok(())
}
