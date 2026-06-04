//! Phases 2 and 3: assemble the publish tree and place the binaries.
//!
//! The [`Assembler`] builds the whole tree in a sibling staging directory and
//! swaps it onto `out` only once complete, so a run is all-or-nothing and a
//! re-run never leaves orphaned files from a previous (differently-targeted)
//! tree. Each platform's binary is copied out of cargo's target directory;
//! platforms whose binary is not yet present are reported in one summary.

mod launcher;
mod meta;
mod platform;
mod substitute;
mod writer;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use launcher::LauncherScript;
use meta::MetaPackage;
use platform::PlatformPackage;
use substitute::{ManifestRenderer, RenderedManifest};
use writer::TreeWriter;

use crate::project::Project;
use crate::target::Target;

/// Manifest file name written in every package directory.
const PACKAGE_JSON: &str = "package.json";
/// Suffix of the sibling staging directory assembled before the atomic swap.
const STAGING_SUFFIX: &str = ".npmgen-staging";

/// Assembles the full publish tree for a project.
#[derive(Debug)]
pub struct Assembler<'a> {
    project: &'a Project,
    targets: &'a [Target],
    out: &'a Path,
    variables: BTreeMap<String, String>,
}

impl<'a> Assembler<'a> {
    pub fn new(project: &'a Project, targets: &'a [Target], out: &'a Path) -> Self {
        Self {
            variables: project.variables(),
            project,
            targets,
            out,
        }
    }

    /// Build the tree in staging and atomically swap it onto `out`. On any
    /// failure the staging directory is removed rather than left behind.
    pub fn assemble(&self) -> Result<(), NpmError> {
        let staging = self.staging_dir()?;
        Self::reset(&staging)?;
        match self.assemble_into(&staging) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = Self::reset(&staging);
                Err(error)
            }
        }
    }

    fn assemble_into(&self, staging: &Path) -> Result<(), NpmError> {
        self.assemble_meta(staging)?;
        let missing = self.assemble_platforms(staging)?;
        self.swap(staging)?;

        if !missing.is_empty() {
            warn!(
                placed = self.targets.len() - missing.len(),
                total = self.targets.len(),
                missing = ?missing,
                "platform packages have no binary yet; place them before publishing",
            );
        }
        Ok(())
    }

    fn assemble_meta(&self, staging: &Path) -> Result<(), NpmError> {
        let writer = TreeWriter::new(staging.join(&self.project.identity.name));
        writer.ensure()?;
        writer.write_json(
            PACKAGE_JSON,
            &MetaPackage::new(self.project, self.targets).to_value(),
        )?;

        let renderer = ManifestRenderer::new(&self.variables);
        for manifest in &self.project.config.manifests {
            let src = self.project.workspace_root.join(manifest.src());
            match renderer.render(&src)? {
                RenderedManifest::Json(value) => writer.write_json(manifest.dest(), &value)?,
                RenderedManifest::Toml(text) => writer.write_string(manifest.dest(), &text)?,
            }
        }

        if let Some(launcher) = &self.project.config.launcher {
            let dest = launcher.output();
            if launcher.is_generated() {
                let script = LauncherScript::new(launcher.fail_open()).render();
                writer.write_string(dest, &script)?;
            } else {
                writer.copy_file(&self.project.workspace_root.join(dest), dest)?;
            }
        }

        for include in &self.project.config.include {
            let from = self.project.workspace_root.join(include);
            if !writer.copy_path(&from, include)? {
                warn!(path = %from.display(), "include path not found; skipped");
            }
        }
        Ok(())
    }

    /// Returns the keys of targets whose binary was not present to copy.
    fn assemble_platforms(&self, staging: &Path) -> Result<Vec<String>, NpmError> {
        let name = &self.project.identity.name;
        let mut missing = Vec::new();
        for target in self.targets {
            let writer = TreeWriter::new(staging.join(format!("{name}-{}", target.key)));
            writer.ensure()?;
            writer.write_json(
                PACKAGE_JSON,
                &PlatformPackage::new(self.project, target).to_value(),
            )?;

            let from = target.binary_path(&self.project.target_directory, &self.project.bin);
            let dest = target.binary_filename(name);
            if !writer.copy_path(&from, &dest)? {
                missing.push(target.key.clone());
            }
        }
        Ok(missing)
    }

    /// A genuine sibling of `out` (same parent, so the swap is a cheap rename),
    /// suffixed with the process id so concurrent runs do not collide. Errors
    /// when `out` has no final component (`.`, `..`, a root), which would make
    /// the pre-swap reset delete the wrong directory.
    fn staging_dir(&self) -> Result<PathBuf, NpmError> {
        let file_name = self.out.file_name().ok_or_else(|| NpmError::InvalidOut {
            path: self.out.to_path_buf(),
        })?;
        let mut staged = file_name.to_os_string();
        staged.push(format!("{STAGING_SUFFIX}{}", std::process::id()));
        Ok(match self.out.parent() {
            Some(parent) => parent.join(staged),
            None => PathBuf::from(staged),
        })
    }

    fn swap(&self, staging: &Path) -> Result<(), NpmError> {
        Self::reset(self.out)?;
        std::fs::rename(staging, self.out).map_err(|source| NpmError::Swap {
            from: staging.to_path_buf(),
            to: self.out.to_path_buf(),
            source,
        })
    }

    fn reset(path: &Path) -> Result<(), NpmError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(NpmError::Remove {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

/// Failures while assembling the tree or placing binaries.
#[derive(Debug, thiserror::Error)]
pub enum NpmError {
    #[error("creating directory {}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("writing {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("reading {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("listing directory {}", path.display())]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("copying {} to {}", from.display(), to.display())]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("removing {}", path.display())]
    Remove {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("swapping {} onto {}", from.display(), to.display())]
    Swap {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("payload path {path:?} escapes the package directory")]
    PathEscape { path: String },

    #[error("output path {} has no final component to write into (e.g. \".\" or a root)", path.display())]
    InvalidOut { path: PathBuf },

    #[error("serializing JSON for {}", path.display())]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("parsing JSON manifest {}", path.display())]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("parsing TOML manifest {}", path.display())]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("serializing TOML manifest {}", path.display())]
    SerializeToml {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },

    #[error("manifest {} has no supported extension (.json, .toml)", path.display())]
    UnsupportedManifestFormat { path: PathBuf },

    #[error("unknown variable ${{{name}}} in manifest {}", path.display())]
    UnknownVariable { name: String, path: PathBuf },

    #[error("unterminated ${{...}} placeholder in manifest {}", path.display())]
    UnterminatedPlaceholder { path: PathBuf },
}
