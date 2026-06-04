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

#[cfg(test)]
mod tests {
    use super::MetaPackage;
    use crate::config::Launcher;
    use crate::project::sample_project;
    use crate::target::Target;
    use serde_json::json;

    #[test]
    fn wires_launcher_bin_and_merges_extra_over_computed_fields() {
        let mut project = sample_project();
        project.config.launcher = Some(Launcher::Detailed {
            file: "launch.mjs".to_owned(),
            bin: Some("mytool".to_owned()),
        });
        project
            .config
            .extra
            .insert("keywords".to_owned(), json!(["hook"]));
        // extra deliberately overrides a computed identity field.
        project
            .config
            .extra
            .insert("license".to_owned(), json!("Apache-2.0"));

        let targets = [Target::from_triple("x86_64-unknown-linux-gnu").unwrap()];
        let value = MetaPackage::new(&project, &targets).to_value();

        assert_eq!(value["bin"], json!({ "mytool": "launch.mjs" }));
        assert_eq!(value["keywords"], json!(["hook"]));
        assert_eq!(value["license"], json!("Apache-2.0"));
        assert_eq!(
            value["optionalDependencies"]["@gglinnk/nocmd-linux-x64"],
            json!("0.1.1")
        );
    }

    #[test]
    fn omits_bin_when_launcher_declares_none() {
        let mut project = sample_project();
        project.config.launcher = Some(Launcher::File("launch.mjs".to_owned()));
        let value = MetaPackage::new(&project, &[]).to_value();
        assert!(value.get("bin").is_none());
        assert_eq!(value["files"], json!(["launch.mjs"]));
    }
}
