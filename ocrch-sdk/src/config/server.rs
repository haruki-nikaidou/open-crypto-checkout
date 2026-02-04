//! Server configuration.

use std::net::SocketAddr;

/// Server configuration with runtime values.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The address and port to listen on.
    pub listen: SocketAddr,
}
