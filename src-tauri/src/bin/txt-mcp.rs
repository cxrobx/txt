//! txt's MCP server: stdio by default. Reads and writes the same notes folder
//! and index as the app (`TXT_NOTES_DIR` / `TXT_SUPPORT_DIR` apply here too).
//!
//! Register: `claude mcp add --scope user txt /Applications/txt.app/Contents/MacOS/txt-mcp`
//!
//! `txt-mcp resolve <id>` is a one-shot lookup for other apps (Margin's link
//! chips): it prints `{"id","title","path"}` as JSON, or exits 2 when no note
//! has that id. It reads only, so a chip never changes a note.
//!
//! `txt-mcp --http PORT` serves streamable HTTP at `http://127.0.0.1:PORT/mcp`
//! instead, so one long-lived process (a LaunchAgent) serves every session:
//! `claude mcp add --scope user --transport http txt http://127.0.0.1:PORT/mcp`.
//! It binds loopback only and rmcp refuses a non-loopback `Host` header, so a
//! web page can't reach it by DNS rebinding. Each session gets its own handler
//! over one shared store and embedding index. There is no orphan watchdog in
//! either mode: stdio exits when stdin closes, and HTTP runs until SIGTERM.

use rmcp::transport::streamable_http_server::{session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::sync::Arc;
use txt_lib::embed::{Config, Index};
use txt_lib::mcp::TxtMcp;
use txt_lib::notes::Store;

/// The port after `--http`, as `--http 18703` or `--http=18703`.
fn http_port(args: &[String]) -> Result<Option<u16>, String> {
    for (i, a) in args.iter().enumerate() {
        let raw = if a == "--http" { args.get(i + 1).map(String::as_str) } else if let Some(v) = a.strip_prefix("--http=") { Some(v) } else { continue };
        return raw.and_then(|v| v.parse::<u16>().ok()).filter(|p| *p != 0).map(Some).ok_or_else(|| "usage: txt-mcp --http PORT".to_string());
    }
    Ok(None)
}

async fn serve_http(port: u16, notes_dir: PathBuf, support: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(Store::new(notes_dir, support.clone()));
    let index = Arc::new(Index::open(&support, Config::load(&support)));
    let config = StreamableHttpServerConfig::default();
    let stop = config.cancellation_token.clone();
    let service = StreamableHttpService::new(move || Ok(TxtMcp::shared(store.clone(), index.clone())), Arc::new(LocalSessionManager::default()), config);
    let app = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("txt-mcp: serving http://127.0.0.1:{port}/mcp");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            stop.cancel();
        })
        .await?;
    Ok(())
}

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
    match http_port(&args) {
        Ok(Some(port)) => return serve_http(port, notes_dir, support).await,
        Ok(None) => {}
        Err(e) => {
            eprintln!("txt-mcp: {e}");
            std::process::exit(2);
        }
    }
    let server = TxtMcp::new(notes_dir, support);
    // stdout is the protocol; anything for a person goes to stderr.
    let service = server.serve(stdio()).await.inspect_err(|e| eprintln!("txt-mcp: {e:?}"))?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::http_port;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn http_flag() {
        assert_eq!(http_port(&a(&[])), Ok(None));
        assert_eq!(http_port(&a(&["--http", "18703"])), Ok(Some(18703)));
        assert_eq!(http_port(&a(&["--http=18703"])), Ok(Some(18703)));
        assert!(http_port(&a(&["--http"])).is_err());
        assert!(http_port(&a(&["--http", "0"])).is_err());
        assert!(http_port(&a(&["--http", "x"])).is_err());
    }
}
