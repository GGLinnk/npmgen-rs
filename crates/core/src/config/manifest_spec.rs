use serde::Deserialize;

/// A foreign manifest to render by identity substitution. Accepts a bare path
/// (source and destination are the same, relative to the project root) or a
/// table with distinct `src`/`dest` when the template lives apart from its
/// published location.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestSpec {
    Path(String),
    Pair { src: String, dest: String },
}

impl ManifestSpec {
    /// Template path read from the project root.
    pub fn src(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Pair { src, .. } => src,
        }
    }

    /// Output path written under the meta package.
    pub fn dest(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Pair { dest, .. } => dest,
        }
    }
}
