//! Integration test: prove that a `show_status` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_is_error, response_ok_text, TestServer};

#[tokio::test]
async fn show_status_accepts_trend_and_style() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-status").await?;
    let parsed = s
        .call_tool(
            "show_status",
            serde_json::json!({
                "fields": [
                    { "label": "cpu", "value": "45%", "trend": "up", "style": "warn" },
                    { "label": "memory", "value": "2.3 GB", "trend": "flat", "style": "good" }
                ]
            }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn show_status_rejects_bad_trend() -> anyhow::Result<()> {
    let s = TestServer::boot("integration-test-status-err").await?;
    let parsed = s
        .call_tool(
            "show_status",
            serde_json::json!({
                "fields": [{ "label": "test", "value": "x", "trend": "sideways" }]
            }),
        )
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}
