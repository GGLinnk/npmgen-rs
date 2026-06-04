use std::fs;
use std::path::Path;

use super::{Target, TargetError};
use crate::config::Config;

/// Cargo configuration directory and the candidate file names within it.
const CARGO_CONFIG_DIR: &str = ".cargo";
const CARGO_CONFIG_FILES: &[&str] = &["config.toml", "config"];
/// `[build] target` table/key cargo declares its default targets under.
const BUILD_TABLE: &str = "build";
const TARGET_KEY: &str = "target";

/// Resolves the build target set for a project by precedence:
///
/// 1. `config.targets` if non-empty (explicit, highest precedence);
/// 2. else cargo's `[build] target` discovered from the workspace root;
/// 3. else the default platform set.
///
/// A `--target` key filter, when present, narrows whichever set wins.
#[derive(Debug)]
pub struct TargetResolver<'a> {
    config: &'a Config,
    workspace_root: &'a Path,
}

impl<'a> TargetResolver<'a> {
    pub fn new(config: &'a Config, workspace_root: &'a Path) -> Self {
        Self {
            config,
            workspace_root,
        }
    }

    /// Resolve the targets, then retain only those whose key is in `filter`
    /// (empty `filter` keeps all). An unmatched filter key is an error.
    pub fn resolve(&self, filter: &[String]) -> Result<Vec<Target>, TargetError> {
        let mut targets = self.base_targets()?;
        if !filter.is_empty() {
            for key in filter {
                if !targets.iter().any(|target| &target.key == key) {
                    return Err(TargetError::UnknownFilterKey { key: key.clone() });
                }
            }
            targets.retain(|target| filter.iter().any(|key| key == &target.key));
        }
        Ok(targets)
    }

    fn base_targets(&self) -> Result<Vec<Target>, TargetError> {
        if !self.config.targets.is_empty() {
            return self.config.targets.iter().map(Target::from_spec).collect();
        }
        if let Some(triples) = self.cargo_targets()? {
            return triples
                .iter()
                .map(|triple| Target::from_triple(triple))
                .collect();
        }
        Ok(Target::defaults())
    }

    /// First `[build] target` walking `.cargo/config.toml` from the workspace
    /// root upward, the way cargo merges configuration. Neither the home-level
    /// (`$CARGO_HOME`) config nor the `CARGO_BUILD_TARGET` env var is consulted:
    /// a global default target is an unusual choice for a publish tool, and
    /// reading either would route a config knob outside the project tree (the
    /// crate reads no env vars outside clap). Pass `--target` to override.
    fn cargo_targets(&self) -> Result<Option<Vec<String>>, TargetError> {
        for dir in self.workspace_root.ancestors() {
            let cargo_dir = dir.join(CARGO_CONFIG_DIR);
            let path = CARGO_CONFIG_FILES
                .iter()
                .map(|name| cargo_dir.join(name))
                .find(|candidate| candidate.is_file());
            let Some(path) = path else { continue };

            let text = fs::read_to_string(&path).map_err(|source| TargetError::CargoConfig {
                path: path.clone(),
                source,
            })?;
            let value: toml::Value =
                toml::from_str(&text).map_err(|source| TargetError::CargoConfigParse {
                    path: path.clone(),
                    source,
                })?;

            if let Some(target) = value
                .get(BUILD_TABLE)
                .and_then(|build| build.get(TARGET_KEY))
            {
                return Ok(Some(Self::triples(target)));
            }
        }
        Ok(None)
    }

    /// `build.target` is a single triple string or an array of triples.
    fn triples(value: &toml::Value) -> Vec<String> {
        match value {
            toml::Value::String(triple) => vec![triple.clone()],
            toml::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TargetSpec;

    fn spec(key: &str) -> TargetSpec {
        TargetSpec {
            key: key.to_owned(),
            triple: None,
            os: None,
            cpu: None,
        }
    }

    fn config_with(targets: Vec<TargetSpec>) -> Config {
        Config {
            targets,
            ..Config::default()
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("npmgen-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn explicit_targets_take_precedence_and_skip_the_filesystem() {
        let config = config_with(vec![spec("linux-x64")]);
        let targets = TargetResolver::new(&config, Path::new("/does/not/exist"))
            .resolve(&[])
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].triple, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn filter_narrows_and_rejects_unknown_keys() {
        let config = config_with(vec![spec("win32-x64"), spec("linux-x64")]);
        let resolver = TargetResolver::new(&config, Path::new("/does/not/exist"));

        let kept = resolver.resolve(&["linux-x64".to_owned()]).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].key, "linux-x64");

        assert!(resolver.resolve(&["bogus".to_owned()]).is_err());
    }

    #[test]
    fn inherits_cargo_configured_target() {
        let root = scratch("cargo-config");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo").join("config.toml"),
            "[build]\ntarget = \"aarch64-apple-darwin\"\n",
        )
        .unwrap();

        let config = Config::default();
        let targets = TargetResolver::new(&config, &root).resolve(&[]).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "darwin-arm64");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn inherits_cargo_target_array() {
        let root = scratch("cargo-array");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo").join("config.toml"),
            "[build]\ntarget = [\"x86_64-pc-windows-msvc\", \"x86_64-apple-darwin\"]\n",
        )
        .unwrap();

        let config = Config::default();
        let targets = TargetResolver::new(&config, &root).resolve(&[]).unwrap();
        let keys: Vec<&str> = targets.iter().map(|target| target.key.as_str()).collect();
        assert_eq!(keys, ["win32-x64", "darwin-x64"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn explicit_config_beats_a_present_cargo_config() {
        let root = scratch("config-beats-cargo");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo").join("config.toml"),
            "[build]\ntarget = \"aarch64-apple-darwin\"\n",
        )
        .unwrap();

        let config = config_with(vec![spec("win32-x64")]);
        let targets = TargetResolver::new(&config, &root).resolve(&[]).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "win32-x64");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nearest_cargo_config_wins_over_an_ancestor() {
        let root = scratch("nested-cargo");
        let inner = root.join("sub");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::create_dir_all(inner.join(".cargo")).unwrap();
        fs::write(
            root.join(".cargo").join("config.toml"),
            "[build]\ntarget = \"x86_64-unknown-linux-gnu\"\n",
        )
        .unwrap();
        fs::write(
            inner.join(".cargo").join("config.toml"),
            "[build]\ntarget = \"aarch64-apple-darwin\"\n",
        )
        .unwrap();

        let config = Config::default();
        let targets = TargetResolver::new(&config, &inner).resolve(&[]).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "darwin-arm64");

        let _ = fs::remove_dir_all(&root);
    }
}
