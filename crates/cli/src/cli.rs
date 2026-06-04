use std::path::PathBuf;

use clap::Parser;
use npmgen_core::Generator;

/// Generate the npm publish tree (meta + per-platform packages) that ships a
/// prebuilt Rust binary.
#[derive(Debug, Parser)]
#[command(name = "npmgen", version, about, long_about = None)]
pub struct Cli {
    /// Manifest of the target crate.
    #[arg(long, env = "NPMGEN_MANIFEST_PATH", default_value = "Cargo.toml")]
    manifest_path: PathBuf,

    /// Output root for the generated tree.
    #[arg(long, env = "NPMGEN_OUT", default_value = "dist/npm")]
    out: PathBuf,

    /// Workspace package to describe and build.
    #[arg(short = 'p', long, env = "NPMGEN_PACKAGE")]
    package: Option<String>,

    /// Cargo bin name shipped in platform packages.
    #[arg(long, env = "NPMGEN_BIN")]
    bin: Option<String>,

    /// Override the package version (otherwise read from Cargo.toml).
    #[arg(long = "pkg-version", env = "NPMGEN_PKG_VERSION")]
    pkg_version: Option<String>,

    /// Require the resolved version to equal `v<tag>`.
    #[arg(long, env = "NPMGEN_TAG")]
    tag: Option<String>,

    /// Assemble from existing binaries instead of building.
    #[arg(long, env = "NPMGEN_NO_BUILD")]
    no_build: bool,

    /// Build driver invoked per target (e.g. cargo, cross, cargo-zigbuild).
    #[arg(long, env = "NPMGEN_BUILDER", default_value = "cargo")]
    builder: String,

    /// Restrict to these target keys (repeatable or comma-separated).
    #[arg(long = "target", env = "NPMGEN_TARGETS", value_delimiter = ',')]
    targets: Vec<String>,
}

impl Cli {
    /// Build the configured [`Generator`] from parsed arguments.
    pub fn into_generator(self) -> Generator {
        let mut builder = Generator::builder()
            .manifest_path(self.manifest_path)
            .out(self.out)
            .no_build(self.no_build)
            .driver(self.builder)
            .targets(self.targets);
        if let Some(package) = self.package {
            builder = builder.package(package);
        }
        if let Some(bin) = self.bin {
            builder = builder.bin(bin);
        }
        if let Some(version) = self.pkg_version {
            builder = builder.version(version);
        }
        if let Some(tag) = self.tag {
            builder = builder.tag(tag);
        }
        builder.build()
    }
}
