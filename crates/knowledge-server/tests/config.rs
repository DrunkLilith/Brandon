use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use knowledge_server::config::ServerConfig;

#[test]
fn explicit_listen_address_is_used() {
    let config = ServerConfig::from_pairs([("KNOWLEDGE_LISTEN_ADDRESS", "127.0.0.1:4100")])
        .expect("valid address should load");

    assert_eq!(
        config.listen_address,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4100)
    );
}
