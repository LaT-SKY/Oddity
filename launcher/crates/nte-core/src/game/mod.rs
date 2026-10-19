//! First-round game resource update (PatcherSDK protocol).
//!
//! This module intentionally does not call the shipped PatcherSDK: it speaks
//! the public CDN protocol directly.

pub mod check;
pub mod config;
pub mod download;
pub mod ledger;
pub mod manifest;
pub mod patch;
pub mod pxml;

pub use check::{CheckOptions, UpdateItem, UpdateKind, UpdatePlan};
pub use config::GameUpdateConfig;
pub use download::{apply_plan, ApplyReport, ApplyResult};
pub use manifest::{DiffManifest, GameManifest, LocalManifest, RemoteConfig};
