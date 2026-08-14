//! Shared data layer for UGLy.
//!
//! Deliberately free of any Tauri dependency so it can be linked by both the desktop
//! app and the MCP server, which runs as a plain stdio process without the app.

pub mod installed;
pub mod library;
pub mod models;
pub mod paths;
pub mod store;
