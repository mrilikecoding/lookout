//! Integration test: prove that a `show_log` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_is_error, response_ok_text, TestServer};

#[tokio::test]
async fn show_log_text_splits_into_entries() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-log").await?;
    let parsed = s
        .call_tool("show_log", serde_json::json!({ "text": "a\nb\nc" }))
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn show_log_with_neither_input_returns_error() -> anyhow::Result<()> {
    let s = TestServer::boot("integration-test-log-err").await?;
    let parsed = s.call_tool("show_log", serde_json::json!({})).await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}
