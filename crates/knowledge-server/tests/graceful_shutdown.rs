use knowledge_server::{shutdown::serve_until, state::AppState};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn graceful_shutdown_serves_then_stops_accepting_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_until(listener, AppState::default(), async {
        let _ = receive.await;
    }));
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            b"GET /api/v1/health/live HTTP/1.1\r\nHost:localhost\r\nConnection:close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    send.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(TcpStream::connect(address).await.is_err());
}
