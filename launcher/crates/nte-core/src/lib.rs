//! Core library for Oddity, a native Linux/Wayland launcher for
//! Neverness To Everness.
//!
//! Protocol details are kept in the project's internal notes; this crate
//! exposes only the public launcher and resource-update behavior.

pub mod attendance;
pub mod background;
pub mod bulletin;
pub mod config;
pub mod disk;
pub mod distribution;
pub mod environment;
pub mod error;
pub mod game;
pub mod game_mode;
pub mod install;
pub mod install_lock;
pub mod launch;
pub mod logs;
pub mod manifest;
mod paths;
pub mod recovery;
pub mod settings;
pub mod update;
pub mod version;

pub(crate) mod http;

#[cfg(test)]
pub(crate) mod testing;

pub use error::{Error, Result};

pub const USER_AGENT: &str = "Oddity/0.1 (+linux)";
