//! Phase 1: cross-build the binary for each target.
//!
//! The [`Compiler`] orchestrates a build driver (cargo by default) rather than
//! bundling a cross-compiler: it invokes `<driver> build --release --target
//! <triple>`, so pointing the driver at `cross`/`cargo-zigbuild` works with no
//! code change. Toolchain installation remains the caller's responsibility.

use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use tracing::info;

use crate::project::Project;
use crate::target::Target;

/// Builds target binaries with a configurable build driver.
pub struct Compiler {
    driver: String,
}

impl Compiler {
    /// Create a compiler driven by `driver` (e.g. `cargo`, `cross`).
    pub fn new(driver: impl Into<String>) -> Self {
        Self {
            driver: driver.into(),
        }
    }

    /// Build every target, verifying each artifact lands where the assembly
    /// phase will look for it.
    pub fn compile_all(&self, project: &Project, targets: &[Target]) -> Result<(), CompileError> {
        for target in targets {
            self.compile_one(project, target)?;
        }
        Ok(())
    }

    fn compile_one(&self, project: &Project, target: &Target) -> Result<(), CompileError> {
        info!(triple = %target.triple, bin = %project.bin, "building");
        let status = Command::new(&self.driver)
            .args(["build", "--release", "--target"])
            .arg(&target.triple)
            .arg("--bin")
            .arg(&project.bin)
            .current_dir(&project.workspace_root)
            .status()
            .map_err(|source| CompileError::Spawn {
                driver: self.driver.clone(),
                source,
            })?;
        if !status.success() {
            return Err(CompileError::BuildFailed {
                triple: target.triple.clone(),
                status,
            });
        }

        let path = target.binary_path(&project.target_directory, &project.bin);
        if !path.is_file() {
            return Err(CompileError::BinaryMissing {
                triple: target.triple.clone(),
                path,
            });
        }
        Ok(())
    }
}

/// Failures while building target binaries.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("spawning build driver {driver:?}")]
    Spawn {
        driver: String,
        #[source]
        source: std::io::Error,
    },

    #[error("build for {triple} failed: {status}")]
    BuildFailed { triple: String, status: ExitStatus },

    #[error("binary not found after a successful build: {}", path.display())]
    BinaryMissing { triple: String, path: PathBuf },
}
