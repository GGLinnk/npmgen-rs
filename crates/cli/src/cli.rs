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

    /// Publish only these workspace packages (repeatable or comma-separated).
    #[arg(
        short = 'p',
        long = "package",
        env = "NPMGEN_PACKAGE",
        value_delimiter = ','
    )]
    packages: Vec<String>,

    /// Publish every workspace member.
    #[arg(long, env = "NPMGEN_WORKSPACE")]
    workspace: bool,

    /// Exclude these workspace packages (repeatable or comma-separated).
    #[arg(long, env = "NPMGEN_EXCLUDE", value_delimiter = ',')]
    exclude: Vec<String>,

    /// Publish only these binaries (repeatable or comma-separated).
    #[arg(long = "bin", env = "NPMGEN_BIN", value_delimiter = ',')]
    bins: Vec<String>,

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
    /// Discover the publishable binaries (cargo's selection model), then
    /// generate them together into one tree. Discovery (the cargo adapter) and
    /// generation are composed here; the library exposes them separately.
    pub fn run(self) -> Result<()> {
        let overrides = Overrides {
            packages: self.packages,
            workspace: self.workspace,
            exclude: self.exclude,
            bins: self.bins,
            version: self.pkg_version,
        };
        let projects = Project::discover(&self.manifest_path, &overrides)?;

        let mut generator = Generator::for_projects(&projects)
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
