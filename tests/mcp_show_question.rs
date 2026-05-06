//! Integration test: prove that a `show_question` MCP tool call lands a card in
//! the feed and emits `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_ok_text, TestServer};

#[tokio::test]
async fn show_question_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-question").await?;
    let parsed = s
        .call_tool(
            "show_question",
            serde_json::json!({
                "question": "Which approach should we take?",
                "options": ["Option A", "Option B"],
                "context": "We need to decide on the implementation strategy."
            }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}
