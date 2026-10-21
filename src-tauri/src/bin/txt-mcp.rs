//! txt's MCP server: stdio, no flags. Reads and writes the same notes folder
//! and index as the app (`TXT_NOTES_DIR` / `TXT_SUPPORT_DIR` apply here too).
//!
//! Register: `claude mcp add --scope user txt /Applications/txt.app/Contents/MacOS/txt-mcp`
//!
//! `txt-mcp resolve <id>` is a one-shot lookup for other apps (Margin's link
//! chips): it prints `{"id","title","path"}` as JSON, or exits 2 when no note
//! has that id. It reads only, so a chip never changes a note.

use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (notes_dir, support) = txt_lib::dirs();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("resolve") {
        let id = args.get(1).map(|s| s.trim_start_matches("txt://note/")).unwrap_or_default();
        let store = txt_lib::notes::Store::new(notes_dir, support);
        match store.list().into_iter().find(|n| n.id == id) {
            Some(n) => println!("{}", serde_json::json!({ "id": n.id, "title": n.title, "path": n.path })),
            None => std::process::exit(2),
        }
        return Ok(());
    }
    let server = txt_lib::mcp::TxtMcp::new(notes_dir, support);
    // stdout is the protocol; anything for a person goes to stderr.
    let service = server.serve(stdio()).await.inspect_err(|e| eprintln!("txt-mcp: {e:?}"))?;
    service.waiting().await?;
    Ok(())
}
