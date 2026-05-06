//! Integration tests for the `show_image` MCP tool.

mod common;
use common::{response_is_error, response_ok_text, TestServer};
use lookout::imagepaths::ImagePathAllowlist;

#[tokio::test]
async fn show_image_with_base64_pushes_a_card() -> anyhow::Result<()> {
    // Empty allowlist is fine for base64 mode (no path check needed).
    let mut s = TestServer::boot("integration-test-image-b64").await?;

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"PNG");

    let parsed = s
        .call_tool(
            "show_image",
            serde_json::json!({ "base64": b64, "mime": "image/png" }),
        )
        .await?;
    let text = response_ok_text(&parsed).unwrap_or("");
    assert!(text.starts_with("ok:"), "tool result: {text:?}");
    s.recv_card_pushed().await?;
    s.shutdown();
    Ok(())
}

#[tokio::test]
async fn show_image_path_outside_allowlist_returns_error() -> anyhow::Result<()> {
    // Write the file into one temp dir but configure the allowlist to a *different* dir.
    let allowed_dir = tempfile::tempdir()?;
    let outside_dir = tempfile::tempdir()?;
    let img_path = outside_dir.path().join("secret.png");
    std::fs::write(&img_path, b"PNG")?;

    let allowlist = ImagePathAllowlist::new(vec![allowed_dir.path().to_path_buf()]);
    let s = TestServer::boot_with_allowlist("integration-test-image-deny", allowlist).await?;

    let parsed = s
        .call_tool(
            "show_image",
            serde_json::json!({ "path": img_path.to_str().unwrap() }),
        )
        .await?;
    assert!(response_is_error(&parsed), "expected error, got: {parsed}");
    s.shutdown();
    Ok(())
}
