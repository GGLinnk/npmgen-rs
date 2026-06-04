//! Writes into one package directory of the publish tree, rooted at a fixed
//! base so callers pass paths relative to that package.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::debug;

use super::NpmError;

/// A package directory under the output tree. All paths are relative to `root`.
pub struct TreeWriter {
    root: PathBuf,
}

impl TreeWriter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create the package directory.
    pub fn ensure(&self) -> Result<(), NpmError> {
        Self::create_dir(&self.root)
    }

    /// Write `value` as pretty JSON with a trailing newline.
    pub fn write_json(&self, relative: &str, value: &Value) -> Result<(), NpmError> {
        let path = self.root.join(relative);
        let mut text =
            serde_json::to_string_pretty(value).map_err(|source| NpmError::Serialize {
                path: path.clone(),
                source,
            })?;
        text.push('\n');
        self.write_string(relative, &text)
    }

    /// Write `text` verbatim, creating parent directories.
    pub fn write_string(&self, relative: &str, text: &str) -> Result<(), NpmError> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            Self::create_dir(parent)?;
        }
        fs::write(&path, text).map_err(|source| NpmError::Write {
            path: path.clone(),
            source,
        })?;
        debug!(path = %path.display(), "wrote file");
        Ok(())
    }

    /// Copy a single file to `relative`, creating parent directories.
    pub fn copy_file(&self, from: &Path, relative: &str) -> Result<(), NpmError> {
        let to = self.root.join(relative);
        if let Some(parent) = to.parent() {
            Self::create_dir(parent)?;
        }
        Self::copy_one(from, &to)
    }

    /// Copy a file or directory tree to `relative`. Returns `false` when the
    /// source is absent, letting the caller decide whether that is fatal.
    pub fn copy_path(&self, from: &Path, relative: &str) -> Result<bool, NpmError> {
        if !from.exists() {
            return Ok(false);
        }
        if from.is_dir() {
            Self::copy_tree(from, &self.root.join(relative))?;
        } else {
            self.copy_file(from, relative)?;
        }
        Ok(true)
    }

    fn create_dir(path: &Path) -> Result<(), NpmError> {
        fs::create_dir_all(path).map_err(|source| NpmError::CreateDir {
            path: path.to_path_buf(),
            source,
        })
    }

    fn copy_one(from: &Path, to: &Path) -> Result<(), NpmError> {
        fs::copy(from, to).map_err(|source| NpmError::Copy {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            source,
        })?;
        debug!(from = %from.display(), to = %to.display(), "copied file");
        Ok(())
    }

    fn copy_tree(from: &Path, to: &Path) -> Result<(), NpmError> {
        Self::create_dir(to)?;
        let entries = fs::read_dir(from).map_err(|source| NpmError::ReadDir {
            path: from.to_path_buf(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| NpmError::ReadDir {
                path: from.to_path_buf(),
                source,
            })?;
            let source = entry.path();
            let destination = to.join(entry.file_name());
            if source.is_dir() {
                Self::copy_tree(&source, &destination)?;
            } else {
                Self::copy_one(&source, &destination)?;
            }
        }
        Ok(())
    }
}
