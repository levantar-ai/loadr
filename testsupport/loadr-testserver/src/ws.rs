use std::net::SocketAddr;

use futures::{SinkExt as _, StreamExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::TestServerError;

/// In-process WebSocket echo server.
///
/// Echoes every text and binary message back to the sender. Sending the text
/// message `ping-close` makes the server close the connection. The request
/// path is ignored. Shuts down on drop.
pub struct WsEchoServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl WsEchoServer {
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
                                tokio::spawn(handle_connection(stream, peer));
                            }
                            Err(e) => tracing::warn!(error = %e, "ws test server accept failed"),
                        }
                    }
                }
            }
            tracing::debug!("ws test server stopped");
        });
        tracing::debug!(%addr, "ws test server listening");
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    /// WebSocket URL, e.g. `ws://127.0.0.1:54321/`.
    pub fn url(&self) -> String {
        format!("ws://{}/", self.addr)
    }

    /// Base URL without a trailing slash, e.g. `ws://127.0.0.1:54321`.
    pub fn base_url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for WsEchoServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn handle_connection(stream: TcpStream, peer: SocketAddr) {
    let mut ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::debug!(error = %e, %peer, "ws handshake failed");
            return;
        }
    };
    while let Some(message) = ws.next().await {
        let message = match message {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, %peer, "ws read error");
                break;
            }
        };
        match message {
            Message::Text(text) => {
                if text.as_str() == "ping-close" {
                    let _ = ws.send(Message::Close(None)).await;
                    break;
                }
                if ws.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            Message::Binary(data) => {
                if ws.send(Message::Binary(data)).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            // Ping/Pong are handled by tungstenite automatically.
            _ => {}
        }
    }
}
