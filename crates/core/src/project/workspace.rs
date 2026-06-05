use std::collections::BTreeMap;
use std::path::Path;

use cargo_metadata::{Metadata, MetadataCommand, Package};

use super::{METADATA_KEY, Overrides, Project, ProjectError};
use crate::config::Config;

/// The target cargo workspace, resolved once via `cargo metadata`, and the
/// source for per-binary discovery.
pub struct Workspace {
    metadata: Metadata,
}

impl Workspace {
    /// Run `cargo metadata` for the workspace containing `manifest_path`.
    pub fn load(manifest_path: &Path) -> Result<Self, ProjectError> {
        let metadata = MetadataCommand::new()
            .manifest_path(manifest_path)
            .exec()
            .map_err(|source| ProjectError::Metadata {
                source: Box::new(source),
            })?;
        Ok(Self { metadata })
    }

    /// One project per published binary, named after the binary, selected like
    /// `cargo build`.
    ///
    /// The package set is the default-members (or all members), or the explicit
    /// `--package`/`--workspace` set, minus `--exclude` and minus
    /// `publish = false` crates (kept only when named explicitly). Each selected
    /// package contributes all its binaries, or just those named by `--bin`. A
    /// member's `[package.metadata.npmgen]` inherits `[workspace.metadata.npmgen]`
    /// and is parsed only when the member actually ships a binary, so an
    /// unrelated member's malformed config never aborts the run.
    pub fn projects(&self, overrides: &Overrides) -> Result<Vec<Project>, ProjectError> {
        let workspace_root = self.metadata.workspace_root.as_std_path();
        let target_directory = self.metadata.target_directory.as_std_path();
        let workspace_config = self.workspace_config()?;

        let mut projects = Vec::new();
        for package in self.selected_packages(overrides)? {
            let bins = Self::selected_bins(package, overrides);
            if bins.is_empty() {
                continue;
            }
            let config = Self::package_config(package)?.inherit(&workspace_config);
            for bin in bins {
                projects.push(Project::from_package_bin(
                    package,
                    bin,
                    &config,
                    overrides,
                    workspace_root,
                    target_directory,
                )?);
            }
        }

        self.reject_unmatched_bins(overrides, &projects)?;
        Self::reject_duplicate_names(&projects)?;
        Ok(projects)
    }

    /// The packages in scope, following cargo's selection precedence.
    fn selected_packages(&self, overrides: &Overrides) -> Result<Vec<&Package>, ProjectError> {
        let mut packages = if !overrides.packages.is_empty() {
            // Explicit `--package`: resolve each by name. Honor it even for a
            // `publish = false` crate, since the user asked for it by name.
            let mut picked = Vec::new();
            for name in &overrides.packages {
                picked.push(self.package_named(name)?);
            }
            picked
        } else {
            let base = if overrides.workspace {
                self.metadata.workspace_packages()
            } else {
                self.default_packages()
            };
            base.into_iter().filter(|p| Self::publishable(p)).collect()
        };

        if !overrides.exclude.is_empty() {
            packages.retain(|package| {
                !overrides
                    .exclude
                    .iter()
                    .any(|name| name == package.name.as_str())
            });
        }
        Ok(packages)
    }

    /// The default-members of the workspace (or all members when none are
    /// configured, or the running cargo is too old to report them).
    fn default_packages(&self) -> Vec<&Package> {
        if self.metadata.workspace_default_members.is_available() {
            self.metadata.workspace_default_packages()
        } else {
            self.metadata.workspace_packages()
        }
    }

    fn package_named(&self, name: &str) -> Result<&Package, ProjectError> {
        self.metadata
            .workspace_packages()
            .into_iter()
            .find(|package| package.name.as_str() == name)
            .ok_or_else(|| ProjectError::PackageNotFound {
                name: name.to_owned(),
            })
    }

    /// A `publish = false` crate is private and never published (like cargo).
    fn publishable(package: &Package) -> bool {
        !package
            .publish
            .as_ref()
            .is_some_and(|registries| registries.is_empty())
    }

    /// The package's binaries, narrowed to `--bin` when given.
    fn selected_bins<'a>(package: &'a Package, overrides: &Overrides) -> Vec<&'a str> {
        Self::bin_names(package)
            .filter(|name| {
                overrides.bins.is_empty() || overrides.bins.iter().any(|bin| bin == name)
            })
            .collect()
    }

    fn workspace_config(&self) -> Result<Config, ProjectError> {
        match self.metadata.workspace_metadata.get(METADATA_KEY) {
            Some(value) => Ok(Config::from_metadata(value)?),
            None => Ok(Config::default()),
        }
    }

    fn package_config(package: &Package) -> Result<Config, ProjectError> {
        match package.metadata.get(METADATA_KEY) {
            Some(value) => Ok(Config::from_metadata(value)?),
            None => Ok(Config::default()),
        }
    }

    /// Each `--bin` must match at least one shipped binary.
    fn reject_unmatched_bins(
        &self,
        overrides: &Overrides,
        projects: &[Project],
    ) -> Result<(), ProjectError> {
        for wanted in &overrides.bins {
            if !projects.iter().any(|project| &project.bin == wanted) {
                return Err(if overrides.packages.is_empty() {
                    ProjectError::BinNotInWorkspace {
                        bin: wanted.clone(),
                    }
                } else {
                    ProjectError::UnknownBin {
                        package: overrides.packages.join(", "),
                        bin: wanted.clone(),
                    }
                });
            }
        }
        Ok(())
    }

    /// Two members can legally declare a bin with the same name; published under
    /// that name they would collide. Reject it so the user disambiguates (e.g.
    /// with `--package`) instead of one silently winning.
    fn reject_duplicate_names(projects: &[Project]) -> Result<(), ProjectError> {
        let mut by_name: BTreeMap<&str, &str> = BTreeMap::new();
        for project in projects {
            let package = project.package.as_deref().unwrap_or_default();
            if let Some(other) = by_name.insert(&project.identity.name, package) {
                return Err(ProjectError::AmbiguousBin {
                    bin: project.identity.name.clone(),
                    packages: vec![other.to_owned(), package.to_owned()],
                });
            }
        }
        Ok(())
    }

    fn bin_names(package: &Package) -> impl Iterator<Item = &str> {
        package
            .targets
            .iter()
            .filter(|target| target.is_bin())
            .map(|target| target.name.as_str())
    }
}
