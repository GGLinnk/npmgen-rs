//! Phases 2 and 3: assemble the publish tree and place the binaries.
//!
//! The [`Assembler`] writes the meta `package.json`, renders foreign manifests,
//! copies launcher and `include` payload, then writes each platform
//! `package.json` and copies its binary out of the target directory.

mod meta;
mod platform;
mod substitute;
mod writer;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use meta::MetaPackage;
use platform::PlatformPackage;
use substitute::{ManifestRenderer, RenderedManifest};
use writer::TreeWriter;

use crate::project::Project;
use crate::target::Target;

/// Assembles the full publish tree for a project.
pub struct Assembler<'a> {
    project: &'a Project,
    targets: &'a [Target],
    out: &'a Path,
    require_binaries: bool,
    variables: BTreeMap<String, String>,
}

impl<'a> Assembler<'a> {
    /// With `require_binaries`, a missing platform binary is fatal; otherwise
    /// (e.g. `--no-build`) it is a warning and the platform package is still
    /// written.
    pub fn new(
        project: &'a Project,
        targets: &'a [Target],
        out: &'a Path,
        require_binaries: bool,
    ) -> Self {
        Self {
            variables: project.variables(),
            project,
            targets,
            out,
            require_binaries,
        }
    }

    /// Write the whole tree under `out`.
    pub fn assemble(&self) -> Result<(), NpmError> {
        self.assemble_meta()?;
        self.assemble_platforms()
    }

    fn assemble_meta(&self) -> Result<(), NpmError> {
        let writer = TreeWriter::new(self.out.join(&self.project.identity.name));
        writer.ensure()?;
        writer.write_json(
            "package.json",
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
            writer.copy_file(
                &self.project.workspace_root.join(launcher.file()),
                launcher.file(),
            )?;
        }

        for include in &self.project.config.include {
            let from = self.project.workspace_root.join(include);
            if !writer.copy_path(&from, include)? {
                warn!(path = %from.display(), "include path not found; skipped");
            }
        }
        Ok(())
    }

    fn assemble_platforms(&self) -> Result<(), NpmError> {
        let name = &self.project.identity.name;
        for target in self.targets {
            let writer = TreeWriter::new(self.out.join(format!("{name}-{}", target.key)));
            writer.ensure()?;
            writer.write_json(
                "package.json",
                &PlatformPackage::new(self.project, target).to_value(),
            )?;

            let from = target.binary_path(&self.project.target_directory, &self.project.bin);
            let dest = target.binary_filename(name);
            if !writer.copy_path(&from, &dest)? {
                if self.require_binaries {
                    return Err(NpmError::BinaryMissing {
                        triple: target.triple.clone(),
                        path: from,
                    });
                }
                warn!(path = %from.display(), "binary not found; platform package has no binary");
            }
        }
        Ok(())
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

    #[error("binary not found for {triple}: {}", path.display())]
    BinaryMissing { triple: String, path: PathBuf },
}
