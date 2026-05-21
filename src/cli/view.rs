//! `lookout view` subcommand. Connects to a running `serve` over SSE,
//! mirrors its state locally, and renders the existing TUI against that
//! mirror.
//!
//! Wire (T13): SSE consumer parses StateDeltas and applies them to a local
//! AppState. T14: also forwards each delta to a local broadcast channel so
//! the existing TuiApp can subscribe and render. T15: TUI keybind-issued
//! Commands route through an MCP client to the server.

use std::sync::{Arc, Mutex};

use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::{broadcast, mpsc};

use crate::error::Result;
use crate::state::{AppState, Command, StateDelta};
use crate::tui::app::{TuiApp, UiSnapshot};

/// Run view mode against a serve URL like `http://127.0.0.1:9477`. The `/events`
/// endpoint is appended internally.
pub async fn run(url: String) -> Result<()> {
    tracing::info!(%url, "view: connecting to serve");

    let state = Arc::new(Mutex::new(AppState::new(1000)));
    let (delta_tx, delta_rx) = broadcast::channel::<StateDelta>(256);

    // Stub cmd channel. T15 replaces this with an MCP-backed forwarder that
    // sends Command::ClearFeed / Unpin / PinCard to the remote serve as MCP
    // tool calls. For T14 we just log so keybind dispatch is observable.
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(64);
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            tracing::warn!(?cmd, "view: command channel not yet wired (lands in P2.T15)");
        }
    });

    // SSE consumer: own task so the foreground can run TuiApp.
    let state_for_sse = state.clone();
    let delta_tx_for_sse = delta_tx.clone();
    let sse_url = format!("{url}/events");
    tokio::spawn(async move {
        let resp = match reqwest::Client::new()
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("lookout server gone, exiting ({e})");
                std::process::exit(0);
            }
        };
        if !resp.status().is_success() {
            eprintln!(
                "lookout server gone, exiting (status {})",
                resp.status()
            );
            std::process::exit(0);
        }

        let mut stream = resp.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            match event {
                Ok(ev) => {
                    let delta: StateDelta = match serde_json::from_str(&ev.data) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(?e, event = %ev.event, "view: skipping unparseable delta");
                            continue;
                        }
                    };
                    apply_delta(&state_for_sse, &delta);
                    let _ = delta_tx_for_sse.send(delta);
                }
                Err(e) => {
                    eprintln!("lookout server gone, exiting ({e})");
                    std::process::exit(0);
                }
            }
        }

        eprintln!("lookout server gone, exiting (stream ended)");
        std::process::exit(0);
    });

    // TUI render against the local mirror.
    let state_for_refresh = state.clone();
    let url_for_refresh = url.clone();
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

    let app = TuiApp::new(delta_rx, refresh, cmd_tx);
    app.run().await
}

/// Best-effort delta application. Snapshot replays cards through the public
/// AppState API. CardPushed/CardEvicted/PinReplaced/PinRemoved/FeedCleared
/// /SessionUpdated cases need to mutate the mirror so the TUI render is
/// accurate. They are handled here against the public AppState API.
///
/// NOTE: non-Snapshot deltas do NOT update the local mirror because the
/// wire-level StateDelta variants reference cards by ID without carrying
/// their bodies. Cards pushed to the server after the initial snapshot will
/// not appear in the local state, and therefore won't render in the TUI
/// until the view reconnects and receives a fresh snapshot. This is a known
/// limitation — see T14 report. A future enhancement could send
/// fully-hydrated card bodies in CardPushed deltas.
fn apply_delta(state: &Arc<Mutex<AppState>>, delta: &StateDelta) {
    let mut s = state.lock().unwrap();
    match delta {
        StateDelta::Snapshot {
            feed,
            pins,
            sessions,
        } => {
            for card in feed {
                let _ = s.push(card.clone());
            }
            for (slot, card) in pins {
                let _ = s.pin_card(card.id, slot.clone());
            }
            for (sid, info) in sessions {
                let _ = s.set_session_label(sid, info.label.clone(), Some(info.color));
            }
        }
        // Other delta variants carry only IDs, not card bodies, so we can't
        // apply them to the local mirror without a richer payload. Deferred.
        _ => {}
    }
}
