//! The target crate(s), resolved via `cargo metadata`.
//!
//! npmgen mirrors cargo: it publishes the binaries cargo would build, one npm
//! package per binary, named after the binary. Identity (version, description,
//! author, repository, license) comes from each package with `[workspace.package]`
//! inheritance already applied by cargo. npmgen-specific settings live in
//! `[package.metadata.npmgen]`, inheriting `[workspace.metadata.npmgen]` the way
//! cargo inherits `[workspace.package]`.

mod author;
mod builder;
mod identity;
mod workspace;

pub use author::Author;
pub use builder::ProjectBuilder;
pub use identity::Identity;
pub use workspace::Workspace;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cargo_metadata::Package;

use crate::config::Config;

/// Default manifest path for [`Project::discover`].
pub const DEFAULT_MANIFEST_PATH: &str = "Cargo.toml";

/// Metadata table key under `[package.metadata.*]` / `[workspace.metadata.*]`.
pub(crate) const METADATA_KEY: &str = "npmgen";

/// Command-line selection, mirroring `cargo`'s package/target flags. Empty
/// vectors mean "no restriction"; the defaults match `cargo build`.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// `-p/--package`: restrict to these workspace members (repeatable).
    pub packages: Vec<String>,
    /// `--workspace`: select every workspace member.
    pub workspace: bool,
    /// `--exclude`: drop these members from the selection (repeatable).
    pub exclude: Vec<String>,
    /// `--bin`: restrict to these binaries (repeatable).
    pub bins: Vec<String>,
    /// Override the package version for every selected bin.
    pub version: Option<String>,
}

/// Everything the pipeline needs to ship one binary as an npm package.
#[derive(Debug, Clone)]
pub struct Project {
    pub identity: Identity,
    pub version: String,
    pub description: String,
    pub author: Author,
    pub license: String,
    pub repository: String,
    /// Cargo bin name to build and ship; also the npm package name.
    pub bin: String,
    /// Owning cargo package, passed as `--package` to the build.
    pub package: Option<String>,
    pub config: Config,
    pub workspace_root: PathBuf,
    pub target_directory: PathBuf,
}

/// Failures loading and resolving the target crate(s).
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("running `cargo metadata`")]
    Metadata {
        #[source]
        source: Box<cargo_metadata::Error>,
    },

    #[error("no workspace package named {name:?}")]
    PackageNotFound { name: String },

    #[error("package repository must be set to https://<host>/<owner>/<repo>")]
    MissingRepository,

    #[error("package {package:?} has no bin named {bin:?}")]
    UnknownBin { package: String, bin: String },

    #[error("no workspace bin named {bin:?}")]
    BinNotInWorkspace { bin: String },

    #[error("bin {bin:?} is defined by more than one package ({}); select one with --package", packages.join(", "))]
    AmbiguousBin { bin: String, packages: Vec<String> },

    #[error(
        "nothing to publish: no selected package ships a binary (every match is a library or `publish = false`)"
    )]
    NothingToPublish,

    #[error(
        "{count} binaries match; narrow with --package/--bin, or use Project::discover for the full set"
    )]
    NotSingle { count: usize },

    #[error("invalid project field {field}: {reason}")]
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
}

impl Project {
    /// Construct a project programmatically, with no `Cargo.toml`, `cargo
    /// metadata`, or TOML parsing. The scope, name and version are required.
    pub fn builder(
        scope: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> ProjectBuilder {
        ProjectBuilder::new(scope, name, version)
    }

    /// Resolve every publishable binary at `manifest_path` into a project.
    ///
    /// Selection mirrors `cargo build`: by default the workspace's
    /// default-members (or all members), each member's binaries, skipping
    /// libraries and `publish = false` crates. `--package`/`--workspace`/
    /// `--exclude`/`--bin` narrow the set exactly as cargo's flags do. Each
    /// binary becomes one npm package named after the binary.
    pub fn discover(
        manifest_path: &Path,
        overrides: &Overrides,
    ) -> Result<Vec<Self>, ProjectError> {
        let projects = Workspace::load(manifest_path)?.projects(overrides)?;
        if projects.is_empty() {
            return Err(ProjectError::NothingToPublish);
        }
        Ok(projects)
    }

    /// Resolve a single binary at `manifest_path`. A convenience over
    /// [`discover`](Self::discover) for the common one-binary case; errors with
    /// [`ProjectError::NotSingle`] when the selection matches more than one.
    pub fn load(manifest_path: &Path, overrides: &Overrides) -> Result<Self, ProjectError> {
        let mut projects = Self::discover(manifest_path, overrides)?;
        match projects.len() {
            1 => Ok(projects.pop().unwrap()),
            count => Err(ProjectError::NotSingle { count }),
        }
    }

    /// Build a per-bin project from a workspace member package. The npm name is
    /// the bin name; scope and git URL come from the package's repository.
    pub(crate) fn from_package_bin(
        package: &Package,
        bin: &str,
        config: &Config,
        overrides: &Overrides,
        workspace_root: &Path,
        target_directory: &Path,
    ) -> Result<Self, ProjectError> {
        let repository = package
            .repository
            .clone()
            .ok_or(ProjectError::MissingRepository)?;
        let base = Identity::from_repository(&repository, config.scope.as_deref())?;
        let identity = Identity {
            name: bin.to_owned(),
            ..base
        };
        let version = overrides
            .version
            .clone()
            .unwrap_or_else(|| package.version.to_string());
        let license = config
            .license
            .clone()
            .or_else(|| package.license.clone())
            .unwrap_or_default();
        Ok(Self {
            identity,
            version,
            description: package.description.clone().unwrap_or_default(),
            author: Author::parse(&package.authors.first().cloned().unwrap_or_default()),
            license,
            repository,
            bin: bin.to_owned(),
            package: Some(package.name.as_str().to_owned()),
            config: config.clone(),
            workspace_root: workspace_root.to_path_buf(),
            target_directory: target_directory.to_path_buf(),
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
}

/// A nocmd-shaped [`Project`] for tests in this crate (no filesystem or cargo
/// metadata needed). Override individual fields per test.
#[cfg(test)]
pub(crate) fn sample_project() -> Project {
    Project {
        identity: Identity {
            scope: "@gglinnk".to_owned(),
            name: "nocmd".to_owned(),
            git_url: "git+https://github.com/gglinnk/nocmd.git".to_owned(),
        },
        version: "0.1.1".to_owned(),
        description: "a hook".to_owned(),
        author: Author::parse("Gabriel GRONDIN <gglinnk@protonmail.com>"),
        license: "MIT".to_owned(),
        repository: "https://github.com/gglinnk/nocmd".to_owned(),
        bin: "nocmd".to_owned(),
        package: Some("nocmd".to_owned()),
        config: Config::default(),
        workspace_root: PathBuf::from("."),
        target_directory: PathBuf::from("target"),
    }
}
