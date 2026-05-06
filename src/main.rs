use lookout::error::Result;
use lookout::imagepaths::ImagePathAllowlist;
use lookout::mcp::server::McpServer;
use lookout::state::{AppState, Command, StateDelta};
use lookout::tui::app::{TuiApp, UiSnapshot};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let port: u16 = 9477;
    let feed_max = 1000;

    let state = Arc::new(Mutex::new(AppState::new(feed_max)));

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(1024);
    let (delta_tx, delta_rx) = broadcast::channel::<StateDelta>(256);

    // State loop: drain commands, apply to shared AppState, broadcast deltas.
    let state_for_loop = state.clone();
    let delta_tx_for_loop = delta_tx.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            let deltas: Vec<StateDelta> = {
                let mut s = state_for_loop.lock().unwrap();
                match cmd {
                    Command::PushCard(card) => s.push(card),
                    Command::Unpin { slot } => s.unpin(&slot).into_iter().collect(),
                    Command::ClearFeed => vec![s.clear_feed()],
                    Command::SetSessionLabel { session, label, color } => {
                        vec![s.set_session_label(&session, label, color)]
                    }
                }
            };
            for d in deltas {
                let _ = delta_tx_for_loop.send(d);
            }
        }
    });

    // Bind MCP server. McpServer::bind takes a port (u16), not a SocketAddr.
    // The server spawns its own background task internally.
    let default_session: Arc<dyn Fn() -> String + Send + Sync> =
        Arc::new(|| "default-session".to_string());
    let cmd_tx_for_tui = cmd_tx.clone();
    let server = McpServer::bind(
        port,
        cmd_tx,
        default_session,
        ImagePathAllowlist::default_roots(),
    )
    .await?;

    let url = server.url();

    let state_for_refresh = state.clone();
    let url_for_refresh = url.clone();
    let refresh = Arc::new(move || {
        let s = state_for_refresh.lock().unwrap();
        UiSnapshot {
            feed: s.feed().iter().cloned().collect(),
            pins: s.pins().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            url: url_for_refresh.clone(),
        }
    });

    let app = TuiApp::new(delta_rx, refresh, cmd_tx_for_tui);
    app.run().await?;

    // TUI exited (q or Ctrl-C). Drop everything; tokio cleans up.
    server.shutdown();
    Ok(())
}
