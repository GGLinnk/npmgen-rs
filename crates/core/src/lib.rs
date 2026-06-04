//! Generate the npm publish tree that ships a prebuilt Rust binary.
//!
//! The tree follows the "platform packages" pattern: a meta package whose
//! `optionalDependencies` point at one `@scope/name-<os>-<cpu>` package per
//! platform, each carrying the binary and filtered by npm `os`/`cpu`. Package
//! identity is read from the target crate via `cargo metadata`; targets, payload
//! and foreign manifests are declared under `[package.metadata.npmgen]` (or
//! `[workspace.metadata.npmgen]`).
//!
//! Entry point: [`Generator::builder`].

mod error;
mod pipeline;

pub mod compile;
pub mod config;
pub mod npm;
pub mod project;
pub mod target;

pub use config::{Config, Launcher, ManifestSpec, TargetSpec};
pub use error::{Error, Result};
pub use pipeline::{
    DEFAULT_DRIVER, DEFAULT_MANIFEST_PATH, DEFAULT_OUT, Generator, GeneratorBuilder,
};
pub use project::{Author, Identity, Overrides, Project};
pub use target::Target;
