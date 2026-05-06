//! Integration test: prove that a `show_diff` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

use std::sync::Arc;
use std::time::Duration;

use lookout::{
    card::SessionId,
    mcp::server::McpServer,
    state::{AppState, Command, StateDelta, state_task},
};
use tokio::sync::{broadcast, mpsc};

/// Parse an SSE body and extract the `data:` field from the *last* non-empty
/// event (skipping priming events that have no meaningful JSON data).
fn extract_sse_data(body: &str) -> Option<serde_json::Value> {
    body.split("\n\n")
        .filter(|e| !e.trim().is_empty())
        .filter_map(|event| {
            event
                .lines()
                .find(|l| l.starts_with("data:"))
                .map(|l| l.trim_start_matches("data:").trim().to_owned())
        })
        .filter(|data| !data.is_empty())
        .filter_map(|data| serde_json::from_str(&data).ok())
        .last()
}

#[tokio::test]
async fn show_diff_pushes_a_card() -> anyhow::Result<()> {
    // ── State task setup ──────────────────────────────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    // ── Bind the MCP server on an ephemeral port ──────────────────────────────
    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-diff".to_string());
    let server = McpServer::bind(0, cmd_tx, default_session, lookout::imagepaths::ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url(); // e.g. "http://127.0.0.1:55123/mcp"

    let client = reqwest::Client::new();

    // ── Step 1: initialize ────────────────────────────────────────────────────
    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "integration-test", "version": "0.1.0" }
        }
    });

    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&init_body)
        .send()
        .await?;

    assert_eq!(resp.status(), 200, "initialize should return 200");

    // The session ID is returned in the `mcp-session-id` response header.
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .expect("server must return mcp-session-id")
        .to_str()?
        .to_owned();

    let _body = resp.text().await?; // Drain the SSE stream.

    // ── Step 2: initialized notification ────────────────────────────────────
    let initialized_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&initialized_body)
        .send()
        .await?;

    // Notifications return 202 Accepted.
    assert!(
        resp.status() == 200 || resp.status() == 202,
        "initialized notification should return 200 or 202, got {}",
        resp.status()
    );

    // ── Step 3: tools/call show_diff with simple before/after strings ────────
    let call_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "show_diff",
            "arguments": {
                "before": "a\nb\n",
                "after": "a\nB\n"
            }
        }
    });

    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&call_body)
        .send()
        .await?;

    assert_eq!(resp.status(), 200, "tools/call should return 200");

    let tool_resp_body = resp.text().await?;
    let parsed = extract_sse_data(&tool_resp_body)
        .expect("SSE body should contain a JSON data event");

    // Verify MCP-level success: result.content[0].text starts with "ok:"
    let content_text = parsed["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text.starts_with("ok:"),
        "tool result should be 'ok:<uuid>', got: {:?}",
        content_text
    );

    // ── Step 4: assert CardPushed delta arrived ───────────────────────────────
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let delta = loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(d @ StateDelta::CardPushed { in_feed: true, .. })) => break d,
            Ok(Ok(_other)) => continue, // skip unrelated deltas
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no CardPushed delta received within 1 second"),
        }
    };

    assert!(
        matches!(delta, StateDelta::CardPushed { in_feed: true, .. }),
        "expected CardPushed with in_feed=true"
    );

    server.shutdown();
    Ok(())
}
