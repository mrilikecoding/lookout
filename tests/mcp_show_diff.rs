//! Integration test: prove that a `show_diff` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_ok_text, TestServer};

#[tokio::test]
async fn show_diff_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-diff").await?;
    let parsed = s
        .call_tool(
            "show_diff",
            serde_json::json!({ "before": "a\nb\n", "after": "a\nB\n" }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}
