use std::process::Command;

use tracing::info;

use crate::project::Project;
use crate::target::Target;

use super::{BuildDriver, CompileError};

/// Builds with a cargo-compatible command (cargo, cross, cargo-zigbuild). It runs
/// `<command> build --release --target <triple> [--package <pkg>] --bin <bin>` in
/// the workspace root. The `--release` flag must keep agreeing with the
/// `release` artifact directory the assembly phase reads from.
#[derive(Debug)]
pub struct CargoDriver<'a> {
    command: &'a str,
}

impl<'a> CargoDriver<'a> {
    pub fn new(command: &'a str) -> Self {
        Self { command }
    }
}

impl BuildDriver for CargoDriver<'_> {
    fn build(&self, project: &Project, target: &Target) -> Result<(), CompileError> {
        if !project.workspace_root.is_dir() {
            return Err(CompileError::MissingWorkspaceRoot {
                path: project.workspace_root.clone(),
            });
        }
        info!(triple = %target.triple, bin = %project.bin, "building");
        let mut command = Command::new(self.command);
        command
            .args(["build", "--release", "--target"])
            .arg(&target.triple);
        if let Some(package) = &project.package {
            command.arg("--package").arg(package);
        }
        command
            .arg("--bin")
            .arg(&project.bin)
            .current_dir(&project.workspace_root);

        let status = command.status().map_err(|source| CompileError::Spawn {
            driver: self.command.to_owned(),
            source,
        })?;
        if !status.success() {
            return Err(CompileError::BuildFailed {
                triple: target.triple.clone(),
                status,
            });
        }
        Ok(())
    }
}
