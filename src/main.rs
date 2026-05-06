use clap::Parser;
use lookout::card::SessionId;
use lookout::error::Result;
use lookout::imagepaths::ImagePathAllowlist;
use lookout::logging;
use lookout::mcp::server::McpServer;
use lookout::state::{AppState, Command, StateDelta};
use lookout::tui::app::{TuiApp, UiSnapshot};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

#[derive(Parser, Debug)]
#[command(version, about = "Streaming visualizer with MCP interface")]
struct Args {
    #[arg(long, default_value_t = 9477)]
    port: u16,
    #[arg(long, default_value_t = 1000)]
    max_cards: usize,
    /// Comma-separated paths allowed for `show_image(path=...)`. Defaults to $HOME and $TMPDIR.
    #[arg(long, value_delimiter = ',')]
    image_paths: Vec<PathBuf>,
    #[arg(long, default_value_t = false)]
    debug: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    let _guard = logging::init(args.debug)?;
    tracing::info!(port = args.port, max_cards = args.max_cards, "lookout starting");

    let state = Arc::new(Mutex::new(AppState::new(args.max_cards)));

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(1024);
    let (delta_tx, delta_rx) = broadcast::channel::<StateDelta>(256);

    let state_for_loop = state.clone();
    let delta_tx_for_loop = delta_tx.clone();
    let state_loop = tokio::spawn(async move {
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

    let allowlist = if args.image_paths.is_empty() {
        ImagePathAllowlist::default_roots()
    } else {
        ImagePathAllowlist::new(args.image_paths)
    };

    let server = McpServer::bind(
        args.port,
        cmd_tx.clone(),
        Arc::new(|| SessionId::from("default-session")),
        allowlist,
    )
    .await?;
    let url = server.url();
    tracing::info!(url = %url, "mcp server bound");

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

    let app = TuiApp::new(delta_rx, refresh, cmd_tx.clone());
    let tui_result = app.run().await;

    // Graceful drain: stop accepting new MCP traffic, then give the state loop
    // up to 2 seconds to drain in-flight commands.
    tracing::info!("lookout shutting down");
    server.shutdown();
    drop(cmd_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), state_loop).await;

    tui_result
}
