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
        Self::guard(relative)?;
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
        Self::guard(relative)?;
        let to = self.root.join(relative);
        if let Some(parent) = to.parent() {
            Self::create_dir(parent)?;
        }
        Self::copy_one(from, &to)
    }

    /// Copy a file or directory tree to `relative`. Returns `false` when the
    /// source is absent, letting the caller decide whether that is fatal.
    pub fn copy_path(&self, from: &Path, relative: &str) -> Result<bool, NpmError> {
        Self::guard(relative)?;
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

    /// Reject a destination that is absolute or climbs out of the package via
    /// `..`, so payload always lands inside the package directory.
    fn guard(relative: &str) -> Result<(), NpmError> {
        use std::path::Component;
        let escapes = Path::new(relative).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        if escapes {
            return Err(NpmError::PathEscape {
                path: relative.to_owned(),
            });
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::TreeWriter;
    use crate::npm::NpmError;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("npmgen-writer-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_paths_that_escape_the_package() {
        let dir = scratch("guard");
        let writer = TreeWriter::new(dir.join("pkg"));
        writer.ensure().unwrap();

        assert!(matches!(
            writer.write_string("../escape.json", "x").unwrap_err(),
            NpmError::PathEscape { .. }
        ));
        let absolute = if cfg!(windows) {
            "C:/escape.json"
        } else {
            "/escape.json"
        };
        assert!(matches!(
            writer.write_string(absolute, "x").unwrap_err(),
            NpmError::PathEscape { .. }
        ));

        writer.write_string("nested/ok.json", "x").unwrap();
        assert!(dir.join("pkg/nested/ok.json").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_a_deep_directory_tree() {
        let root = scratch("tree");
        let src = root.join("src");
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("a/b/leaf.txt"), "leaf").unwrap();
        fs::write(src.join("top.txt"), "top").unwrap();

        let writer = TreeWriter::new(root.join("pkg"));
        writer.ensure().unwrap();
        assert!(writer.copy_path(&src, "payload").unwrap());

        assert!(root.join("pkg/payload/a/b/leaf.txt").is_file());
        assert!(root.join("pkg/payload/top.txt").is_file());
        let _ = fs::remove_dir_all(&root);
    }
}
