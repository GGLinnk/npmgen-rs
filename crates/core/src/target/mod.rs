//! Build targets: the resolved [`Target`] domain type and the [`TargetResolver`]
//! that derives the target set by precedence.

mod defaults;
mod resolver;

pub use resolver::TargetResolver;

use std::path::{Path, PathBuf};

use crate::config::TargetSpec;

/// Cargo profile directory the release build lands in; must agree with the
/// `--release` flag the build driver passes.
pub(crate) const RELEASE_PROFILE_DIR: &str = "release";

/// A resolved platform: the npm key, its npm `os`/`cpu` install filters, the
/// Rust target triple to build, and whether the binary carries a `.exe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub key: String,
    pub os: String,
    pub cpu: String,
    pub triple: String,
    pub windows: bool,
}

impl Target {
    /// Build a target by decomposing a Rust target triple into npm os/cpu/key.
    pub fn from_triple(triple: &str) -> Result<Self, TargetError> {
        let segments: Vec<&str> = triple.split('-').collect();
        let arch = segments.first().copied().unwrap_or_default();
        let cpu = Self::cpu_for_arch(arch).ok_or_else(|| TargetError::UnknownTriple {
            triple: triple.to_owned(),
        })?;
        // Match the right-most system token: Rust triples place the OS after the
        // kernel, so `aarch64-linux-android` is android, not linux.
        let (system, os) = segments
            .iter()
            .rev()
            .find_map(|segment| Self::os_for_system(segment).map(|os| (*segment, os)))
            .ok_or_else(|| TargetError::UnknownTriple {
                triple: triple.to_owned(),
            })?;

        Ok(Self {
            key: format!("{os}-{cpu}"),
            os: os.to_owned(),
            cpu: cpu.to_owned(),
            triple: triple.to_owned(),
            windows: Self::is_windows_system(system),
        })
    }

    /// Build a target from a user spec: the triple defaults from the key, and the
    /// os/cpu default from the key's two segments.
    pub fn from_spec(spec: &TargetSpec) -> Result<Self, TargetError> {
        let triple = spec
            .triple
            .clone()
            .or_else(|| Self::default_triple(&spec.key).map(str::to_owned))
            .ok_or_else(|| TargetError::UnknownTargetKey {
                key: spec.key.clone(),
            })?;

        let (key_os, key_cpu) =
            spec.key
                .split_once('-')
                .ok_or_else(|| TargetError::InvalidKey {
                    key: spec.key.clone(),
                })?;
        let os = spec.os.clone().unwrap_or_else(|| key_os.to_owned());
        let cpu = spec.cpu.clone().unwrap_or_else(|| key_cpu.to_owned());

        Ok(Self {
            windows: Self::is_windows_os(&os),
            key: spec.key.clone(),
            os,
            cpu,
            triple,
        })
    }

    /// The default platform set: every entry of the key/triple table.
    pub(super) fn defaults() -> Vec<Self> {
        defaults::KEY_TRIPLES
            .iter()
            .map(|(_, triple)| Self::from_triple(triple).expect("default triples decode"))
            .collect()
    }

    /// File name of `stem` for this platform (appends `.exe` on Windows).
    pub fn binary_filename(&self, stem: &str) -> String {
        if self.windows {
            format!("{stem}.exe")
        } else {
            stem.to_owned()
        }
    }

    /// Path of the compiled `bin` under cargo's target directory:
    /// `<target_dir>/<triple>/release/<bin>[.exe]`.
    pub fn binary_path(&self, target_directory: &Path, bin: &str) -> PathBuf {
        target_directory
            .join(&self.triple)
            .join(RELEASE_PROFILE_DIR)
            .join(self.binary_filename(bin))
    }

    fn default_triple(key: &str) -> Option<&'static str> {
        Self::lookup(defaults::KEY_TRIPLES, key)
    }

    fn cpu_for_arch(arch: &str) -> Option<&'static str> {
        Self::lookup(defaults::ARCH_CPU, arch)
    }

    fn os_for_system(system: &str) -> Option<&'static str> {
        Self::lookup(defaults::SYSTEM_OS, system)
    }

    fn is_windows_system(system: &str) -> bool {
        system == defaults::WINDOWS_SYSTEM
    }

    fn is_windows_os(os: &str) -> bool {
        os == defaults::WINDOWS_OS
    }

    fn lookup(table: &[(&str, &'static str)], needle: &str) -> Option<&'static str> {
        table
            .iter()
            .find(|(key, _)| *key == needle)
            .map(|(_, value)| *value)
    }
}

/// Failures resolving the target set.
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    #[error("target key {key:?} is not of the form <os>-<cpu>")]
    InvalidKey { key: String },

    #[error("no default triple for target key {key:?}; declare `triple` explicitly")]
    UnknownTargetKey { key: String },

    #[error("cannot derive os/cpu from target triple {triple:?}")]
    UnknownTriple { triple: String },

    #[error("--target {key:?} matches none of the resolved targets")]
    UnknownFilterKey { key: String },

    #[error("reading cargo config {}", path.display())]
    CargoConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing cargo config {}", path.display())]
    CargoConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn decomposes_known_triples() {
        let windows = Target::from_triple("x86_64-pc-windows-msvc").unwrap();
        assert_eq!(windows.key, "win32-x64");
        assert_eq!(windows.os, "win32");
        assert_eq!(windows.cpu, "x64");
        assert!(windows.windows);

        let mac = Target::from_triple("aarch64-apple-darwin").unwrap();
        assert_eq!(mac.key, "darwin-arm64");
        assert!(!mac.windows);

        let linux = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(linux.key, "linux-x64");

        // Two system tokens: the rightmost (android) wins over linux.
        let android = Target::from_triple("aarch64-linux-android").unwrap();
        assert_eq!(android.key, "android-arm64");
        assert_eq!(android.os, "android");
        assert!(!android.windows);
    }

    #[test]
    fn rejects_undecodable_triple() {
        assert!(Target::from_triple("sparc-unknown-haiku").is_err());
    }

    #[test]
    fn spec_defaults_triple_from_key() {
        let spec = TargetSpec {
            key: "win32-arm64".to_owned(),
            triple: None,
            os: None,
            cpu: None,
        };
        let target = Target::from_spec(&spec).unwrap();
        assert_eq!(target.triple, "aarch64-pc-windows-msvc");
        assert_eq!(target.os, "win32");
        assert_eq!(target.cpu, "arm64");
        assert!(target.windows);
    }

    #[test]
    fn spec_without_default_triple_needs_explicit_one() {
        let spec = TargetSpec {
            key: "haiku-x64".to_owned(),
            triple: None,
            os: None,
            cpu: None,
        };
        assert!(Target::from_spec(&spec).is_err());
    }

    #[test]
    fn binary_filename_appends_exe_on_windows_only() {
        let windows = Target::from_triple("x86_64-pc-windows-msvc").unwrap();
        let linux = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(windows.binary_filename("tool"), "tool.exe");
        assert_eq!(linux.binary_filename("tool"), "tool");
        assert_eq!(
            linux.binary_path(Path::new("/t"), "tool"),
            Path::new("/t/x86_64-unknown-linux-gnu/release/tool")
        );
    }
}
