//! Integration tests for the `show_progress` MCP tool.

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
async fn show_progress_replaces_pin_slot_by_id() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-progress".to_string());

    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    // First call: show_progress with id="deploy", current=0.5
    let resp1 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "show_progress",
                "arguments": {
                    "id": "deploy",
                    "label": "uploading",
                    "current": 0.5,
                    "total": 1.0
                }
            }
        }))
        .send()
        .await?;

    assert_eq!(resp1.status(), 200, "show_progress (1) should return 200");
    let body1 = resp1.text().await?;
    let parsed1 = extract_sse_data(&body1).expect("SSE body should contain a JSON data event");

    let content_text1 = parsed1["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text1.starts_with("ok:"),
        "tool result should be 'ok:<uuid>', got: {:?}",
        content_text1
    );

    // Consume the PinReplaced event for the first call
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::PinReplaced { slot })) if slot == "progress:deploy" => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no PinReplaced delta received for first call within 1 second"),
        }
    }

    // Second call: show_progress with same id="deploy", but current=0.75
    let resp2 = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "show_progress",
                "arguments": {
                    "id": "deploy",
                    "label": "uploading",
                    "current": 0.75,
                    "total": 1.0
                }
            }
        }))
        .send()
        .await?;

    assert_eq!(resp2.status(), 200, "show_progress (2) should return 200");
    let body2 = resp2.text().await?;
    let parsed2 = extract_sse_data(&body2).expect("SSE body should contain a JSON data event");

    let content_text2 = parsed2["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text2.starts_with("ok:"),
        "tool result should be 'ok:<uuid>', got: {:?}",
        content_text2
    );

    // Consume the second PinReplaced event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(StateDelta::PinReplaced { slot })) if slot == "progress:deploy" => break,
            Ok(Ok(_other)) => continue,
            Ok(Err(e)) => panic!("broadcast channel error: {e}"),
            Err(_) => panic!("no second PinReplaced delta received within 1 second"),
        }
    }

    server.shutdown();
    Ok(())
}
