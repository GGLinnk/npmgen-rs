use serde::Deserialize;

/// A user-declared target. `key` is the npm platform key (`<os>-<cpu>`); the
/// Rust `triple` and the npm `os`/`cpu` filters are derived from `key` when
/// omitted, and overridable per entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    pub key: String,
    #[serde(default)]
    pub triple: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub cpu: Option<String>,
}
