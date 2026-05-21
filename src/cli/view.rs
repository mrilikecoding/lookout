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

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(64);
    let mcp_url = format!("{url}/mcp");
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            let result = match cmd {
                Command::ClearFeed => {
                    call_mcp_tool(&mcp_url, "clear_feed", serde_json::json!({})).await
                }
                Command::Unpin { slot } => {
                    call_mcp_tool(&mcp_url, "unpin", serde_json::json!({"slot": slot})).await
                }
                Command::PinCard { card_id, slot } => {
                    call_mcp_tool(
                        &mcp_url,
                        "pin_card",
                        serde_json::json!({
                            "card_id": card_id.0.to_string(),
                            "slot": slot,
                        }),
                    )
                    .await
                }
                // View never originates PushCard or SetSessionLabel — those
                // are agent-driven. If they appear here it's a TUI bug; log
                // and drop without forwarding.
                Command::PushCard(_) | Command::SetSessionLabel { .. } => {
                    tracing::warn!(?cmd, "view: unexpected command type, dropping");
                    continue;
                }
            };
            if let Err(e) = result {
                tracing::warn!(error = %e, "view: mcp control call failed");
            }
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

/// Fire-and-forget MCP tool call against the remote serve. Returns Err on
/// HTTP failure, but the caller usually ignores it (tools that go through
/// here are TUI keybinds — a transient error gets logged, not surfaced to
/// the user).
async fn call_mcp_tool(
    mcp_url: &str,
    name: &str,
    args: serde_json::Value,
) -> Result<()> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    let resp = reqwest::Client::new()
        .post(mcp_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            crate::error::Error::Io(std::io::Error::other(format!("mcp call: {e}")))
        })?;
    if !resp.status().is_success() {
        return Err(crate::error::Error::Io(std::io::Error::other(format!(
            "mcp call {name} failed: {}",
            resp.status()
        ))));
    }
    Ok(())
}

/// Best-effort delta application. Snapshot replays cards through the public
/// AppState API. CardPushed/PinRemoved/FeedCleared now mutate the mirror so
/// the TUI render stays accurate on live server updates.
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
        StateDelta::CardPushed { card, .. } => {
            // s.push internally also emits a CardPushed (and possibly PinReplaced)
            // delta vec, but those are discarded here. The server-side
            // PinReplaced (if any) arrives right after as its own delta and is
            // a no-op against the now-pinned local state.
            let _ = s.push(card.clone());
        }
        StateDelta::PinRemoved { slot } => {
            let _ = s.unpin(slot);
        }
        StateDelta::FeedCleared => {
            let _ = s.clear_feed();
        }
        // CardEvicted, PinReplaced, SessionUpdated: rely on the corresponding
        // CardPushed / Snapshot to keep the local mirror consistent.
        // CardEvicted in particular requires removing a card from the feed
        // by id, which AppState doesn't currently expose publicly. The local
        // feed_max naturally evicts oldest, so it stays roughly in sync.
        _ => {}
    }
}
