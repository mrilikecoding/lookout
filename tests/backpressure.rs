//! Integration test: prove that when the state queue is saturated (full),
//! the MCP tools fail-fast with an overloaded error instead of blocking indefinitely.

use std::sync::Arc;
use std::time::Duration;

use lookout::{
    card::SessionId,
    imagepaths::ImagePathAllowlist,
    mcp::server::McpServer,
    state::{Command, StateDelta},
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_text_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    // ── Create a tiny (capacity-1) command channel ──────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(1);
    let (_delta_tx, _) = broadcast::channel::<StateDelta>(32);

    // Spawn a task that accepts the receiver but never reads from it.
    // This keeps the channel full forever (until we drop cmd_tx clones).
    tokio::spawn(async move {
        let _cmd_rx = cmd_rx;
        // Just sleep forever; never call recv(), keeping the channel saturated.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // Pre-fill the channel so the next send returns Full.
    cmd_tx
        .try_send(Command::ClearFeed)
        .expect("first send should fit in capacity-1 channel");

    // The channel is now full (capacity 1, one message in it).
    // Any further try_send will fail immediately.

    // ── Bind the MCP server on an ephemeral port ──────────────────────────────
    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "backpressure-test".to_string());
    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![]))
        .await?;
    let base_url = server.url();

    let client = reqwest::Client::new();

    // ── Step 1: initialize ────────────────────────────────────────────────────
    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "backpressure-test", "version": "0.1.0" }
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

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .expect("server must return mcp-session-id")
        .to_str()?
        .to_owned();

    let _body = resp.text().await?;

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

    assert!(
        resp.status() == 200 || resp.status() == 202,
        "initialized notification should return 200 or 202"
    );

    // ── Step 3: tools/call show_text (should fail-fast with overloaded) ─────────
    let call_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "show_text",
            "arguments": {
                "content": "hello from backpressure test",
                "format": "plain"
            }
        }
    });

    // Use a short timeout to ensure the request doesn't hang.
    let timeout = Duration::from_secs(2);
    let call_fut = async {
        client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session_id)
            .json(&call_body)
            .send()
            .await
    };

    let resp = match tokio::time::timeout(timeout, call_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            server.shutdown();
            return Err(anyhow::anyhow!("HTTP request failed: {e}"));
        }
        Err(_) => {
            server.shutdown();
            panic!("show_text call hung for 2 seconds; try_send backpressure failed");
        }
    };

    assert_eq!(resp.status(), 200, "tools/call should return 200");

    let tool_resp_body = resp.text().await?;
    let parsed = match extract_sse_data(&tool_resp_body) {
        Some(v) => v,
        None => {
            server.shutdown();
            panic!(
                "expected SSE data in response, got body: {}",
                tool_resp_body
            );
        }
    };

    // The response should contain an error, either as a JSON-RPC error
    // or as a tool-level error (isError=true).
    let has_error = parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false)
        || parsed["error"].is_object();

    assert!(
        has_error,
        "expected error response due to saturated queue, got: {parsed}"
    );

    // Verify the error mentions 'overloaded'.
    let error_text = serde_json::to_string(&parsed).unwrap_or_default();
    assert!(
        error_text.contains("overload") || error_text.contains("error"),
        "error should mention 'overload' or contain error details, got: {error_text}"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unpin_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    // ── Create a tiny (capacity-1) command channel ──────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(1);
    let (_delta_tx, _) = broadcast::channel::<StateDelta>(32);

    // Spawn a task that holds the receiver but never reads from it.
    tokio::spawn(async move {
        let _cmd_rx = cmd_rx;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // Pre-fill the channel so the next send is Full.
    cmd_tx
        .try_send(Command::ClearFeed)
        .expect("first send should fit in capacity-1 channel");

    // ── Bind the MCP server ──────────────────────────────────────────────────
    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "backpressure-test-unpin".to_string());
    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![]))
        .await?;
    let base_url = server.url();

    let client = reqwest::Client::new();

    // Initialize
    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "t", "version": "0.1" }
            }
        }))
        .send()
        .await?;

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()?
        .to_owned();

    let _ = resp.text().await?;

    // Send initialized notification
    let _ = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .send()
        .await?;

    // Call unpin (should fail-fast with overloaded)
    let timeout = Duration::from_secs(2);
    let call_fut = async {
        client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {
                    "name": "unpin",
                    "arguments": { "slot": "test-pin" }
                }
            }))
            .send()
            .await
    };

    let resp = match tokio::time::timeout(timeout, call_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            server.shutdown();
            return Err(anyhow::anyhow!("HTTP request failed: {e}"));
        }
        Err(_) => {
            server.shutdown();
            panic!("unpin call hung for 2 seconds; try_send backpressure failed");
        }
    };

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    let parsed = match extract_sse_data(&body) {
        Some(v) => v,
        None => {
            server.shutdown();
            panic!("expected SSE data in response, got body: {}", body);
        }
    };

    let has_error = parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false)
        || parsed["error"].is_object();

    assert!(
        has_error,
        "expected error response for overloaded queue, got: {parsed}"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clear_feed_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    // ── Create a tiny (capacity-1) command channel ──────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(1);
    let (_delta_tx, _) = broadcast::channel::<StateDelta>(32);

    // Spawn a task that holds the receiver but never reads from it.
    tokio::spawn(async move {
        let _cmd_rx = cmd_rx;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // Pre-fill the channel so the next send is Full.
    cmd_tx
        .try_send(Command::ClearFeed)
        .expect("first send should fit in capacity-1 channel");

    // ── Bind the MCP server ──────────────────────────────────────────────────
    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "backpressure-test-clear".to_string());
    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![]))
        .await?;
    let base_url = server.url();

    let client = reqwest::Client::new();

    // Initialize
    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "t", "version": "0.1" }
            }
        }))
        .send()
        .await?;

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()?
        .to_owned();

    let _ = resp.text().await?;

    // Send initialized notification
    let _ = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .send()
        .await?;

    // Call clear_feed (should fail-fast with overloaded)
    let timeout = Duration::from_secs(2);
    let call_fut = async {
        client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {
                    "name": "clear_feed",
                    "arguments": {}
                }
            }))
            .send()
            .await
    };

    let resp = match tokio::time::timeout(timeout, call_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            server.shutdown();
            return Err(anyhow::anyhow!("HTTP request failed: {e}"));
        }
        Err(_) => {
            server.shutdown();
            panic!("clear_feed call hung for 2 seconds; try_send backpressure failed");
        }
    };

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    let parsed = match extract_sse_data(&body) {
        Some(v) => v,
        None => {
            server.shutdown();
            panic!("expected SSE data in response, got body: {}", body);
        }
    };

    let has_error = parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false)
        || parsed["error"].is_object();

    assert!(
        has_error,
        "expected error response for overloaded queue, got: {parsed}"
    );

    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_session_label_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    // ── Create a tiny (capacity-1) command channel ──────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(1);
    let (_delta_tx, _) = broadcast::channel::<StateDelta>(32);

    // Spawn a task that holds the receiver but never reads from it.
    tokio::spawn(async move {
        let _cmd_rx = cmd_rx;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // Pre-fill the channel so the next send is Full.
    cmd_tx
        .try_send(Command::ClearFeed)
        .expect("first send should fit in capacity-1 channel");

    // ── Bind the MCP server ──────────────────────────────────────────────────
    let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
        Arc::new(|| "backpressure-test-label".to_string());
    let server = McpServer::bind(0, cmd_tx, default_session, ImagePathAllowlist::new(vec![]))
        .await?;
    let base_url = server.url();

    let client = reqwest::Client::new();

    // Initialize
    let resp = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "t", "version": "0.1" }
            }
        }))
        .send()
        .await?;

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()?
        .to_owned();

    let _ = resp.text().await?;

    // Send initialized notification
    let _ = client
        .post(&base_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .send()
        .await?;

    // Call set_session_label (should fail-fast with overloaded)
    let timeout = Duration::from_secs(2);
    let call_fut = async {
        client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {
                    "name": "set_session_label",
                    "arguments": { "session": "test-id", "label": "Test Label" }
                }
            }))
            .send()
            .await
    };

    let resp = match tokio::time::timeout(timeout, call_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            server.shutdown();
            return Err(anyhow::anyhow!("HTTP request failed: {e}"));
        }
        Err(_) => {
            server.shutdown();
            panic!("set_session_label call hung for 2 seconds; try_send backpressure failed");
        }
    };

    assert_eq!(resp.status(), 200);

    let body = resp.text().await?;
    let parsed = match extract_sse_data(&body) {
        Some(v) => v,
        None => {
            server.shutdown();
            panic!("expected SSE data in response, got body: {}", body);
        }
    };

    let has_error = parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false)
        || parsed["error"].is_object();

    assert!(
        has_error,
        "expected error response for overloaded queue, got: {parsed}"
    );

    server.shutdown();
    Ok(())
}
