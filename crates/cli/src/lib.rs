//! CLI driver shared by the `npmgen` and `cargo-npmgen` binaries.

mod cli;

use std::ffi::OsString;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cli::Cli;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

/// Parse arguments and run a generation. Both binaries delegate here, so
/// `npmgen …`, `cargo-npmgen …`, and `cargo npmgen …` behave identically.
pub fn main() {
    init_tracing();
    let cli = Cli::parse_from(strip_cargo_subcommand(std::env::args_os()));
    if let Err(error) = cli.run() {
        tracing::error!(%error, "npmgen failed");
        std::process::exit(1);
    }
}

fn init_tracing() {
    // RUST_LOG is tracing's own observability convention, not application config,
    // so it is read here rather than routed through the clap argument surface.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER)),
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

#[cfg(test)]
mod tests {
    use super::strip_cargo_subcommand;
    use std::ffi::OsString;

    fn argv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[test]
    fn drops_cargo_injected_subcommand() {
        let stripped =
            strip_cargo_subcommand(argv(&["cargo-npmgen", "npmgen", "--out", "x"]).into_iter());
        assert_eq!(stripped, argv(&["cargo-npmgen", "--out", "x"]));
    }

    #[test]
    fn leaves_direct_invocations_untouched() {
        let standalone = strip_cargo_subcommand(argv(&["npmgen", "--out", "x"]).into_iter());
        assert_eq!(standalone, argv(&["npmgen", "--out", "x"]));

        let direct = strip_cargo_subcommand(argv(&["cargo-npmgen", "--out", "x"]).into_iter());
        assert_eq!(direct, argv(&["cargo-npmgen", "--out", "x"]));
    }
}
