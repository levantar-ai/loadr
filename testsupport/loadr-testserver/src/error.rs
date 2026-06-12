/// Errors that can occur while spawning a test server.
#[derive(Debug, thiserror::Error)]
pub enum TestServerError {
    /// Socket binding or other I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Certificate generation or TLS configuration failure.
    #[error("TLS setup error: {0}")]
    Tls(String),
    /// gRPC server construction failure.
    #[error("gRPC setup error: {0}")]
    Grpc(String),
}
