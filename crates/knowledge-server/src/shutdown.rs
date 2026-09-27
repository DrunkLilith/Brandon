//! Stop claims immediately, drain active HTTP requests, then enforce a deadline.
use crate::{build_router, state::AppState};
use std::{
    future::{Future, IntoFuture},
    io,
    time::Duration,
};
use tokio::net::TcpListener;

pub async fn serve_until(
    listener: TcpListener,
    state: AppState,
    stop: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let scheduler = tokio::spawn(crate::operations::run_scheduler(state.clone()));
    let (send, receive) = tokio::sync::oneshot::channel();
    let server = axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = receive.await;
    })
    .into_future();
    tokio::pin!(server);
    let result = tokio::select! {
        result=&mut server=>result,
        ()=stop=>{
            scheduler.abort();
            let _=send.send(());
            match tokio::time::timeout(Duration::from_secs(15),&mut server).await {
                Ok(result)=>result,
                Err(_)=>Err(io::Error::new(io::ErrorKind::TimedOut,"shutdown drain deadline elapsed")),
            }
        }
    };
    scheduler.abort();
    let _ = scheduler.await;
    result
}

pub async fn signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("shutdown signal registration failed");
        tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
