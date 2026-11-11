//! What Excalidraw Desktop keeps on disk, and the rules for writing it.
//!
//! Shared by the Tauri app (`src-tauri`) and the Iced port, so it depends on
//! neither and on no UI toolkit: each app wraps these functions in its own
//! commands or messages. Moved here from `src-tauri/src` in phase 0 of
//! `docs/iced-port/PLAN.md`, behaviour unchanged.

pub mod files;
pub mod library;
pub mod recent;
pub mod scope;
pub mod session;
pub mod settings;
pub mod store;
