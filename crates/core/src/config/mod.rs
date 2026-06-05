//! The `[package.metadata.npmgen]` (or `[workspace.metadata.npmgen]`) schema.
//!
//! Three payload classes flow from here: npmgen-owned manifests built in code
//! (`package.json`), foreign manifests rendered by identity substitution
//! ([`ManifestSpec`]), and verbatim copies ([`Config::include`]).

mod launcher;
mod manifest_spec;
mod target_spec;

pub use launcher::Launcher;
pub use manifest_spec::ManifestSpec;
pub use target_spec::TargetSpec;

use serde::Deserialize;

/// Deserialized `npmgen` metadata table. Every field is optional so a project
/// publishing a plain cross-platform binary needs no configuration at all.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// npm scope (`@owner`); defaults to the repository owner.
    pub scope: Option<String>,
    /// SPDX license override; defaults to the crate's `license`.
    pub license: Option<String>,
    /// Launcher file bundled into the meta package, optionally wired as `bin`.
    pub launcher: Option<Launcher>,
    /// Non-manifest files/dirs copied verbatim into the meta package.
    pub include: Vec<String>,
    /// Extra fields merged into the meta `package.json`.
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// Foreign manifests rendered by `${var}` identity substitution.
    pub manifests: Vec<ManifestSpec>,
    /// Highest-precedence platform target list; empty means inherit from cargo or default.
    pub targets: Vec<TargetSpec>,
}

/// Failure deserializing the `npmgen` metadata table.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("[package.metadata.npmgen] is not valid")]
    Deserialize {
        #[source]
        source: serde_json::Error,
    },
}

impl Config {
    /// Build from the `npmgen` sub-value of a `metadata` table. A `Null` value
    /// (table absent) yields the all-defaults config.
    pub fn from_metadata(value: &serde_json::Value) -> Result<Self, ConfigError> {
        if value.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(value.clone()).map_err(|source| ConfigError::Deserialize { source })
    }

    /// Resolve this package-level config against the workspace-level `base`,
    /// mirroring cargo's `[workspace.package]` inheritance: a field set here
    /// wins, an unset one is inherited. Scalars inherit when `None`; lists
    /// inherit when empty; `extra` maps merge key by key with this package's
    /// entries taking precedence.
    pub fn inherit(mut self, base: &Config) -> Config {
        self.scope = self.scope.or_else(|| base.scope.clone());
        self.license = self.license.or_else(|| base.license.clone());
        self.launcher = self.launcher.or_else(|| base.launcher.clone());
        if self.include.is_empty() {
            self.include = base.include.clone();
        }
        if self.manifests.is_empty() {
            self.manifests = base.manifests.clone();
        }
        if self.targets.is_empty() {
            self.targets = base.targets.clone();
        }
        let mut extra = base.extra.clone();
        extra.extend(std::mem::take(&mut self.extra));
        self.extra = extra;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_table_yields_defaults() {
        let config = Config::from_metadata(&serde_json::Value::Null).unwrap();
        assert!(config.scope.is_none());
        assert!(config.targets.is_empty());
        assert!(config.manifests.is_empty());
    }

    #[test]
    fn deserializes_full_table() {
        let value = json!({
            "scope": "@gglinnk",
            "launcher": { "file": "launch.mjs", "bin": "nocmd" },
            "include": ["hooks"],
            "manifests": [".claude-plugin/plugin.json"],
            "extra": { "keywords": ["hook"] },
            "targets": [{ "key": "win32-x64", "triple": "x86_64-pc-windows-msvc" }],
        });
        let config = Config::from_metadata(&value).unwrap();
        assert_eq!(config.scope.as_deref(), Some("@gglinnk"));
        assert_eq!(config.launcher.as_ref().unwrap().bin(), Some("nocmd"));
        assert_eq!(config.manifests[0].dest(), ".claude-plugin/plugin.json");
        assert_eq!(config.targets[0].key, "win32-x64");
        assert!(config.extra.contains_key("keywords"));
    }

    #[test]
    fn inherit_takes_workspace_defaults_then_package_overrides() {
        let base = Config::from_metadata(&json!({
            "scope": "@acme",
            "targets": [{ "key": "linux-x64", "triple": "x86_64-unknown-linux-gnu" }],
            "extra": { "keywords": ["shared"], "private": false },
        }))
        .unwrap();
        let package = Config::from_metadata(&json!({
            "manifests": [".claude-plugin/plugin.json"],
            "extra": { "keywords": ["own"] },
        }))
        .unwrap();

        let merged = package.inherit(&base);
        // Unset fields inherit from the workspace.
        assert_eq!(merged.scope.as_deref(), Some("@acme"));
        assert_eq!(merged.targets[0].key, "linux-x64");
        // Set fields win.
        assert_eq!(merged.manifests[0].dest(), ".claude-plugin/plugin.json");
        // Maps merge, package entries overriding.
        assert_eq!(merged.extra["keywords"], json!(["own"]));
        assert_eq!(merged.extra["private"], json!(false));
    }

    #[test]
    fn accepts_launcher_and_manifest_shorthands() {
        let value = json!({
            "launcher": "launch.mjs",
            "manifests": [{ "src": "tmpl/plugin.json", "dest": ".claude-plugin/plugin.json" }],
        });
        let config = Config::from_metadata(&value).unwrap();
        let launcher = config.launcher.unwrap();
        assert_eq!(launcher.output(), "launch.mjs");
        assert_eq!(launcher.bin(), None);
        assert!(!launcher.is_generated());
        assert_eq!(config.manifests[0].src(), "tmpl/plugin.json");
        assert_eq!(config.manifests[0].dest(), ".claude-plugin/plugin.json");
    }

    #[test]
    fn launcher_without_a_file_is_generated() {
        let value = json!({ "launcher": { "bin": "mytool", "fail_open": true } });
        let launcher = Config::from_metadata(&value).unwrap().launcher.unwrap();
        assert!(launcher.is_generated());
        assert_eq!(launcher.bin(), Some("mytool"));
        assert!(launcher.fail_open());
        assert_eq!(launcher.output(), "launch.mjs");
    }

    #[test]
    fn rejects_fail_open_on_a_copied_launcher() {
        let value = json!({ "launcher": { "file": "launch.mjs", "fail_open": true } });
        assert!(Config::from_metadata(&value).is_err());
    }

    #[test]
    fn rejects_an_unknown_launcher_key() {
        // A typo on `file` must error, not silently ship a generated launcher.
        let value = json!({ "launcher": { "fial": "launch.mjs" } });
        assert!(Config::from_metadata(&value).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        let value = json!({ "nonsense": true });
        assert!(Config::from_metadata(&value).is_err());
    }
}
