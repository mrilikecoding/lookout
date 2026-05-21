use clap::{Parser, Subcommand};
use lookout::error::Result;
use lookout::runtime::{run_server, ServerConfig};
use lookout::tui::app::{TuiApp, UiSnapshot};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run server + TUI in one process (default).
    Tui,
    /// Run server only, no TUI.
    Serve,
    /// Attach a viewer TUI to a running headless server.
    View {
        /// URL of the running serve.
        #[arg(long, default_value = "http://127.0.0.1:9477")]
        url: String,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    let _guard = lookout::logging::init(args.debug)?;

    match args.cmd.unwrap_or(Cmd::Tui) {
        Cmd::Tui => run_tui(args.port, args.max_cards, args.image_paths).await,
        Cmd::Serve => run_serve(args.port, args.max_cards, args.image_paths).await,
        Cmd::View { url: _ } => {
            tracing::info!("view subcommand not yet implemented");
            unimplemented!("view mode lands in P2.T13")
        }
    }
}

async fn run_serve(port: u16, max_cards: usize, image_paths: Vec<PathBuf>) -> Result<()> {
    tracing::info!(port, max_cards, "lookout starting (serve mode, headless)");
    let handles = run_server(ServerConfig {
        port,
        max_cards,
        image_paths,
    })
    .await?;

    eprintln!("lookout serve listening on {}", handles.url);

    tokio::signal::ctrl_c().await.ok();

    tracing::info!("lookout shutting down");
    handles.server.shutdown();
    drop(handles.cmd_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handles.state_loop).await;

    Ok(())
}

async fn run_tui(port: u16, max_cards: usize, image_paths: Vec<PathBuf>) -> Result<()> {
    tracing::info!(port, max_cards, "lookout starting (tui mode)");
    let handles = run_server(ServerConfig {
        port,
        max_cards,
        image_paths,
    })
    .await?;

    let state_for_refresh = handles.state.clone();
    let url_for_refresh = handles.url.clone();
    let refresh = Arc::new(move || {
        let s = state_for_refresh.lock().unwrap();
        UiSnapshot {
            feed: s.feed().iter().cloned().collect(),
            pins: s
                .pins()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            url: url_for_refresh.clone(),
        }
    });

    let delta_rx = handles.delta_tx.subscribe();
    let app = TuiApp::new(delta_rx, refresh, handles.cmd_tx.clone());
    let tui_result = app.run().await;

    tracing::info!("lookout shutting down");
    handles.server.shutdown();
    drop(handles.cmd_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handles.state_loop).await;

    tui_result
}
