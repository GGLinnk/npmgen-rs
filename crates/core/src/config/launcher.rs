use serde::Deserialize;

/// File name npmgen writes a generated launcher to.
pub(crate) const GENERATED_LAUNCHER: &str = "launch.mjs";

/// The launcher bundled into the meta package: either **copied** from a file the
/// project provides, or **generated** by npmgen. Naming a `file` copies it;
/// omitting `file` generates the standard shim. `fail_open` only applies to a
/// generated launcher.
///
/// In config: `launcher = "launch.mjs"` / `{ file = "...", bin = "..." }` copies;
/// `{ bin = "..." }` / `{ fail_open = true }` generates. Naming both `file` and
/// `fail_open` is rejected rather than silently dropping the flag.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "LauncherConfig")]
pub struct Launcher {
    file: Option<String>,
    bin: Option<String>,
    fail_open: bool,
}

impl Launcher {
    /// Copy a launcher the project provides.
    pub fn copied(file: impl Into<String>, bin: Option<String>) -> Self {
        Self {
            file: Some(file.into()),
            bin,
            fail_open: false,
        }
    }

    /// Generate the standard launcher shim.
    pub fn generated(bin: Option<String>, fail_open: bool) -> Self {
        Self {
            file: None,
            bin,
            fail_open,
        }
    }

    /// File name of the launcher inside the meta package: the provided path for a
    /// copied launcher, or the default name for a generated one.
    pub fn output(&self) -> &str {
        self.file.as_deref().unwrap_or(GENERATED_LAUNCHER)
    }

    /// npm `bin` command to wire to the launcher, when requested.
    pub fn bin(&self) -> Option<&str> {
        self.bin.as_deref()
    }

    /// Whether npmgen generates the launcher rather than copying a provided file.
    pub fn is_generated(&self) -> bool {
        self.file.is_none()
    }

    /// Whether a generated launcher exits 0 (rather than failing) when no
    /// platform binary is installed.
    pub fn fail_open(&self) -> bool {
        self.fail_open
    }
}

/// Wire form: a bare path string, or a table with optional file/bin/fail_open.
#[derive(Deserialize)]
#[serde(untagged)]
enum LauncherConfig {
    Path(String),
    Table {
        #[serde(default)]
        file: Option<String>,
        #[serde(default)]
        bin: Option<String>,
        #[serde(default)]
        fail_open: Option<bool>,
    },
}

impl TryFrom<LauncherConfig> for Launcher {
    type Error = String;

    fn try_from(config: LauncherConfig) -> Result<Self, Self::Error> {
        match config {
            LauncherConfig::Path(file) => Ok(Self::copied(file, None)),
            LauncherConfig::Table {
                file,
                bin,
                fail_open,
            } => {
                if file.is_some() && fail_open == Some(true) {
                    return Err(
                        "`fail_open` only applies to a generated launcher; omit `file` to generate one"
                            .to_owned(),
                    );
                }
                Ok(Self {
                    file,
                    bin,
                    fail_open: fail_open.unwrap_or(false),
                })
            }
        }
    }
}
