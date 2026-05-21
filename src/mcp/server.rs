//! Streamable-HTTP MCP server bootstrap.

use std::sync::{Arc, Mutex};

use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    card::SessionId,
    error::Result,
    imagepaths::ImagePathAllowlist,
    mcp::tools::LookoutServer,
    state::{AppState, Command, StateDelta},
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
        state: Arc<Mutex<AppState>>,
        delta_tx: broadcast::Sender<StateDelta>,
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
        let events_state = crate::mcp::events::EventsState {
            state: state.clone(),
            delta_tx: delta_tx.clone(),
        };

        let router = axum::Router::new()
            .nest_service("/mcp", service.clone())
            .route("/events", axum::routing::get(crate::mcp::events::events))
            .layer(axum::Extension(events_state));

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
    use crate::state::{AppState, Command, StateDelta};
    use std::sync::Mutex;
    use tokio::sync::{broadcast, mpsc};

    async fn make_server() -> McpServer {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(8);
        let (delta_tx, _) = broadcast::channel::<StateDelta>(16);
        let state = Arc::new(Mutex::new(AppState::new(8)));

        let state_for_loop = state.clone();
        let delta_tx_for_loop = delta_tx.clone();
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                let deltas: Vec<StateDelta> = {
                    let mut s = state_for_loop.lock().unwrap();
                    match cmd {
                        Command::PushCard(card) => s.push(card),
                        Command::Unpin { slot } => s.unpin(&slot).into_iter().collect(),
                        Command::PinCard { card_id, slot } => s.pin_card(card_id, slot),
                        Command::ClearFeed => vec![s.clear_feed()],
                        Command::SetSessionLabel {
                            session,
                            label,
                            color,
                        } => {
                            vec![s.set_session_label(&session, label, color)]
                        }
                    }
                };
                for d in deltas {
                    let _ = delta_tx_for_loop.send(d);
                }
            }
        });

        let session_fn: Arc<dyn Fn() -> SessionId + Send + Sync> =
            Arc::new(|| "test-session".to_string());
        McpServer::bind(
            0,
            cmd_tx,
            session_fn,
            crate::imagepaths::ImagePathAllowlist::new(vec![]),
            state,
            delta_tx,
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
        // should succeed. This is the real regression: clients that restart send
        // their old session ID, and stateful mode would return 404 "Session not
        // found". Stateless mode (NeverSessionManager) has no session store, so
        // every request is processed normally.
        //
        // We send the stale header explicitly (rather than omitting it) because
        // a sessionless request would also succeed in stateful mode (rmcp creates
        // a fresh session). The regression is specifically about *stale* IDs.
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
            .header("mcp-session-id", "stale-session-from-previous-client-run")
            .json(&body)
            .send()
            .await
            .expect("request failed");
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        assert!(
            status.is_success(),
            "expected 2xx for initialize with stale session id, got {status}; body: {body_text}"
        );
        s.shutdown();
    }

    #[tokio::test]
    async fn events_endpoint_emits_snapshot_first() {
        let s = make_server().await;
        // Convert the /mcp URL to /events
        let base = s.url().trim_end_matches("/mcp").to_string();
        let events_url = format!("{}/events", base);
        let client = reqwest::Client::new();
        let resp = client
            .get(&events_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .expect("connect");
        assert!(resp.status().is_success(), "got {}", resp.status());
        // Read enough of the body to see the snapshot frame.
        use futures::StreamExt as _;
        let mut body = resp.bytes_stream();
        let mut accumulated = String::new();
        // Pull up to 4 chunks looking for the snapshot event header.
        for _ in 0..4 {
            match body.next().await {
                Some(Ok(bytes)) => {
                    accumulated.push_str(&String::from_utf8_lossy(&bytes));
                    if accumulated.contains("event: snapshot") {
                        break;
                    }
                }
                Some(Err(e)) => panic!("stream error: {e}"),
                None => break,
            }
        }
        assert!(
            accumulated.contains("event: snapshot"),
            "expected snapshot event in initial frames, got: {accumulated}"
        );
        s.shutdown();
    }
}
