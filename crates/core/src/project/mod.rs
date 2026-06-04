//! The target crate, resolved via `cargo metadata`.
//!
//! Identity (version, description, author, repository, license) is taken from
//! the selected package with workspace inheritance applied by cargo. When no
//! package is selected (a virtual workspace root), it falls back to
//! `[workspace.package]` read from the workspace `Cargo.toml`. The `npmgen`
//! configuration is read from the package's `metadata.npmgen`, else from
//! `[workspace.metadata.npmgen]`.

mod author;
mod identity;

pub use author::Author;
pub use identity::Identity;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{MetadataCommand, Package};

use crate::config::Config;

/// Identity overrides supplied on the command line. Each `Some` wins over the
/// value read from the manifest.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Workspace package to describe and build.
    pub package: Option<String>,
    /// Cargo bin name shipped in platform packages.
    pub bin: Option<String>,
    /// Package version.
    pub version: Option<String>,
}

/// Everything the pipeline needs about the target crate.
#[derive(Debug, Clone)]
pub struct Project {
    pub identity: Identity,
    pub version: String,
    pub description: String,
    pub author: Author,
    pub license: String,
    pub repository: String,
    /// Cargo bin name to build and ship.
    pub bin: String,
    pub config: Config,
    pub workspace_root: PathBuf,
    pub target_directory: PathBuf,
}

/// Failures loading and resolving the target crate.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("running `cargo metadata`")]
    Metadata {
        #[source]
        source: cargo_metadata::Error,
    },

    #[error("no workspace package named {name:?}")]
    PackageNotFound { name: String },

    #[error("[workspace.package] repository must be set to https://<host>/<owner>/<repo>")]
    MissingRepository,

    #[error("no version found; set it in Cargo.toml or pass --pkg-version")]
    MissingVersion,

    #[error("reading manifest {}", path.display())]
    ReadManifest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing manifest {}", path.display())]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
}

impl Project {
    /// Load and resolve the crate at `manifest_path`, applying `overrides`.
    pub fn load(manifest_path: &Path, overrides: &Overrides) -> Result<Self, ProjectError> {
        let metadata = MetadataCommand::new()
            .manifest_path(manifest_path)
            .exec()
            .map_err(|source| ProjectError::Metadata { source })?;

        let workspace_root = metadata.workspace_root.as_std_path().to_path_buf();
        let target_directory = metadata.target_directory.as_std_path().to_path_buf();
        let workspace_package = WorkspacePackage::read(&workspace_root)?;

        let selected = Self::select_package(&metadata, overrides.package.as_deref())?;

        let npmgen_value = selected
            .and_then(|package| package.metadata.get("npmgen"))
            .or_else(|| metadata.workspace_metadata.get("npmgen"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let config = Config::from_metadata(&npmgen_value)?;

        let version = overrides
            .version
            .clone()
            .or_else(|| selected.map(|package| package.version.to_string()))
            .or_else(|| workspace_package.version.clone())
            .ok_or(ProjectError::MissingVersion)?;

        let description = selected
            .and_then(|package| package.description.clone())
            .or_else(|| workspace_package.description.clone())
            .unwrap_or_default();

        let author_full = selected
            .and_then(|package| package.authors.first().cloned())
            .or_else(|| workspace_package.author.clone())
            .unwrap_or_default();

        let repository = selected
            .and_then(|package| package.repository.clone())
            .or_else(|| workspace_package.repository.clone())
            .ok_or(ProjectError::MissingRepository)?;

        let license = config
            .license
            .clone()
            .or_else(|| selected.and_then(|package| package.license.clone()))
            .or_else(|| workspace_package.license.clone())
            .unwrap_or_default();

        let identity = Identity::from_repository(&repository, config.scope.as_deref())?;

        let bin = overrides
            .bin
            .clone()
            .or_else(|| config.bin.clone())
            .unwrap_or_else(|| identity.name.clone());

        Ok(Self {
            author: Author::parse(&author_full),
            version,
            description,
            license,
            repository,
            bin,
            identity,
            config,
            workspace_root,
            target_directory,
        })
    }

    /// `@scope/name` meta package name.
    pub fn package_name(&self) -> String {
        format!("{}/{}", self.identity.scope, self.identity.name)
    }

    /// Identity values exposed to foreign-manifest substitution.
    pub fn variables(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("name".to_owned(), self.identity.name.clone()),
            ("scope".to_owned(), self.identity.scope.clone()),
            ("package".to_owned(), self.package_name()),
            ("version".to_owned(), self.version.clone()),
            ("description".to_owned(), self.description.clone()),
            ("license".to_owned(), self.license.clone()),
            ("repository".to_owned(), self.repository.clone()),
            ("git_url".to_owned(), self.identity.git_url.clone()),
            ("bin".to_owned(), self.bin.clone()),
            ("author".to_owned(), self.author.full.clone()),
            ("author_name".to_owned(), self.author.name.clone()),
            (
                "author_email".to_owned(),
                self.author.email.clone().unwrap_or_default(),
            ),
        ])
    }

    /// Select the package whose identity and config drive generation: an
    /// explicit `--package`, else the workspace root package, else none (a
    /// virtual workspace).
    fn select_package<'a>(
        metadata: &'a cargo_metadata::Metadata,
        package: Option<&str>,
    ) -> Result<Option<&'a Package>, ProjectError> {
        match package {
            Some(name) => metadata
                .workspace_packages()
                .into_iter()
                .find(|package| package.name.as_str() == name)
                .map(Some)
                .ok_or_else(|| ProjectError::PackageNotFound {
                    name: name.to_owned(),
                }),
            None => Ok(metadata.root_package()),
        }
    }
}

/// The literal `[workspace.package]` fields, the identity source for a virtual
/// workspace root that no member inherits from.
#[derive(Debug, Default)]
struct WorkspacePackage {
    version: Option<String>,
    description: Option<String>,
    repository: Option<String>,
    license: Option<String>,
    author: Option<String>,
}

impl WorkspacePackage {
    fn read(workspace_root: &Path) -> Result<Self, ProjectError> {
        let path = workspace_root.join("Cargo.toml");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ProjectError::ReadManifest { path, source }),
        };
        let value: toml::Value =
            toml::from_str(&text).map_err(|source| ProjectError::ParseManifest { path, source })?;

        let Some(package) = value.get("workspace").and_then(|ws| ws.get("package")) else {
            return Ok(Self::default());
        };
        let string = |key: &str| {
            package
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
        };
        Ok(Self {
            version: string("version"),
            description: string("description"),
            repository: string("repository"),
            license: string("license"),
            author: package
                .get("authors")
                .and_then(toml::Value::as_array)
                .and_then(|authors| authors.first())
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
        })
    }
}
