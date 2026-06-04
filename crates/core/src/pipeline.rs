//! The generation pipeline over a resolved [`Project`]: check the tag, resolve
//! targets, build (unless skipped), assemble the tree.
//!
//! Acquiring the project is a separate concern: build one in memory with
//! [`Project::builder`](crate::Project::builder) or load it from a manifest with
//! [`Project::load`](crate::Project::load). The engine itself reads no manifest.

use std::path::PathBuf;

use tracing::info;

use crate::compile::{BuildDriver, CargoDriver, Compiler};
use crate::error::{Error, Result};
use crate::npm::Assembler;
use crate::project::Project;
use crate::target::TargetResolver;

/// Default output root for the generated tree.
pub const DEFAULT_OUT: &str = "dist/npm";
/// Default build driver command.
pub const DEFAULT_DRIVER: &str = "cargo";

/// Release-tag prefix that `--tag` is checked against (`v<version>`).
const TAG_PREFIX: &str = "v";

/// Generates the publish tree for a [`Project`]. Construct with [`Generator::new`]
/// and configure with the chained setters.
#[derive(Debug)]
pub struct Generator<'a> {
    project: &'a Project,
    out: PathBuf,
    tag: Option<String>,
    no_build: bool,
    driver: String,
    targets: Vec<String>,
    build_driver: Option<&'a dyn BuildDriver>,
}

impl<'a> Generator<'a> {
    /// Start a generation for `project`, with default output root and driver.
    pub fn new(project: &'a Project) -> Self {
        Self {
            project,
            out: PathBuf::from(DEFAULT_OUT),
            tag: None,
            no_build: false,
            driver: DEFAULT_DRIVER.to_owned(),
            targets: Vec::new(),
            build_driver: None,
        }
    }

    /// Inject a build driver, overriding the default cargo command. Lets a
    /// library consumer or test supply a custom [`BuildDriver`].
    pub fn build_driver(mut self, driver: &'a dyn BuildDriver) -> Self {
        self.build_driver = Some(driver);
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
    pub fn targets(mut self, targets: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.targets = targets.into_iter().map(Into::into).collect();
        self
    }

    /// Run the full pipeline.
    pub fn run(&self) -> Result<()> {
        let project = self.project;

        if let Some(tag) = &self.tag {
            let expected = format!("{TAG_PREFIX}{}", project.version);
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
            let cargo = CargoDriver::new(&self.driver);
            let driver: &dyn BuildDriver = match self.build_driver {
                Some(injected) => injected,
                None => &cargo,
            };
            Compiler::new(driver).compile_all(project, &targets)?;
        }
        Assembler::new(project, &targets, &self.out).assemble()?;

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
