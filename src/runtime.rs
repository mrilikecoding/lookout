//! Shared server startup logic used by all run modes (tui, serve, view).
//! Boots the state loop and MCP server. Returns handles the caller uses to
//! drive a TUI (or `serve` mode's signal wait) and graceful shutdown.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::card::SessionId;
use crate::error::Result;
use crate::imagepaths::ImagePathAllowlist;
use crate::mcp::server::McpServer;
use crate::state::{AppState, Command, StateDelta};

/// Configuration for a lookout server boot. Maps to the CLI flags shared by
/// all run modes.
pub struct ServerConfig {
    pub port: u16,
    pub max_cards: usize,
    pub image_paths: Vec<PathBuf>,
}

/// Handles for a running lookout server: shared state, control channels, the
/// MCP server, and the state loop's join handle. The caller is responsible
/// for graceful shutdown: call `server.shutdown()`, drop `cmd_tx`, then await
/// `state_loop` with a timeout.
pub struct ServerHandles {
    pub state: Arc<Mutex<AppState>>,
    pub cmd_tx: mpsc::Sender<Command>,
    pub delta_tx: broadcast::Sender<StateDelta>,
    pub server: McpServer,
    pub url: String,
    pub state_loop: JoinHandle<()>,
}

/// Spin up the state loop and MCP server. Returns handles the caller uses to
/// render a TUI, await SIGINT, or expose additional HTTP endpoints.
pub async fn run_server(cfg: ServerConfig) -> Result<ServerHandles> {
    let state = Arc::new(Mutex::new(AppState::new(cfg.max_cards)));

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(1024);
    let (delta_tx, _delta_rx) = broadcast::channel::<StateDelta>(256);

    let state_for_loop = state.clone();
    let delta_tx_for_loop = delta_tx.clone();
    let state_loop = tokio::spawn(async move {
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

    let allowlist = if cfg.image_paths.is_empty() {
        ImagePathAllowlist::default_roots()
    } else {
        ImagePathAllowlist::new(cfg.image_paths)
    };

    let server = McpServer::bind(
        cfg.port,
        cmd_tx.clone(),
        Arc::new(|| SessionId::from("default-session")),
        allowlist,
        state.clone(),
        delta_tx.clone(),
    )
    .await?;
    let url = server.url();
    tracing::info!(url = %url, "mcp server bound");

    Ok(ServerHandles {
        state,
        cmd_tx,
        delta_tx,
        server,
        url,
        state_loop,
    })
}
