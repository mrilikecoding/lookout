//! Integration tests for the `show_progress` MCP tool.

mod common;
use common::{response_ok_text, TestServer};
use lookout::state::StateDelta;

#[tokio::test]
async fn show_progress_replaces_pin_slot_by_id() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-progress").await?;

    // First call: id="deploy", current=0.5
    let parsed = s
        .call_tool(
            "show_progress",
            serde_json::json!({
                "id": "deploy",
                "label": "uploading",
                "current": 0.5,
                "total": 1.0
            }),
        )
        .await?;
    assert!(response_ok_text(&parsed).unwrap_or("").starts_with("ok:"));
    s.recv_matching(|d| matches!(d, StateDelta::PinReplaced { slot } if slot == "progress:deploy"))
        .await?;

    // Second call: same id, current=0.75 — must replace the pin in place.
    let parsed = s
        .call_tool(
            "show_progress",
            serde_json::json!({
                "id": "deploy",
                "label": "uploading",
                "current": 0.75,
                "total": 1.0
            }),
        )
        .await?;
    assert!(response_ok_text(&parsed).unwrap_or("").starts_with("ok:"));
    s.recv_matching(|d| matches!(d, StateDelta::PinReplaced { slot } if slot == "progress:deploy"))
        .await?;

    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn show_progress_accepts_numeric_strings() -> anyhow::Result<()> {
    // Some MCP clients serialize numeric tool-args as strings. Lookout must
    // tolerate this for the common scalars (`current`, `total`).
    let mut s = TestServer::boot("integration-test-progress-strings").await?;

    let parsed = s
        .call_tool(
            "show_progress",
            serde_json::json!({
                "id": "deploy",
                "label": "uploading",
                "current": "0.5",
                "total": "1"
            }),
        )
        .await?;
    assert!(response_ok_text(&parsed).unwrap_or("").starts_with("ok:"));
    s.recv_matching(|d| matches!(d, StateDelta::PinReplaced { slot } if slot == "progress:deploy"))
        .await?;

    s.shutdown();
    Ok(())
}
