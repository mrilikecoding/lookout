//! Streamable-HTTP MCP server bootstrap.
//!
//! For now this binds the listener and accepts connections. The actual tool
//! registration is added in Task 7.

use crate::error::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub struct McpServer {
    listener: TcpListener,
    bound_addr: SocketAddr,
}

impl McpServer {
    pub async fn bind(port: u16) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        let bound_addr = listener.local_addr()?;
        Ok(Self { listener, bound_addr })
    }

    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.bound_addr)
    }

    /// Run the server until the connection closes. For Task 6 this is a
    /// minimal accept-and-drop loop; Task 7 plugs in rmcp's tool dispatch.
    pub async fn run(self) -> Result<()> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\n\r\n").await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binds_to_ephemeral_port() {
        let s = McpServer::bind(0).await.unwrap();
        assert!(s.url().starts_with("http://127.0.0.1:"));
    }
}
