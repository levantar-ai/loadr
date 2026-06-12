use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::TestServerError;

/// In-process TCP echo server: echoes all bytes back until the peer closes
/// its write side (EOF). Shuts down on drop.
pub struct TcpEchoServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TcpEchoServer {
    /// Spawns the server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, mut rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, peer)) => {
                                tokio::spawn(async move {
                                    let (mut reader, mut writer) = stream.into_split();
                                    if let Err(e) = tokio::io::copy(&mut reader, &mut writer).await {
                                        tracing::debug!(error = %e, %peer, "tcp echo error");
                                    }
                                });
                            }
                            Err(e) => tracing::warn!(error = %e, "tcp test server accept failed"),
                        }
                    }
                }
            }
            tracing::debug!("tcp test server stopped");
        });
        tracing::debug!(%addr, "tcp test server listening");
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

impl Drop for TcpEchoServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
