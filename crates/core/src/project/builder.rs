use std::path::PathBuf;

use super::{Author, Identity, Project};
use crate::config::Config;

/// Programmatic, dependency-free construction of a [`Project`].
///
/// Unlike [`Project::load`](super::Project::load), this needs no `Cargo.toml`,
/// no `cargo metadata`, and no TOML parsing: a caller that already holds the
/// package facts supplies them directly. The scope, name and version are
/// required; every other field defaults.
#[derive(Debug, Clone)]
pub struct ProjectBuilder {
    scope: String,
    name: String,
    version: String,
    git_url: String,
    description: String,
    author: String,
    license: String,
    repository: String,
    bin: Option<String>,
    package: Option<String>,
    config: Config,
    workspace_root: PathBuf,
    target_directory: PathBuf,
}

impl ProjectBuilder {
    pub(crate) fn new(
        scope: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            scope: scope.into(),
            name: name.into(),
            version: version.into(),
            git_url: String::new(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            repository: String::new(),
            bin: None,
            package: None,
            config: Config::default(),
            workspace_root: PathBuf::from("."),
            target_directory: PathBuf::from("target"),
        }
    }

    /// npm git URL recorded in the meta `package.json` repository field.
    pub fn git_url(mut self, git_url: impl Into<String>) -> Self {
        self.git_url = git_url.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Author entry in `Name <email>` form.
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn license(mut self, license: impl Into<String>) -> Self {
        self.license = license.into();
        self
    }

    /// Raw repository URL exposed to manifest substitution.
    pub fn repository(mut self, repository: impl Into<String>) -> Self {
        self.repository = repository.into();
        self
    }

    /// Cargo bin name to build and ship; defaults to the package name.
    pub fn bin(mut self, bin: impl Into<String>) -> Self {
        self.bin = Some(bin.into());
        self
    }

    /// Cargo package passed as `--package` to the build.
    pub fn package(mut self, package: impl Into<String>) -> Self {
        self.package = Some(package.into());
        self
    }

    /// Targets, payload and manifests to generate.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Root the payload and manifest sources are read from, and where the build runs.
    pub fn workspace_root(mut self, workspace_root: impl Into<PathBuf>) -> Self {
        self.workspace_root = workspace_root.into();
        self
    }

    /// Cargo target directory the compiled binaries are copied from.
    pub fn target_directory(mut self, target_directory: impl Into<PathBuf>) -> Self {
        self.target_directory = target_directory.into();
        self
    }

    pub fn build(self) -> Project {
        let bin = self
            .bin
            .or_else(|| self.config.bin.clone())
            .unwrap_or_else(|| self.name.clone());
        Project {
            identity: Identity {
                scope: self.scope,
                name: self.name,
                git_url: self.git_url,
            },
            version: self.version,
            description: self.description,
            author: Author::parse(&self.author),
            license: self.license,
            repository: self.repository,
            bin,
            package: self.package,
            config: self.config,
            workspace_root: self.workspace_root,
            target_directory: self.target_directory,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectBuilder;

    #[test]
    fn builds_identity_and_defaults_bin_to_name() {
        let project = ProjectBuilder::new("@me", "tool", "1.2.3")
            .git_url("git+https://example.test/me/tool.git")
            .build();
        assert_eq!(project.package_name(), "@me/tool");
        assert_eq!(project.version, "1.2.3");
        assert_eq!(project.bin, "tool");
        assert_eq!(
            project.identity.git_url,
            "git+https://example.test/me/tool.git"
        );
    }

    #[test]
    fn explicit_bin_overrides_the_name_default() {
        let project = ProjectBuilder::new("@me", "tool", "1.2.3")
            .bin("other")
            .build();
        assert_eq!(project.bin, "other");
    }
}
