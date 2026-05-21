//! `lookout view` subcommand. Connects to a running `serve` over SSE,
//! mirrors its state locally, and (in Task 14) renders the existing TUI
//! against that mirror.
//!
//! This task (T13) is wire-only: we log every received delta and apply it
//! to a local AppState, but render nothing. T14 plugs the TUI in; T15 adds
//! control-command write-back to the server via MCP.

use std::sync::{Arc, Mutex};

use eventsource_stream::Eventsource;
use futures::StreamExt;

use crate::error::Result;
use crate::state::{AppState, StateDelta};

/// Run view mode against a serve URL like `http://127.0.0.1:9477`. The /events
/// endpoint is appended internally.
pub async fn run(url: String) -> Result<()> {
    tracing::info!(%url, "view: connecting to serve");

    let state = Arc::new(Mutex::new(AppState::new(1000)));

    let events_url = format!("{url}/events");
    let resp = match reqwest::Client::new()
        .get(&events_url)
        .header("Accept", "text/event-stream")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("lookout server gone, exiting ({e})");
            return Ok(());
        }
    };

    if !resp.status().is_success() {
        eprintln!(
            "lookout server gone, exiting (status {})",
            resp.status()
        );
        return Ok(());
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
                apply_delta(&state, &delta);
                tracing::info!(event = %ev.event, kind = ?std::mem::discriminant(&delta), "view: applied delta");
            }
            Err(e) => {
                eprintln!("lookout server gone, exiting ({e})");
                return Ok(());
            }
        }
    }

    eprintln!("lookout server gone, exiting (stream ended)");
    Ok(())
}

/// Best-effort delta application. The Snapshot variant replaces the local
/// state by pushing each card back through AppState::push (which exercises
/// the same machinery the server used). Other variants are no-ops for T13;
/// T14 will fold in the rest once render is hooked up.
fn apply_delta(state: &Arc<Mutex<AppState>>, delta: &StateDelta) {
    let mut s = state.lock().unwrap();
    match delta {
        StateDelta::Snapshot { feed, pins, sessions } => {
            // The local state was just created and is empty. Replay the
            // server's snapshot into it.
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
        // Other variants are received but unused at the wire-only stage.
        // T14's render hook will need a proper apply step here.
        _ => {}
    }
}
