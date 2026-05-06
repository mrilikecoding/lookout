//! Integration test: prove that when the state queue is saturated (full),
//! the MCP tools fail-fast with an overloaded error instead of blocking
//! indefinitely. Each test uses a 2-second deadline as the regression guard.

mod common;
use common::{response_is_error, TestServer};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_text_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    let s = TestServer::boot_saturated("backpressure-test").await?;
    let parsed = s
        .call_tool_with_deadline(
            "show_text",
            serde_json::json!({
                "content": "hello from backpressure test",
                "format": "plain"
            }),
        )
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unpin_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    let s = TestServer::boot_saturated("backpressure-test-unpin").await?;
    let parsed = s
        .call_tool_with_deadline("unpin", serde_json::json!({ "slot": "test-pin" }))
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clear_feed_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    let s = TestServer::boot_saturated("backpressure-test-clear").await?;
    let parsed = s
        .call_tool_with_deadline("clear_feed", serde_json::json!({}))
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_session_label_fails_fast_when_state_queue_is_full() -> anyhow::Result<()> {
    let s = TestServer::boot_saturated("backpressure-test-label").await?;
    let parsed = s
        .call_tool_with_deadline(
            "set_session_label",
            serde_json::json!({ "session": "test-id", "label": "Test Label" }),
        )
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}
