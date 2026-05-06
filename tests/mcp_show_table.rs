//! Integration test: prove that `show_table` MCP tool calls land a card in
//! the feed and emit `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_ok_text, TestServer};

#[tokio::test]
async fn show_table_with_rows_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-table-rows").await?;
    let parsed = s
        .call_tool(
            "show_table",
            serde_json::json!({
                "rows": [
                    { "id": 1, "name": "Alice" },
                    { "id": 2, "name": "Bob", "score": 0.95 }
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
async fn show_table_with_csv_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-table-csv").await?;
    let parsed = s
        .call_tool(
            "show_table",
            serde_json::json!({ "csv": "name,score\nAlice,0.9\nBob,0.85" }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}
