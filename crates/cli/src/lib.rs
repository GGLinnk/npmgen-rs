//! CLI driver shared by the `npmgen` and `cargo-npmgen` binaries.

mod cli;

use std::ffi::OsString;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cli::Cli;

/// Parse arguments and run a generation. Both binaries delegate here, so
/// `npmgen …`, `cargo-npmgen …`, and `cargo npmgen …` behave identically.
pub fn main() {
    init_tracing();
    let cli = Cli::parse_from(strip_cargo_subcommand(std::env::args_os()));
    if let Err(error) = cli.into_generator().run() {
        tracing::error!(%error, "npmgen failed");
        std::process::exit(1);
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
}

/// When invoked as `cargo npmgen`, cargo runs `cargo-npmgen npmgen …`; drop the
/// injected subcommand so the same parser serves every invocation.
fn strip_cargo_subcommand(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut args: Vec<OsString> = args.collect();
    if args.get(1).and_then(|arg| arg.to_str()) == Some("npmgen") {
        args.remove(1);
    }
    args
}
