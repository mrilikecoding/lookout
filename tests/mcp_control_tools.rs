//! Integration tests for control MCP tools: unpin, clear_feed, set_session_label.

mod common;
use common::{response_ok_text, TestServer};
use lookout::state::StateDelta;

#[tokio::test]
async fn unpin_emits_pin_removed_delta() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-unpin").await?;

    // Pin a card first.
    s.call_tool(
        "show_text",
        serde_json::json!({ "content": "pinned content", "pin": "slot" }),
    )
    .await?;
    s.recv_matching(|d| matches!(d, StateDelta::PinReplaced { slot } if slot == "slot"))
        .await?;

    // Now unpin the slot.
    let parsed = s
        .call_tool("unpin", serde_json::json!({ "slot": "slot" }))
        .await?;
    assert!(response_ok_text(&parsed).unwrap_or("").starts_with("ok:"));
    s.recv_matching(|d| matches!(d, StateDelta::PinRemoved { slot } if slot == "slot"))
        .await?;

    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn clear_feed_emits_feed_cleared_delta() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-clear-feed").await?;

    // Push something to clear.
    s.call_tool("show_text", serde_json::json!({ "content": "some content" }))
        .await?;
    s.recv_card_pushed().await?;

    let parsed = s.call_tool("clear_feed", serde_json::json!({})).await?;
    assert_eq!(response_ok_text(&parsed), Some("ok"));
    s.recv_matching(|d| matches!(d, StateDelta::FeedCleared)).await?;

    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn set_session_label_emits_session_updated_delta() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-set-label").await?;

    let parsed = s
        .call_tool(
            "set_session_label",
            serde_json::json!({ "session": "s1", "label": "research" }),
        )
        .await?;
    assert!(response_ok_text(&parsed).unwrap_or("").starts_with("ok:"));
    s.recv_matching(|d| matches!(d, StateDelta::SessionUpdated(_)))
        .await?;

    s.shutdown();
    Ok(())
}
