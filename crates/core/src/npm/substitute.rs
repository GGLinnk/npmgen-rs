//! Foreign-manifest rendering by structured data substitution.
//!
//! The source is parsed by extension into a value tree (JSON or TOML), every
//! `${var}` occurrence inside a string node is expanded against the project
//! variables, and the tree is handed back for serialization. Substitution at
//! the data layer (not text) guarantees the output stays valid and correctly
//! escaped no matter what a variable's value contains.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::NpmError;

/// Largest foreign manifest npmgen reads, so a crafted file cannot exhaust
/// memory when slurped whole.
const MAX_MANIFEST_BYTES: u64 = 5 * 1024 * 1024;

/// Largest `${...}` placeholder npmgen scans for, bounding the search when a
/// closing brace is absent.
const MAX_PLACEHOLDER_LEN: usize = 256;

/// A rendered manifest, ready to be written in its native format.
#[derive(Debug)]
pub enum RenderedManifest {
    Json(serde_json::Value),
    Toml(String),
}

/// Expands `${var}` placeholders in foreign manifests from a fixed variable set.
pub struct ManifestRenderer<'a> {
    variables: &'a BTreeMap<String, String>,
}

impl<'a> ManifestRenderer<'a> {
    pub fn new(variables: &'a BTreeMap<String, String>) -> Self {
        Self { variables }
    }

    /// Parse `src` by extension, substitute identity variables, and return the
    /// rendered tree.
    pub fn render(&self, src: &Path) -> Result<RenderedManifest, NpmError> {
        let metadata = fs::metadata(src).map_err(|source| NpmError::Read {
            path: src.to_path_buf(),
            source,
        })?;
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(NpmError::ManifestTooLarge {
                path: src.to_path_buf(),
                size: metadata.len(),
                max: MAX_MANIFEST_BYTES,
            });
        }
        let text = fs::read_to_string(src).map_err(|source| NpmError::Read {
            path: src.to_path_buf(),
            source,
        })?;

        match src.extension().and_then(|extension| extension.to_str()) {
            Some("json") => {
                let mut value: serde_json::Value =
                    serde_json::from_str(&text).map_err(|source| NpmError::ParseJson {
                        path: src.to_path_buf(),
                        source,
                    })?;
                self.substitute_json(&mut value, src)?;
                Ok(RenderedManifest::Json(value))
            }
            Some("toml") => {
                let mut value: toml::Value =
                    toml::from_str(&text).map_err(|source| NpmError::ParseToml {
                        path: src.to_path_buf(),
                        source,
                    })?;
                self.substitute_toml(&mut value, src)?;
                let mut rendered =
                    toml::to_string_pretty(&value).map_err(|source| NpmError::SerializeToml {
                        path: src.to_path_buf(),
                        source,
                    })?;
                if !rendered.ends_with('\n') {
                    rendered.push('\n');
                }
                Ok(RenderedManifest::Toml(rendered))
            }
            _ => Err(NpmError::UnsupportedManifestFormat {
                path: src.to_path_buf(),
            }),
        }
    }

    fn substitute_json(&self, value: &mut serde_json::Value, src: &Path) -> Result<(), NpmError> {
        match value {
            serde_json::Value::String(text) => *text = self.expand(text, src)?,
            serde_json::Value::Array(items) => {
                for item in items {
                    self.substitute_json(item, src)?;
                }
            }
            serde_json::Value::Object(map) => {
                for item in map.values_mut() {
                    self.substitute_json(item, src)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn substitute_toml(&self, value: &mut toml::Value, src: &Path) -> Result<(), NpmError> {
        match value {
            toml::Value::String(text) => *text = self.expand(text, src)?,
            toml::Value::Array(items) => {
                for item in items {
                    self.substitute_toml(item, src)?;
                }
            }
            toml::Value::Table(map) => {
                for (_, item) in map.iter_mut() {
                    self.substitute_toml(item, src)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Expand every `${name}` in `input`. An unknown name or an unterminated
    /// placeholder is an error rather than a silent pass-through.
    fn expand(&self, input: &str, src: &Path) -> Result<String, NpmError> {
        const OPEN: &str = "${";
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find(OPEN) {
            out.push_str(&rest[..start]);
            let after = &rest[start + OPEN.len()..];
            let limit = after.len().min(MAX_PLACEHOLDER_LEN);
            let end = after.as_bytes()[..limit]
                .iter()
                .position(|&byte| byte == b'}')
                .ok_or_else(|| NpmError::UnterminatedPlaceholder {
                    path: src.to_path_buf(),
                })?;
            let name = &after[..end];
            let value = self
                .variables
                .get(name)
                .ok_or_else(|| NpmError::UnknownVariable {
                    name: name.to_owned(),
                    path: src.to_path_buf(),
                })?;
            out.push_str(value);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("name".to_owned(), "nocmd".to_owned()),
            ("version".to_owned(), "0.1.1".to_owned()),
            // Deliberately hostile value: a quote, a backslash and a newline.
            (
                "description".to_owned(),
                "say \"hi\"\\done\nnext".to_owned(),
            ),
        ])
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("npmgen-subst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn substitution_at_data_layer_stays_valid_json() {
        let path = scratch("plugin.json");
        std::fs::write(
            &path,
            r#"{ "name": "${name}", "version": "${version}", "blurb": "${description}" }"#,
        )
        .unwrap();

        let variables = variables();
        let rendered = ManifestRenderer::new(&variables).render(&path).unwrap();
        let RenderedManifest::Json(value) = rendered else {
            panic!("expected json");
        };

        // The hostile value round-trips intact, and re-serializing yields valid JSON.
        assert_eq!(value["blurb"], serde_json::json!(variables["description"]));
        let serialized = serde_json::to_string(&value).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            reparsed["blurb"],
            serde_json::json!(variables["description"])
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nested_placeholders_are_expanded() {
        let path = scratch("nested.json");
        std::fs::write(
            &path,
            r#"{ "a": ["${name}"], "b": { "c": "v${version}" } }"#,
        )
        .unwrap();

        let variables = variables();
        let RenderedManifest::Json(value) =
            ManifestRenderer::new(&variables).render(&path).unwrap()
        else {
            panic!("expected json");
        };
        assert_eq!(value["a"][0], serde_json::json!("nocmd"));
        assert_eq!(value["b"]["c"], serde_json::json!("v0.1.1"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_variable_is_an_error() {
        let path = scratch("bad.json");
        std::fs::write(&path, r#"{ "x": "${nope}" }"#).unwrap();
        let variables = variables();
        let error = ManifestRenderer::new(&variables).render(&path).unwrap_err();
        assert!(matches!(error, NpmError::UnknownVariable { .. }));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn toml_substitution_round_trips_and_ends_with_newline() {
        let path = scratch("manifest.toml");
        std::fs::write(
            &path,
            "name = \"${name}\"\nblurb = \"${description}\"\n\n[nested]\ninner = \"${name}\"\nlist = [\"${version}\", \"plain\"]\n",
        )
        .unwrap();

        let variables = variables();
        let RenderedManifest::Toml(rendered) =
            ManifestRenderer::new(&variables).render(&path).unwrap()
        else {
            panic!("expected toml");
        };

        assert!(rendered.ends_with('\n'));
        let reparsed: toml::Value = toml::from_str(&rendered).unwrap();
        assert_eq!(
            reparsed["blurb"].as_str(),
            Some(variables["description"].as_str())
        );
        assert_eq!(reparsed["nested"]["inner"].as_str(), Some("nocmd"));
        assert_eq!(reparsed["nested"]["list"][0].as_str(), Some("0.1.1"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn oversized_manifest_is_rejected() {
        let path = scratch("huge.json");
        let big = "a".repeat(MAX_MANIFEST_BYTES as usize + 1);
        std::fs::write(&path, format!("{{ \"x\": \"{big}\" }}")).unwrap();
        let variables = variables();
        let error = ManifestRenderer::new(&variables).render(&path).unwrap_err();
        assert!(matches!(error, NpmError::ManifestTooLarge { .. }));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_overlong_placeholder_is_unterminated() {
        let path = scratch("overlong.json");
        let name = "a".repeat(MAX_PLACEHOLDER_LEN + 8);
        let placeholder = format!("${{{name}}}");
        std::fs::write(&path, format!("{{ \"x\": \"{placeholder}\" }}")).unwrap();
        let variables = variables();
        let error = ManifestRenderer::new(&variables).render(&path).unwrap_err();
        assert!(matches!(error, NpmError::UnterminatedPlaceholder { .. }));
        let _ = std::fs::remove_file(&path);
    }
}
