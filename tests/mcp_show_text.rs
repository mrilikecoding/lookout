//! Integration test: prove that a `show_text` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_is_error, response_ok_text, TestServer};

#[tokio::test]
async fn show_text_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test").await?;
    let parsed = s
        .call_tool(
            "show_text",
            serde_json::json!({
                "content": "hello from integration test",
                "format": "plain"
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
async fn show_text_with_invalid_format_returns_error() -> anyhow::Result<()> {
    let s = TestServer::boot("integration-test-err").await?;
    let parsed = s
        .call_tool(
            "show_text",
            serde_json::json!({ "content": "x", "format": "html" }),
        )
        .await?;
    assert!(
        response_is_error(&parsed),
        "expected error response, got: {parsed}"
    );
    s.shutdown();
    Ok(())
}
