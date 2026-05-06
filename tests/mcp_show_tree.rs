//! Integration test: prove that a `show_tree` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_ok_text, TestServer};

#[tokio::test]
async fn show_tree_with_data_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-tree").await?;
    let parsed = s
        .call_tool(
            "show_tree",
            serde_json::json!({
                "data": { "a": [1, 2, 3], "b": { "c": "x" } }
            }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}
