//! The npmgen-owned meta `package.json`: identity, computed `files`, and the
//! `optionalDependencies` fan-out to the platform packages.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::project::Project;
use crate::target::Target;

/// Builder of the meta package's `package.json`.
pub struct MetaPackage<'a> {
    project: &'a Project,
    targets: &'a [Target],
}

impl<'a> MetaPackage<'a> {
    pub fn new(project: &'a Project, targets: &'a [Target]) -> Self {
        Self { project, targets }
    }

    /// Render the `package.json`. Object keys serialize sorted (serde_json's
    /// default map), so insertion order is irrelevant.
    pub fn to_value(&self) -> Value {
        let scope = &self.project.identity.scope;
        let name = &self.project.identity.name;

        let mut object = Map::new();
        object.insert("name".to_owned(), json!(self.project.package_name()));
        object.insert("version".to_owned(), json!(self.project.version));
        object.insert("description".to_owned(), json!(self.project.description));
        object.insert("license".to_owned(), json!(self.project.license));
        object.insert("author".to_owned(), json!(self.project.author.full));
        object.insert(
            "repository".to_owned(),
            json!({ "type": "git", "url": self.project.identity.git_url }),
        );
        object.insert("files".to_owned(), json!(self.files()));

        let mut optional = Map::new();
        for target in self.targets {
            optional.insert(
                format!("{scope}/{name}-{}", target.key),
                json!(self.project.version),
            );
        }
        object.insert("optionalDependencies".to_owned(), Value::Object(optional));
        object.insert("publishConfig".to_owned(), json!({ "access": "public" }));

        if let Some(launcher) = &self.project.config.launcher
            && let Some(bin) = launcher.bin()
        {
            object.insert("bin".to_owned(), json!({ bin: launcher.file() }));
        }

        for (key, value) in &self.project.config.extra {
            object.insert(key.clone(), value.clone());
        }
        Value::Object(object)
    }

    /// The npm `files` allow-list: the top-level path segment of every payload
    /// entry (launcher, includes, rendered manifests), deduplicated and sorted.
    fn files(&self) -> Vec<String> {
        let mut entries = BTreeSet::new();
        if let Some(launcher) = &self.project.config.launcher {
            entries.insert(Self::top_segment(launcher.file()));
        }
        for include in &self.project.config.include {
            entries.insert(Self::top_segment(include));
        }
        for manifest in &self.project.config.manifests {
            entries.insert(Self::top_segment(manifest.dest()));
        }
        entries.into_iter().collect()
    }

    fn top_segment(path: &str) -> String {
        path.split(['/', '\\']).next().unwrap_or(path).to_owned()
    }
}
