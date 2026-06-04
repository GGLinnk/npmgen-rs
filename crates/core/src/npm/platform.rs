//! A platform package's `package.json`: the npm `os`/`cpu` install filters and
//! the single binary file it carries.

use serde_json::{Map, Value, json};

use crate::project::Project;
use crate::target::Target;

/// Builder of one platform package's `package.json`.
pub struct PlatformPackage<'a> {
    project: &'a Project,
    target: &'a Target,
}

impl<'a> PlatformPackage<'a> {
    pub fn new(project: &'a Project, target: &'a Target) -> Self {
        Self { project, target }
    }

    /// Render the `package.json` for `@scope/name-<key>`. The shipped binary
    /// file is named after the npm package (what the launcher resolves), not the
    /// cargo bin.
    pub fn to_value(&self) -> Value {
        let name = &self.project.identity.name;
        let binary = self.target.binary_filename(name);

        let mut object = Map::new();
        object.insert(
            "name".to_owned(),
            json!(format!(
                "{}/{}-{}",
                self.project.identity.scope, name, self.target.key
            )),
        );
        object.insert("version".to_owned(), json!(self.project.version));
        object.insert(
            "description".to_owned(),
            json!(format!("{name} binary for {}.", self.target.key)),
        );
        object.insert("license".to_owned(), json!(self.project.license));
        object.insert("os".to_owned(), json!([self.target.os]));
        object.insert("cpu".to_owned(), json!([self.target.cpu]));
        object.insert("files".to_owned(), json!([binary]));
        Value::Object(object)
    }
}
