use clap::Parser;
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
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    let _guard = lookout::logging::init(args.debug)?;
    tracing::info!(port = args.port, max_cards = args.max_cards, "lookout starting");

    let handles = run_server(ServerConfig {
        port: args.port,
        max_cards: args.max_cards,
        image_paths: args.image_paths,
    })
    .await?;

    let state_for_refresh = handles.state.clone();
    let url_for_refresh = handles.url.clone();
    let refresh = Arc::new(move || {
        let s = state_for_refresh.lock().unwrap();
        UiSnapshot {
            feed: s.feed().iter().cloned().collect(),
            pins: s.pins().iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
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
