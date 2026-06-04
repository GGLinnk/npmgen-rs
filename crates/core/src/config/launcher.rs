use serde::Deserialize;

/// A launcher script bundled into the meta package. Accepts a bare path
/// (`launcher = "launch.mjs"`) or a table that also wires the npm `bin`
/// (`launcher = { file = "launch.mjs", bin = "mytool" }`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Launcher {
    File(String),
    Detailed {
        file: String,
        #[serde(default)]
        bin: Option<String>,
    },
}

impl Launcher {
    /// Path of the launcher file, relative to the project root.
    pub fn file(&self) -> &str {
        match self {
            Self::File(file) => file,
            Self::Detailed { file, .. } => file,
        }
    }

    /// npm `bin` name to wire to the launcher, when requested.
    pub fn bin(&self) -> Option<&str> {
        match self {
            Self::File(_) => None,
            Self::Detailed { bin, .. } => bin.as_deref(),
        }
    }
}
