use std::path::PathBuf;

use clap::Parser;
use npmgen_core::{
    DEFAULT_DRIVER, DEFAULT_MANIFEST_PATH, DEFAULT_OUT, Generator, Overrides, Project, Result,
};

/// Generate the npm publish tree (meta + per-platform packages) that ships a
/// prebuilt Rust binary.
#[derive(Debug, Parser)]
#[command(name = "npmgen", version, about, long_about = None)]
pub struct Cli {
    /// Manifest of the target crate.
    #[arg(long, env = "NPMGEN_MANIFEST_PATH", default_value = DEFAULT_MANIFEST_PATH)]
    manifest_path: PathBuf,

    /// Output root for the generated tree.
    #[arg(long, env = "NPMGEN_OUT", default_value = DEFAULT_OUT)]
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
    #[arg(long, env = "NPMGEN_BUILDER", default_value = DEFAULT_DRIVER)]
    builder: String,

    /// Restrict to these target keys (repeatable or comma-separated).
    #[arg(long = "target", env = "NPMGEN_TARGETS", value_delimiter = ',')]
    targets: Vec<String>,
}

impl Cli {
    /// Load the target crate, then generate. Loading (the cargo/TOML adapter) and
    /// generation are composed here; the library exposes them separately.
    pub fn run(self) -> Result<()> {
        let overrides = Overrides {
            package: self.package,
            bin: self.bin,
            version: self.pkg_version,
        };
        let project = Project::load(&self.manifest_path, &overrides)?;

        let mut generator = Generator::new(&project)
            .out(self.out)
            .no_build(self.no_build)
            .driver(self.builder)
            .targets(self.targets);
        if let Some(tag) = self.tag {
            generator = generator.tag(tag);
        }
        generator.run()
    }
}
