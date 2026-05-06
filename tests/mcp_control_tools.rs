//! Integration tests for control MCP tools: unpin, clear_feed, set_session_label.

use std::sync::Arc;
use std::time::Duration;

use lookout::{
    card::SessionId,
    imagepaths::ImagePathAllowlist,
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

/// Handshake helper: initialize + initialized, return session id.
async fn mcp_init(client: &reqwest::Client, base_url: &str) -> String {
    let resp = client
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "integration-test", "version": "0.1.0" }
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "initialize should return 200");
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .expect("server must return mcp-session-id")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.text().await.unwrap();

    let _ = client
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .send()
        .await
        .unwrap();

    session_id
}

#[tokio::test]
async fn unpin_emits_pin_removed_delta() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-unpin".to_string());

    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    // First, show_text with a pin to create a pinned card
    let _resp1 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "show_text",
                "arguments": {
                    "content": "pinned content",
                    "pin": "slot"
                }
            }
        }))
        .send()
        .await?;

    let _ = _resp1.text().await?;

    // Consume the PinReplaced event from show_text
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::PinReplaced { slot })) if slot == "slot" => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no PinReplaced delta received within 1 second"),
        }
    }

    // Now call unpin
    let _resp2 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "unpin",
                "arguments": {
                    "slot": "slot"
                }
            }
        }))
        .send()
        .await?;

    let body2 = _resp2.text().await?;
    let parsed2 = extract_sse_data(&body2).expect("SSE body should contain a JSON data event");
    let content_text2 = parsed2["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text2.starts_with("ok:"),
        "tool result should be 'ok:slot', got: {:?}",
        content_text2
    );

    // Consume the PinRemoved event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::PinRemoved { slot })) if slot == "slot" => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no PinRemoved delta received within 1 second"),
        }
    }

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn clear_feed_emits_feed_cleared_delta() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-clear-feed".to_string());

    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    // First, show_text to add something to the feed
    let _resp1 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "show_text",
                "arguments": {
                    "content": "some content"
                }
            }
        }))
        .send()
        .await?;

    let _ = _resp1.text().await?;

    // Consume the CardPushed event from show_text
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::CardPushed { .. })) => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no CardPushed delta received within 1 second"),
        }
    }

    // Now call clear_feed
    let _resp2 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "clear_feed",
                "arguments": {}
            }
        }))
        .send()
        .await?;

    let body2 = _resp2.text().await?;
    let parsed2 = extract_sse_data(&body2).expect("SSE body should contain a JSON data event");
    let content_text2 = parsed2["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text2 == "ok",
        "tool result should be 'ok', got: {:?}",
        content_text2
    );

    // Consume the FeedCleared event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::FeedCleared)) => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no FeedCleared delta received within 1 second"),
        }
    }

    server.shutdown();
    Ok(())
}

#[tokio::test]
async fn set_session_label_emits_session_updated_delta() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-set-label".to_string());

    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    // Call set_session_label
    let _resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "set_session_label",
                "arguments": {
                    "session": "s1",
                    "label": "research"
                }
            }
        }))
        .send()
        .await?;

    let body = _resp.text().await?;
    let parsed = extract_sse_data(&body).expect("SSE body should contain a JSON data event");
    let content_text = parsed["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text.starts_with("ok:"),
        "tool result should be 'ok:s1', got: {:?}",
        content_text
    );

    // Consume the SessionUpdated event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::SessionUpdated(_))) => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no SessionUpdated delta received within 1 second"),
        }
    }

    server.shutdown();
    Ok(())
}
