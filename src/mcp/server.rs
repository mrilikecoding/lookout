//! Streamable-HTTP MCP server bootstrap.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    card::SessionId, error::Result, imagepaths::ImagePathAllowlist, mcp::tools::LookoutServer,
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
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(cancel.child_token());

        let cmds2 = cmds.clone();
        let default_session2 = default_session.clone();
        let service: StreamableHttpService<LookoutServer, NeverSessionManager> =
            StreamableHttpService::new(
                move || {
                    Ok(LookoutServer::new(
                        cmds2.clone(),
                        default_session2.clone(),
                        image_paths.clone(),
                    ))
                },
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
    use crate::state::{state_task, AppState};
    use tokio::sync::{broadcast, mpsc};

    async fn make_server() -> McpServer {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let (delta_tx, _) = broadcast::channel(16);
        tokio::spawn(state_task(AppState::new(8), cmd_rx, delta_tx));
        let session_fn: Arc<dyn Fn() -> SessionId + Send + Sync> =
            Arc::new(|| "test-session".to_string());
        McpServer::bind(
            0,
            cmd_tx,
            session_fn,
            crate::imagepaths::ImagePathAllowlist::new(vec![]),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn binds_to_ephemeral_port() {
        let s = make_server().await;
        assert!(s.url().starts_with("http://127.0.0.1:"));
        s.shutdown();
    }

    #[tokio::test]
    async fn accepts_request_without_session_id() {
        // Stateless mode: POST to /mcp with a stale/unknown mcp-session-id header
        // should not return 404. This is the real regression — clients that restart
        // send their old session ID, and stateful mode returns 404 "Session not found".
        // In stateless mode (NeverSessionManager) every request is standalone, so
        // there is no session store to miss and the request is processed normally.
        let s = make_server().await;
        let url = s.url();
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }
        });
        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            // Simulate a client restart: send a session ID that was never created.
            .header("mcp-session-id", "stale-session-from-previous-client-run")
            .json(&body)
            .send()
            .await
            .expect("request failed");
        assert_ne!(
            resp.status().as_u16(),
            404,
            "stateless mode must not 404 on stale session id; got body: {}",
            resp.text().await.unwrap_or_default()
        );
        s.shutdown();
    }
}
