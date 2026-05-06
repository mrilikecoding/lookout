//! MCP tool handlers. Populated incrementally in later tasks.

use crate::state::Command;
use tokio::sync::mpsc;

/// Handle to the state task — every tool sends Commands through this.
#[derive(Clone)]
pub struct ToolCtx {
    pub cmds: mpsc::Sender<Command>,
    pub default_session_for: std::sync::Arc<dyn Fn() -> String + Send + Sync>,
}

impl std::fmt::Debug for ToolCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCtx").finish()
    }
}
