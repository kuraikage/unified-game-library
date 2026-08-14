//! UGLy's MCP server: exposes the game library over stdio so an assistant can answer
//! "what should I play?" against what the user actually owns.
//!
//! Runs as a standalone process launched by the MCP client, with no Tauri and no window.
//! It reads the same SQLite database the desktop app writes, so both can be open at once.
//!
//! Nothing may be written to stdout: that is the JSON-RPC channel. Diagnostics go to stderr.

mod library_tools;

use anyhow::{bail, Context, Result};
use rmcp::transport::stdio;
use rmcp::ServiceExt;

use library_tools::LibraryTools;
use ugly_core::paths;
use ugly_core::store::Store;

#[tokio::main]
async fn main() -> Result<()> {
    // `--version` keeps the binary self-identifying for anyone wiring up the config by hand.
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("ugly-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let db = paths::database_path()?;
    if !db.exists() {
        bail!(
            "No library database at {}.\n\
             Run the UGLy desktop app once and import your library first, or set {} if it \
             lives somewhere else.",
            db.display(),
            paths::DATA_DIR_ENV
        );
    }

    let data_dir = paths::data_dir()?;
    let store = Store::open(&data_dir).with_context(|| {
        format!(
            "opening the library database at {}. Is another copy of UGLy mid-write?",
            db.display()
        )
    })?;

    eprintln!("ugly-mcp {} serving {}", env!("CARGO_PKG_VERSION"), db.display());

    let service = LibraryTools::new(store)
        .serve(stdio())
        .await
        .context("starting the MCP stdio server")?;

    // Resolves when the client disconnects or the transport closes.
    service.waiting().await?;
    Ok(())
}
