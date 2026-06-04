//! The generation pipeline driver, configured through a builder.
//!
//! `Generator::builder()…build().run()` loads the project, checks the tag,
//! resolves targets, builds (unless skipped), and assembles the tree.

use std::path::PathBuf;

use tracing::info;

use crate::compile::Compiler;
use crate::error::{Error, Result};
use crate::npm::Assembler;
use crate::project::{Overrides, Project};
use crate::target::TargetResolver;

/// A configured generation run.
pub struct Generator {
    manifest_path: PathBuf,
    out: PathBuf,
    tag: Option<String>,
    no_build: bool,
    driver: String,
    targets: Vec<String>,
    overrides: Overrides,
}

impl Generator {
    /// Start configuring a generator.
    pub fn builder() -> GeneratorBuilder {
        GeneratorBuilder::default()
    }

    /// Run the full pipeline.
    pub fn run(&self) -> Result<()> {
        let project = Project::load(&self.manifest_path, &self.overrides)?;

        if let Some(tag) = &self.tag {
            let expected = format!("v{}", project.version);
            if tag != &expected {
                return Err(Error::TagMismatch {
                    tag: tag.clone(),
                    expected,
                });
            }
        }

        let targets =
            TargetResolver::new(&project.config, &project.workspace_root).resolve(&self.targets)?;

        if !self.no_build {
            Compiler::new(self.driver.clone()).compile_all(&project, &targets)?;
        }
        Assembler::new(&project, &targets, &self.out, !self.no_build).assemble()?;

        info!(
            package = %project.package_name(),
            version = %project.version,
            targets = targets.len(),
            out = %self.out.display(),
            "generated npm publish tree",
        );
        Ok(())
    }
}

/// Builder for [`Generator`]. Mandatory paths carry defaults; optional knobs are
/// left unset until a setter is called.
#[derive(Debug, Clone)]
pub struct GeneratorBuilder {
    manifest_path: PathBuf,
    out: PathBuf,
    tag: Option<String>,
    no_build: bool,
    driver: String,
    targets: Vec<String>,
    overrides: Overrides,
}

impl Default for GeneratorBuilder {
    fn default() -> Self {
        Self {
            manifest_path: PathBuf::from("Cargo.toml"),
            out: PathBuf::from("dist/npm"),
            tag: None,
            no_build: false,
            driver: "cargo".to_owned(),
            targets: Vec::new(),
            overrides: Overrides::default(),
        }
    }
}

impl GeneratorBuilder {
    /// Manifest of the target crate (`cargo metadata --manifest-path`).
    pub fn manifest_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.manifest_path = path.into();
        self
    }

    /// Output root for the generated tree.
    pub fn out(mut self, out: impl Into<PathBuf>) -> Self {
        self.out = out.into();
        self
    }

    /// Require the resolved version to equal `v<tag>`.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Skip the build phase and assemble from existing binaries.
    pub fn no_build(mut self, no_build: bool) -> Self {
        self.no_build = no_build;
        self
    }

    /// Build driver invoked per target (e.g. `cargo`, `cross`, `cargo-zigbuild`).
    pub fn driver(mut self, driver: impl Into<String>) -> Self {
        self.driver = driver.into();
        self
    }

    /// Restrict generation to these target keys; empty means all resolved.
    pub fn targets(mut self, targets: Vec<String>) -> Self {
        self.targets = targets;
        self
    }

    /// Select which workspace package to describe and build.
    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.overrides.package = Some(package.into());
        self
    }

    /// Override the cargo bin name shipped in platform packages.
    pub fn bin(mut self, bin: impl Into<String>) -> Self {
        self.overrides.bin = Some(bin.into());
        self
    }

    /// Override the package version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.overrides.version = Some(version.into());
        self
    }

    /// Finish configuration.
    pub fn build(self) -> Generator {
        Generator {
            manifest_path: self.manifest_path,
            out: self.out,
            tag: self.tag,
            no_build: self.no_build,
            driver: self.driver,
            targets: self.targets,
            overrides: self.overrides,
        }
    }
}
