use std::net::SocketAddr;

use tokio::net::UdpSocket;
use tokio::sync::oneshot;

use crate::TestServerError;

/// In-process UDP echo server: echoes every datagram back to its sender.
/// Shuts down on drop.
pub struct UdpEchoServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl UdpEchoServer {
    /// Spawns the server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        let addr = socket.local_addr()?;
        let (tx, mut rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    received = socket.recv_from(&mut buf) => {
                        match received {
                            Ok((len, peer)) => {
                                if let Err(e) = socket.send_to(&buf[..len], peer).await {
                                    tracing::debug!(error = %e, %peer, "udp echo send failed");
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "udp test server recv failed"),
                        }
                    }
                }
            }
            tracing::debug!("udp test server stopped");
        });
        tracing::debug!(%addr, "udp test server listening");
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for UdpEchoServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
