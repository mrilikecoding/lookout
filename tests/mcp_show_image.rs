//! Integration tests for the `show_image` MCP tool.

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
async fn show_image_with_base64_pushes_a_card() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, mut delta_rx) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-image-b64".to_string());

    // Empty allowlist is fine for base64 mode (no path check needed).
    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![])).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    // Tiny base64 payload — "PNG" as bytes, base64-encoded.
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"PNG");

    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "show_image",
                "arguments": {
                    "base64": b64,
                    "mime": "image/png"
                }
            }
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 200, "tools/call should return 200");

    let body = resp.text().await?;
    let parsed = extract_sse_data(&body).expect("SSE body should contain a JSON data event");

    let content_text = parsed["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content_text.starts_with("ok:"),
        "tool result should be 'ok:<uuid>', got: {:?}",
        content_text
    );

    // Assert CardPushed arrived.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let delta = loop {
        match tokio::time::timeout_at(deadline, delta_rx.recv()).await {
            Ok(Ok(d @ StateDelta::CardPushed { in_feed: true, .. })) => break d,
            Ok(Ok(_other)) => continue,
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

#[tokio::test]
async fn show_image_path_outside_allowlist_returns_error() -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
    let (delta_tx, _) = broadcast::channel::<StateDelta>(32);
    tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "integration-test-image-deny".to_string());

    // Write the file into one temp dir but configure the allowlist to a *different* dir.
    let allowed_dir = tempfile::tempdir()?;
    let outside_dir = tempfile::tempdir()?;
    let img_path = outside_dir.path().join("secret.png");
    std::fs::write(&img_path, b"PNG")?;

    let allowlist = ImagePathAllowlist::new(vec![allowed_dir.path().to_path_buf()]);
    let server = McpServer::bind(0, cmd_tx, default_session, allowlist).await?;
    let base_url = server.url();
    let client = reqwest::Client::new();
    let session_id = mcp_init(&client, &base_url).await;

    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "show_image",
                "arguments": {
                    "path": img_path.to_str().unwrap()
                }
            }
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let body = resp.text().await?;
    let parsed = extract_sse_data(&body).expect("should have SSE data");

    let has_error = parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false);
    assert!(
        has_error,
        "expected an error response for path outside allowlist, got: {parsed}"
    );

    server.shutdown();
    Ok(())
}
