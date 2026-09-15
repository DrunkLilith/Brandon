use knowledge_crypto::MasterKey;
use knowledge_server::{
    build_router,
    config::{S3Config, ServerConfig},
    state::AppState,
};
use knowledge_store::{DatabaseSettings, Store};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    let objects = std::sync::Arc::new(S3Config::from_env()?.build()?);
    let attachment_retention_days = std::env::var("KNOWLEDGE_ATTACHMENT_RETENTION_DAYS")
        .unwrap_or_else(|_| "30".into())
        .parse::<u32>()
        .map_err(|_| "invalid attachment retention configuration")?;
    let database_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?;
    let encoded_master_key = Zeroizing::new(
        std::env::var("KNOWLEDGE_MASTER_KEY").map_err(|_| "KNOWLEDGE_MASTER_KEY must be set")?,
    );
    let master_key = MasterKey::from_base64(&encoded_master_key)?;
    let public_origin = std::env::var("KNOWLEDGE_PUBLIC_ORIGIN")
        .map_err(|_| "KNOWLEDGE_PUBLIC_ORIGIN must be set")?;
    let trusted_proxy_cidrs = std::env::var("KNOWLEDGE_TRUSTED_PROXY_CIDRS").unwrap_or_default();
    let store = Store::connect(&DatabaseSettings::new(database_url, 10)).await?;
    store.migrate().await?;
    let listener = TcpListener::bind(config.listen_address).await?;
    axum::serve(
        listener,
        build_router(
            AppState::new(store, master_key, &public_origin, &trusted_proxy_cidrs)?
                .with_object_store(objects)
                .with_attachment_retention(attachment_retention_days)?,
        )
        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
