use serde::Deserialize;

/// File name npmgen writes a generated launcher to.
pub(crate) const GENERATED_LAUNCHER: &str = "launch.mjs";

/// The launcher bundled into the meta package. It is either **copied** from a
/// file the project provides, or **generated** by npmgen. The form is chosen by
/// whether a source `file` is named:
///
/// - `launcher = "launch.mjs"` or `{ file = "...", bin = "..." }` -> copy it.
/// - `launcher = { bin = "..." }` / `{ fail_open = true }` (no `file`) -> generate.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Launcher {
    File(String),
    Detailed {
        file: String,
        #[serde(default)]
        bin: Option<String>,
    },
    Generated {
        #[serde(default)]
        bin: Option<String>,
        #[serde(default)]
        fail_open: bool,
    },
}

impl Launcher {
    /// File name of the launcher inside the meta package. For a copied launcher
    /// this is the provided path; for a generated one it is the default name.
    pub fn output(&self) -> &str {
        match self {
            Self::File(file) => file,
            Self::Detailed { file, .. } => file,
            Self::Generated { .. } => GENERATED_LAUNCHER,
        }
    }

    /// npm `bin` command to wire to the launcher, when requested.
    pub fn bin(&self) -> Option<&str> {
        match self {
            Self::File(_) => None,
            Self::Detailed { bin, .. } | Self::Generated { bin, .. } => bin.as_deref(),
        }
    }

    /// Whether npmgen generates the launcher rather than copying a provided file.
    pub fn is_generated(&self) -> bool {
        matches!(self, Self::Generated { .. })
    }

    /// Whether a generated launcher exits 0 (rather than failing) when no
    /// platform binary is installed. Only meaningful for the generated form.
    pub fn fail_open(&self) -> bool {
        matches!(
            self,
            Self::Generated {
                fail_open: true,
                ..
            }
        )
    }
}
