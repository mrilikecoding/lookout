//! Shared MCP integration-test helpers.
//!
//! Each integration test file is its own Rust crate, so this module is
//! compiled into every consumer via `mod common;`. The helpers boot a real
//! `McpServer` on an ephemeral port with the state task running, complete
//! the MCP `initialize` + `notifications/initialized` handshake, and expose
//! a small surface for calling tools and asserting on broadcast deltas.

#![allow(dead_code)] // not every consumer uses every helper

use std::sync::Arc;
use std::time::Duration;

use lookout::card::SessionId;
use lookout::imagepaths::ImagePathAllowlist;
use lookout::mcp::server::McpServer;
use lookout::state::{state_task, AppState, Command, StateDelta};
use tokio::sync::{broadcast, mpsc};

/// Parse an SSE body and extract the JSON `data:` field from the *last*
/// non-empty event (skipping priming events that have no meaningful JSON).
pub fn extract_sse_data(body: &str) -> Option<serde_json::Value> {
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

/// A booted lookout server with the state task running and a live MCP
/// session already initialized.
pub struct TestServer {
    pub server: McpServer,
    pub url: String,
    pub session_id: String,
    pub client: reqwest::Client,
    pub delta_rx: broadcast::Receiver<StateDelta>,
    pub cmd_tx: mpsc::Sender<Command>,
}

impl TestServer {
    /// Boot the server with a default-session label and an empty image-path
    /// allowlist. Performs the MCP initialize handshake before returning.
    pub async fn boot(label: &'static str) -> anyhow::Result<Self> {
        Self::boot_with_allowlist(label, ImagePathAllowlist::new(vec![])).await
    }

    /// Boot with a caller-supplied image-path allowlist (for `show_image` tests).
    pub async fn boot_with_allowlist(
        label: &'static str,
        allowlist: ImagePathAllowlist,
    ) -> anyhow::Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(16);
        let (delta_tx, delta_rx) = broadcast::channel::<StateDelta>(32);
        tokio::spawn(state_task(AppState::new(32), cmd_rx, delta_tx));

        let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
            Arc::new(move || label.to_string());
        let server =
            McpServer::bind(0, cmd_tx.clone(), default_session, allowlist).await?;
        let url = server.url();
        let client = reqwest::Client::new();
        let session_id = initialize(&client, &url, label).await?;
        Ok(Self {
            server,
            url,
            session_id,
            client,
            delta_rx,
            cmd_tx,
        })
    }

    /// Boot a server backed by a saturated capacity-1 channel that nothing
    /// drains — useful for backpressure / overload tests. The next `try_send`
    /// from the tool layer must return `Full` immediately.
    pub async fn boot_saturated(label: &'static str) -> anyhow::Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(1);
        let (_delta_tx, delta_rx) = broadcast::channel::<StateDelta>(32);

        // Hold the receiver but never read — keeps the channel alive without draining.
        tokio::spawn(async move {
            let _cmd_rx = cmd_rx;
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        // Pre-fill so the next send returns Full immediately.
        cmd_tx
            .try_send(Command::ClearFeed)
            .expect("first send should fit in capacity-1 channel");

        let default_session: Arc<dyn Fn() -> SessionId + Send + Sync> =
            Arc::new(move || label.to_string());
        let server = McpServer::bind(
            0,
            cmd_tx.clone(),
            default_session,
            ImagePathAllowlist::new(vec![]),
        )
        .await?;
        let url = server.url();
        let client = reqwest::Client::new();
        let session_id = initialize(&client, &url, label).await?;
        Ok(Self {
            server,
            url,
            session_id,
            client,
            delta_rx,
            cmd_tx,
        })
    }

    /// Call a tool with a 2-second deadline. Panics if the request hangs —
    /// used by backpressure tests to catch a regression where blocking sends
    /// reappear in the tool layer.
    pub async fn call_tool_with_deadline(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match tokio::time::timeout(
            Duration::from_secs(2),
            self.call_tool(name, arguments),
        )
        .await
        {
            Ok(r) => r,
            Err(_) => panic!("tool call '{name}' hung for 2 seconds"),
        }
    }

    /// Call an MCP tool and return the parsed JSON-RPC envelope.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .send()
            .await?;
        if resp.status() != 200 {
            anyhow::bail!("tools/call returned {}", resp.status());
        }
        let body = resp.text().await?;
        extract_sse_data(&body)
            .ok_or_else(|| anyhow::anyhow!("no SSE data in response"))
    }

    /// Wait up to one second for a delta matching `pred`. Unrelated deltas
    /// are skipped.
    pub async fn recv_matching<F>(&mut self, pred: F) -> anyhow::Result<StateDelta>
    where
        F: Fn(&StateDelta) -> bool,
    {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        loop {
            match tokio::time::timeout_at(deadline, self.delta_rx.recv()).await {
                Ok(Ok(d)) if pred(&d) => return Ok(d),
                Ok(Ok(_)) => continue,
                Ok(Err(e)) => anyhow::bail!("broadcast error: {e}"),
                Err(_) => anyhow::bail!("no matching delta within 1 second"),
            }
        }
    }

    /// Wait for any `CardPushed { in_feed: true }` delta.
    pub async fn recv_card_pushed(&mut self) -> anyhow::Result<StateDelta> {
        self.recv_matching(|d| {
            matches!(d, StateDelta::CardPushed { in_feed: true, .. })
        })
        .await
    }

    pub fn shutdown(self) {
        self.server.shutdown();
    }
}

/// Tool-call response indicates an error (either JSON-RPC error envelope or
/// `result.isError = true`).
pub fn response_is_error(parsed: &serde_json::Value) -> bool {
    parsed.get("error").is_some()
        || parsed["result"]["isError"].as_bool().unwrap_or(false)
}

/// Tool-call response carries an `ok:<uuid>` success payload.
pub fn response_ok_text(parsed: &serde_json::Value) -> Option<&str> {
    parsed["result"]["content"][0]["text"].as_str()
}

async fn initialize(
    client: &reqwest::Client,
    url: &str,
    label: &str,
) -> anyhow::Result<String> {
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": label, "version": "0.1.0" }
            }
        }))
        .send()
        .await?;
    if resp.status() != 200 {
        anyhow::bail!("initialize returned {}", resp.status());
    }
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .ok_or_else(|| anyhow::anyhow!("missing mcp-session-id header"))?
        .to_str()?
        .to_owned();
    let _ = resp.text().await?;

    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .send()
        .await?;
    if !(resp.status() == 200 || resp.status() == 202) {
        anyhow::bail!("initialized returned {}", resp.status());
    }
    Ok(session_id)
}
