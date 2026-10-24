//! Core library for Oddity, a native Linux/Wayland launcher for
//! Neverness To Everness.
//!
//! Protocol details are kept in the project's internal notes; this crate
//! exposes only the public launcher and resource-update behavior.

pub mod attendance;
pub mod background;
pub mod bulletin;
pub mod config;
pub mod distribution;
pub mod error;
pub mod game;
pub mod install;
pub mod launch;
pub mod manifest;
pub mod update;
pub mod version;

#[cfg(test)]
pub(crate) mod testing;

pub use error::{Error, Result};

/// Canonical CDN used by the launcher (publish_ob branch).
pub const VERSION_URL: &str = "https://yhcdn1.wmupd.com/hd/publish_ob/launcher/Version.ini";
pub const BACKUP_VERSION_URL: &str = "https://yhcdn2.wmupd.com/hd/publish_ob/launcher/Version.ini";
pub const USER_AGENT: &str = "Oddity/0.1 (+linux)";
