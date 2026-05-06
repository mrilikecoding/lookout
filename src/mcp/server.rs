//! Streamable-HTTP MCP server bootstrap.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    card::SessionId,
    error::Result,
    imagepaths::ImagePathAllowlist,
    mcp::tools::LookoutServer,
    state::Command,
};

pub struct McpServer {
    addr: std::net::SocketAddr,
    cancel: CancellationToken,
}

impl McpServer {
    /// Bind to `127.0.0.1:<port>`.  Pass `0` for an ephemeral port.
    pub async fn bind(
        port: u16,
        cmds: mpsc::Sender<Command>,
        default_session: Arc<dyn Fn() -> SessionId + Send + Sync>,
        image_paths: ImagePathAllowlist,
    ) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let addr = listener.local_addr()?;

        let cancel = CancellationToken::new();

        let config = StreamableHttpServerConfig::default()
            .with_sse_keep_alive(None)
            .with_cancellation_token(cancel.child_token());

        let cmds2 = cmds.clone();
        let default_session2 = default_session.clone();
        let service: StreamableHttpService<LookoutServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(LookoutServer::new(cmds2.clone(), default_session2.clone(), image_paths.clone())),
                Default::default(),
                config,
            );

        // Spawn the axum server inline so `bind` returns an already-running server.
        let router = axum::Router::new().nest_service("/mcp", service.clone());
        let cancel_cloned = cancel.clone();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move { cancel_cloned.cancelled_owned().await })
                .await;
        });

        Ok(Self { addr, cancel })
    }

    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    pub fn addr(&self) -> std::net::SocketAddr {
        self.addr
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Gracefully stop the server.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Run until the cancellation token fires.  If you called [`bind`], the
    /// axum server is already running in a background task; call this if you
    /// need a future to `.await` for tests or CLI that block on the server.
    pub async fn run(self) -> Result<()> {
        self.cancel.cancelled().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, state_task};
    use tokio::sync::{broadcast, mpsc};

    async fn make_server() -> McpServer {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let (delta_tx, _) = broadcast::channel(16);
        tokio::spawn(state_task(AppState::new(8), cmd_rx, delta_tx));
        let session_fn: Arc<dyn Fn() -> SessionId + Send + Sync> =
            Arc::new(|| "test-session".to_string());
        McpServer::bind(0, cmd_tx, session_fn, crate::imagepaths::ImagePathAllowlist::new(vec![])).await.unwrap()
    }

    #[tokio::test]
    async fn binds_to_ephemeral_port() {
        let s = make_server().await;
        assert!(s.url().starts_with("http://127.0.0.1:"));
        s.shutdown();
    }
}
