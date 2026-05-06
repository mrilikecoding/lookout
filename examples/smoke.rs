//! Push one of every card type to a running lookout server.
//!
//! Usage:
//!   1. In one terminal: `cargo run`
//!   2. In another:     `cargo run --example smoke`

use std::error::Error;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::var("LOOKOUT_URL").unwrap_or_else(|_| "http://127.0.0.1:9477/mcp".into());
    let client = reqwest::Client::new();

    // 1. Initialize the MCP session and capture the session id header.
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}
        }))
        .send().await?;
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(String::from);
    let _ = resp.text().await?;

    // 2. Send the initialized notification.
    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}));
    if let Some(s) = &session_id { req = req.header("mcp-session-id", s); }
    let _ = req.send().await;

    // 3. Helper to call a tool.
    let call = |name: &'static str, args: serde_json::Value| {
        let url = url.clone();
        let session_id = session_id.clone();
        let client = client.clone();
        async move {
            let mut req = client
                .post(&url)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .json(&serde_json::json!({
                    "jsonrpc":"2.0","id":42,"method":"tools/call",
                    "params":{"name": name, "arguments": args}
                }));
            if let Some(s) = &session_id { req = req.header("mcp-session-id", s); }
            let r = req.send().await?;
            let _ = r.text().await?;
            Ok::<_, Box<dyn Error>>(())
        }
    };

    call("show_text", serde_json::json!({"content":"hello from smoke","title":"text","format":"plain"})).await?;
    call("show_table", serde_json::json!({"title":"smoke table","rows":[{"id":1,"name":"a"},{"id":2,"name":"b"}]})).await?;
    call("show_chart", serde_json::json!({"title":"smoke chart","kind":"line","series":[{"name":"y","points":[[0.0,1.0],[1.0,1.5],[2.0,2.0],[3.0,1.7]]}]})).await?;
    call("show_tree", serde_json::json!({"title":"smoke tree","data":{"a":[1,2,3],"b":{"c":"x"}}})).await?;
    call("show_diff", serde_json::json!({"title":"smoke diff","before":"fn a() {\n    1\n}\n","after":"fn a() {\n    2\n}\n"})).await?;
    call("show_log", serde_json::json!({"title":"smoke log","entries":[{"level":"INFO","msg":"starting"},{"level":"WARN","msg":"slow tick"},{"level":"ERROR","msg":"boom"}]})).await?;
    call("show_status", serde_json::json!({"title":"smoke status","fields":[{"label":"p95","value":"84ms","trend":"down","style":"good"},{"label":"err","value":"0.04%","trend":"flat","style":"good"}]})).await?;
    call("show_question", serde_json::json!({"title":"smoke question","question":"Should we proceed?","options":["Yes","No"],"context":"After review of the migration plan"})).await?;

    for i in 0..=10 {
        call("show_progress", serde_json::json!({
            "id":"smoke","label":"smoke progress","current": i as f64, "total": 10.0,
            "status": format!("step {i}/10")
        })).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    println!("smoke done");
    Ok(())
}
