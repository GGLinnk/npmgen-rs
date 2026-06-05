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
use std::sync::atomic::{AtomicU64, Ordering};

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
/// Suffix of the sibling directory the previous tree is moved to during a swap.
const ASIDE_SUFFIX: &str = ".npmgen-old";

/// Monotonic counter making each set-aside directory name unique within the
/// process, so the swap never renames onto an existing path.
static SWAP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Builds the publish tree for one or more projects in a sibling staging
/// directory, then atomically swaps it onto `out`. Add each project, then
/// `commit`; dropping without committing discards the staging directory, so a
/// run is all-or-nothing and a re-run never leaves a previous tree behind.
#[derive(Debug)]
pub struct Assembler<'a> {
    out: &'a Path,
    staging: PathBuf,
    committed: bool,
}

impl<'a> Assembler<'a> {
    /// Prepare a fresh staging directory for `out`.
    pub fn new(out: &'a Path) -> Result<Self, NpmError> {
        let staging = Self::staging_dir(out)?;
        Self::reset(&staging)?;
        Ok(Self {
            out,
            staging,
            committed: false,
        })
    }

    /// Write one project's package tree into staging. Returns the `<name>-<key>`
    /// of every target whose binary was not present to copy.
    pub fn add(&self, project: &Project, targets: &[Target]) -> Result<Vec<String>, NpmError> {
        let variables = project.variables();
        self.write_meta(project, targets, &variables)?;
        self.write_platforms(project, targets)
    }

    /// Atomically replace `out` with the staged tree.
    ///
    /// The existing tree (if any) is first renamed aside to a unique sibling,
    /// then staging is renamed into the now-free path, then the set-aside tree
    /// is removed best-effort. This avoids the Windows "remove then rename onto
    /// the same path" race: there, the directory deletion is asynchronous, so
    /// the destination is briefly still occupied and the rename fails. Renaming
    /// aside is synchronous and targets a fresh name, so no such window exists.
    /// If the swap fails after the set-aside, the previous tree is restored, so
    /// a failed commit is never data loss.
    pub fn commit(mut self) -> Result<(), NpmError> {
        let aside = Self::aside_dir(self.out);
        let had_previous = match std::fs::rename(self.out, &aside) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(NpmError::Swap {
                    from: self.out.to_path_buf(),
                    to: aside,
                    source,
                });
            }
        };

        if let Err(source) = std::fs::rename(&self.staging, self.out) {
            if had_previous {
                let _ = std::fs::rename(&aside, self.out);
            }
            return Err(NpmError::Swap {
                from: self.staging.clone(),
                to: self.out.to_path_buf(),
                source,
            });
        }

        self.committed = true;
        if had_previous {
            let _ = std::fs::remove_dir_all(&aside);
        }
        Ok(())
    }

    fn write_meta(
        &self,
        project: &Project,
        targets: &[Target],
        variables: &BTreeMap<String, String>,
    ) -> Result<(), NpmError> {
        let writer = TreeWriter::new(self.staging.join(&project.identity.name));
        writer.ensure()?;
        writer.write_json(PACKAGE_JSON, &MetaPackage::new(project, targets).to_value())?;

        let renderer = ManifestRenderer::new(variables);
        for manifest in &project.config.manifests {
            TreeWriter::guard(manifest.src())?;
            let src = project.workspace_root.join(manifest.src());
            TreeWriter::reject_symlink(&src)?;
            match renderer.render(&src)? {
                RenderedManifest::Json(value) => writer.write_json(manifest.dest(), &value)?,
                RenderedManifest::Toml(text) => writer.write_string(manifest.dest(), &text)?,
            }
        }

        if let Some(launcher) = &project.config.launcher {
            let dest = launcher.output();
            if launcher.is_generated() {
                writer.write_string(dest, &LauncherScript::new(launcher.fail_open()).render())?;
            } else {
                writer.copy_file(&project.workspace_root.join(dest), dest)?;
            }
        }

        for include in &project.config.include {
            let from = project.workspace_root.join(include);
            if !writer.copy_path(&from, include)? {
                warn!(path = %from.display(), "include path not found; skipped");
            }
        }
        Ok(())
    }

    fn write_platforms(
        &self,
        project: &Project,
        targets: &[Target],
    ) -> Result<Vec<String>, NpmError> {
        let name = &project.identity.name;
        let mut missing = Vec::new();
        for target in targets {
            let writer = TreeWriter::new(self.staging.join(format!("{name}-{}", target.key)));
            writer.ensure()?;
            writer.write_json(
                PACKAGE_JSON,
                &PlatformPackage::new(project, target).to_value(),
            )?;

            let from = target.binary_path(&project.target_directory, &project.bin);
            let dest = target.binary_filename(name);
            if !writer.copy_path(&from, &dest)? {
                missing.push(format!("{name}-{}", target.key));
            }
        }
        Ok(missing)
    }

    /// A genuine sibling of `out` (same parent, so the swap is a cheap rename),
    /// suffixed with the process id so concurrent runs do not collide. Errors
    /// when `out` has no final component (`.`, `..`, a root), which would make
    /// the pre-swap reset delete the wrong directory.
    fn staging_dir(out: &Path) -> Result<PathBuf, NpmError> {
        let file_name = out.file_name().ok_or_else(|| NpmError::InvalidOut {
            path: out.to_path_buf(),
        })?;
        if out
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(NpmError::OutEscape {
                path: out.to_path_buf(),
            });
        }
        let mut staged = file_name.to_os_string();
        staged.push(format!("{STAGING_SUFFIX}{}", std::process::id()));
        Ok(match out.parent() {
            Some(parent) => parent.join(staged),
            None => PathBuf::from(staged),
        })
    }

    /// A unique sibling of `out` that holds the previous tree during a swap.
    /// Keyed by process id and a monotonic counter, so it never names a
    /// directory that already exists. Mirrors [`Self::staging_dir`]; `out` is
    /// already known to have a final component (validated when staging was set
    /// up), so a missing one degrades to a bare name rather than erroring.
    fn aside_dir(out: &Path) -> PathBuf {
        let seq = SWAP_SEQ.fetch_add(1, Ordering::Relaxed);
        let mut name = out.file_name().unwrap_or_default().to_os_string();
        name.push(format!("{ASIDE_SUFFIX}{}-{seq}", std::process::id()));
        match out.parent() {
            Some(parent) => parent.join(name),
            None => PathBuf::from(name),
        }
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

impl Drop for Assembler<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.staging);
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

    #[error("output path {} must not contain \"..\"", path.display())]
    OutEscape { path: PathBuf },

    #[error("refusing to follow symlink {}", path.display())]
    Symlink { path: PathBuf },

    #[error("manifest {} is {size} bytes, over the {max}-byte limit", path.display())]
    ManifestTooLarge { path: PathBuf, size: u64, max: u64 },

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

#[cfg(test)]
mod tests {
    use super::{Assembler, NpmError};
    use crate::config::ManifestSpec;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("npmgen-assemble-{}-{tag}", std::process::id()))
    }

    #[test]
    fn output_path_with_parent_dir_is_rejected() {
        assert!(matches!(
            Assembler::new(Path::new("../escape")).unwrap_err(),
            NpmError::OutEscape { .. }
        ));
    }

    #[test]
    fn manifest_source_escaping_the_workspace_is_rejected() {
        let mut project = crate::project::sample_project();
        project.config.manifests = vec![ManifestSpec::Path("../secret.json".to_owned())];

        let out = scratch("manifest-escape");
        let assembler = Assembler::new(&out).unwrap();
        let error = assembler.add(&project, &[]).unwrap_err();
        assert!(matches!(error, NpmError::PathEscape { .. }));
    }
}
