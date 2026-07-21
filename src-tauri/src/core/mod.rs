//! The headless engine. Everything here is UI-agnostic and unit-testable
//! without the Tauri shell.
//!
//! Dependency order: config → probe → encoders → encode → verify → paths →
//! replace → manifest → discover → estimate → pipeline, with `ffbin` locating
//! the bundled binaries and `report` providing the frontend boundary.

pub mod abort;
pub mod config;
pub mod decode;
pub mod discover;
pub mod encode;
pub mod encoders;
pub mod estimate;
pub mod ffbin;
pub mod fswatch;
pub mod health;
pub mod hwcaps;
pub mod idle;
pub mod library;
pub mod lock;
pub mod manifest;
pub mod paths;
pub mod pipeline;
pub mod probe;
pub mod replace;
pub mod report;
pub mod schedule;
pub mod util;
pub mod verify;
pub mod vmaf;

pub use config::{Codec, Config, OnSuccess, QualityPreset};
pub use encoders::{Detection, Encoder, EncoderFamily};
pub use ffbin::FfBin;
pub use library::{SavedLibrary, Trigger, WatchConfig};
pub use manifest::Manifest;
pub use report::{Outcome, ProcessResult, Reporter};
