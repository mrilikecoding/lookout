//! GET /events SSE endpoint. Streams an initial state snapshot followed by
//! live deltas from the broadcast channel. Consumed by `lookout view` for
//! cross-process state mirroring.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::{
    Extension,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::{AppState, StateDelta};

/// Shared state injected into the /events handler via axum's Extension layer.
#[derive(Clone)]
pub struct EventsState {
    pub state: Arc<Mutex<AppState>>,
    pub delta_tx: broadcast::Sender<StateDelta>,
}

/// `GET /events` handler. First SSE frame is event `snapshot` with the full
/// current state; subsequent frames are event `delta`, one per broadcast
/// message.
pub async fn events(
    Extension(es): Extension<EventsState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe BEFORE computing the snapshot so we don't miss any deltas that
    // would land between snapshot computation and subscription start. The
    // client can tolerate a delta that's already reflected in the snapshot.
    let rx = es.delta_tx.subscribe();
    let snapshot = es.state.lock().unwrap().snapshot();

    let snapshot_event = stream::once(async move {
        Ok(Event::default()
            .event("snapshot")
            .json_data(&snapshot)
            .expect("snapshot serializes"))
    });

    let live = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(delta) => Some(Ok(Event::default()
                .event("delta")
                .json_data(&delta)
                .expect("delta serializes"))),
            // Lagged: client missed messages; let the stream continue. A
            // future enhancement could send a "lagged" sentinel here.
            Err(_) => None,
        }
    });

    Sse::new(snapshot_event.chain(live)).keep_alive(KeepAlive::default())
}
