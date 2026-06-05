//! Phase 1: cross-build the binary for each target.
//!
//! [`Compiler`] drives an injected [`BuildDriver`] (the default [`CargoDriver`]
//! shells out to cargo) and verifies each artifact lands where the assembly
//! phase looks for it. Swapping the driver makes the build phase mockable and
//! lets a non-cargo backend slot in without bundling a cross-compiler.

mod cargo;
mod driver;

pub use cargo::CargoDriver;
pub use driver::BuildDriver;

use std::path::PathBuf;
use std::process::ExitStatus;

use crate::project::Project;
use crate::target::Target;

/// Builds target binaries through an injected [`BuildDriver`].
#[derive(Debug)]
pub struct Compiler<'a> {
    driver: &'a dyn BuildDriver,
}

impl<'a> Compiler<'a> {
    pub fn new(driver: &'a dyn BuildDriver) -> Self {
        Self { driver }
    }

    /// Build every target, verifying each artifact exists where the assembly
    /// phase will look for it.
    pub fn compile_all(&self, project: &Project, targets: &[Target]) -> Result<(), CompileError> {
        for target in targets {
            self.driver.build(project, target)?;
            let path = target.binary_path(&project.target_directory, &project.bin);
            if !path.is_file() {
                return Err(CompileError::BinaryMissing {
                    triple: target.triple.clone(),
                    path,
                });
            }
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

    #[error("binary for {triple} not found after a successful build: {}", path.display())]
    BinaryMissing { triple: String, path: PathBuf },

    #[error("build driver {driver:?} must be a bare command name on PATH, not a path")]
    InvalidDriver { driver: String },

    #[error("workspace root does not exist: {}", path.display())]
    MissingWorkspaceRoot { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::{BuildDriver, CompileError, Compiler};
    use crate::project::{Project, sample_project};
    use crate::target::Target;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("npmgen-compile-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Reports success without producing any artifact.
    #[derive(Debug)]
    struct NoopDriver;
    impl BuildDriver for NoopDriver {
        fn build(&self, _project: &Project, _target: &Target) -> Result<(), CompileError> {
            Ok(())
        }
    }

    /// Writes the artifact where the assembly phase expects it.
    #[derive(Debug)]
    struct PlacingDriver;
    impl BuildDriver for PlacingDriver {
        fn build(&self, project: &Project, target: &Target) -> Result<(), CompileError> {
            let path = target.binary_path(&project.target_directory, &project.bin);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"binary").unwrap();
            Ok(())
        }
    }

    #[test]
    fn missing_binary_after_a_successful_build_is_an_error() {
        let mut project = sample_project();
        project.target_directory = scratch("missing");
        project.bin = "tool".to_owned();
        let target = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();

        let error = Compiler::new(&NoopDriver)
            .compile_all(&project, std::slice::from_ref(&target))
            .unwrap_err();
        assert!(matches!(error, CompileError::BinaryMissing { .. }));
        let _ = fs::remove_dir_all(&project.target_directory);
    }

    #[test]
    fn verifies_a_placed_binary() {
        let mut project = sample_project();
        project.target_directory = scratch("placed");
        project.bin = "tool".to_owned();
        let target = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();

        Compiler::new(&PlacingDriver)
            .compile_all(&project, std::slice::from_ref(&target))
            .unwrap();
        let _ = fs::remove_dir_all(&project.target_directory);
    }

    #[test]
    fn a_missing_workspace_root_is_an_error() {
        use super::CargoDriver;
        let mut project = sample_project();
        project.workspace_root = PathBuf::from("npmgen-nonexistent-workspace-root-xyz");
        let target = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();
        let error = CargoDriver::new("cargo")
            .build(&project, &target)
            .unwrap_err();
        assert!(matches!(error, CompileError::MissingWorkspaceRoot { .. }));
    }
}
