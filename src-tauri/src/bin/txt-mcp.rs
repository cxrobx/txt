//! txt's MCP server: stdio, no flags. Reads and writes the same notes folder
//! and index as the app (`TXT_NOTES_DIR` / `TXT_SUPPORT_DIR` apply here too).
//!
//! Register: `claude mcp add --scope user txt /Applications/txt.app/Contents/MacOS/txt-mcp`

use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (notes_dir, support) = txt_lib::dirs();
    let server = txt_lib::mcp::TxtMcp::new(notes_dir, support);
    // stdout is the protocol; anything for a person goes to stderr.
    let service = server.serve(stdio()).await.inspect_err(|e| eprintln!("txt-mcp: {e:?}"))?;
    service.waiting().await?;
    Ok(())
}
