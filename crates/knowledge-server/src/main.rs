use knowledge_server::{build_router, config::ServerConfig, state::AppState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    let listener = TcpListener::bind(config.listen_address).await?;
    axum::serve(listener, build_router(AppState)).await?;
    Ok(())
}
