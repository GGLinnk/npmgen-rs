//! The generation pipeline over a resolved [`Project`]: check the tag, resolve
//! targets, build (unless skipped), assemble the tree.
//!
//! Acquiring the project is a separate concern: build one in memory with
//! [`Project::builder`](crate::Project::builder) or load it from a manifest with
//! [`Project::load`](crate::Project::load). The engine itself reads no manifest.

use std::path::PathBuf;

use tracing::{info, warn};

use crate::compile::{BuildDriver, CargoDriver, CompileError, Compiler};
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

/// Generates the publish tree for one or more [`Project`]s into a shared output
/// root, atomically. Construct with [`Generator::new`] (single) or
/// [`Generator::for_projects`] (a workspace's bins) and configure with the
/// chained setters.
#[derive(Debug)]
pub struct Generator<'a> {
    projects: &'a [Project],
    out: PathBuf,
    tag: Option<String>,
    no_build: bool,
    driver: String,
    targets: Vec<String>,
    build_driver: Option<&'a dyn BuildDriver>,
}

impl<'a> Generator<'a> {
    /// Start a generation for a single project.
    pub fn new(project: &'a Project) -> Self {
        Self::for_projects(std::slice::from_ref(project))
    }

    /// Start a generation for several projects (e.g. one per workspace bin)
    /// sharing one output root; they are assembled and swapped together.
    pub fn for_projects(projects: &'a [Project]) -> Self {
        Self {
            projects,
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

    /// Build (unless skipped) and assemble every project, then atomically swap
    /// the whole tree onto `out`. Either all projects land or none do.
    pub fn run(&self) -> Result<()> {
        let assembler = Assembler::new(&self.out)?;
        if !self.no_build && self.build_driver.is_none() {
            validate_driver(&self.driver)?;
        }
        let mut total_targets = 0;
        let mut missing = Vec::new();

        for project in self.projects {
            if let Some(tag) = &self.tag {
                let expected = format!("{TAG_PREFIX}{}", project.version);
                if tag != &expected {
                    return Err(Error::TagMismatch {
                        tag: tag.clone(),
                        expected,
                    });
                }
            }

            let targets = TargetResolver::new(&project.config, &project.workspace_root)
                .resolve(&self.targets)?;

            if !self.no_build {
                let cargo = CargoDriver::new(&self.driver);
                let driver: &dyn BuildDriver = match self.build_driver {
                    Some(injected) => injected,
                    None => &cargo,
                };
                Compiler::new(driver).compile_all(project, &targets)?;
            }

            total_targets += targets.len();
            missing.extend(assembler.add(project, &targets)?);
        }

        assembler.commit()?;

        if !missing.is_empty() {
            warn!(
                placed = total_targets - missing.len(),
                total = total_targets,
                missing = ?missing,
                "platform packages have no binary yet; place them before publishing",
            );
        }
        info!(
            packages = self.projects.len(),
            out = %self.out.display(),
            "generated npm publish tree",
        );
        Ok(())
    }
}

/// Reject a build driver that is a path rather than a bare command name, so a
/// crafted `--builder` cannot point the build at an arbitrary binary; the
/// command is resolved through `PATH` like any cargo subcommand.
fn validate_driver(driver: &str) -> Result<()> {
    if driver.is_empty() || driver.contains('/') || driver.contains('\\') {
        return Err(CompileError::InvalidDriver {
            driver: driver.to_owned(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_driver;
    use crate::error::Error;

    #[test]
    fn bare_command_drivers_are_accepted() {
        assert!(validate_driver("cargo").is_ok());
        assert!(validate_driver("cargo-zigbuild").is_ok());
        assert!(validate_driver("cross").is_ok());
    }

    #[test]
    fn path_like_or_empty_drivers_are_rejected() {
        for bad in ["", "/tmp/evil", "../evil", "a/b", "a\\b"] {
            assert!(matches!(
                validate_driver(bad).unwrap_err(),
                Error::Compile(_)
            ));
        }
    }
}
