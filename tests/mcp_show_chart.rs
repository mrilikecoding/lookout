//! Integration test: prove that `show_chart` MCP tool calls land a card in
//! the feed and emit `StateDelta::CardPushed { in_feed: true, .. }`.

mod common;
use common::{response_ok_text, TestServer};

#[tokio::test]
async fn show_chart_with_line_kind_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-chart-line").await?;
    let parsed = s
        .call_tool(
            "show_chart",
            serde_json::json!({
                "kind": "line",
                "series": [{ "name": "metric1", "points": [[0.0, 1.0], [1.0, 2.0]] }]
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
async fn show_chart_with_sparkline_values_pushes_a_card() -> anyhow::Result<()> {
    let mut s = TestServer::boot("integration-test-chart-sparkline").await?;
    let parsed = s
        .call_tool(
            "show_chart",
            serde_json::json!({
                "kind": "sparkline",
                "series": [{ "name": "trend", "values": [1.0, 2.0, 3.0, 4.0] }]
            }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}
