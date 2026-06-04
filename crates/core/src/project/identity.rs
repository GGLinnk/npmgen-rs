use super::ProjectError;

/// Package identity derived from the repository URL: the npm scope (`@owner`),
/// the npm/package name (repository basename), and the npm git URL. Assumes the
/// npm scope matches the repository owner unless overridden.
#[derive(Debug, Clone)]
pub struct Identity {
    pub scope: String,
    pub name: String,
    pub git_url: String,
}

impl Identity {
    /// Derive identity from a repository URL. `scope_override` (e.g. `@acme`) is
    /// used verbatim when present; otherwise the scope is `@<owner>`.
    pub fn from_repository(
        repository: &str,
        scope_override: Option<&str>,
    ) -> Result<Self, ProjectError> {
        let path = repository
            .trim()
            .trim_end_matches('/')
            .trim_end_matches(".git");
        let mut segments = path.rsplit('/');
        let name = segments.next().unwrap_or_default();
        let owner = segments.next().unwrap_or_default();
        if owner.is_empty() || name.is_empty() {
            return Err(ProjectError::MissingRepository);
        }
        Ok(Self {
            scope: scope_override
                .map(str::to_owned)
                .unwrap_or_else(|| format!("@{owner}")),
            name: name.to_owned(),
            git_url: format!("git+{path}.git"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_scope_name_and_git_url() {
        let identity = Identity::from_repository("https://github.com/gglinnk/nocmd", None).unwrap();
        assert_eq!(identity.scope, "@gglinnk");
        assert_eq!(identity.name, "nocmd");
        assert_eq!(identity.git_url, "git+https://github.com/gglinnk/nocmd.git");
    }

    #[test]
    fn tolerates_trailing_slash_and_dot_git() {
        let identity =
            Identity::from_repository("https://github.com/acme/tool.git/", None).unwrap();
        assert_eq!(identity.scope, "@acme");
        assert_eq!(identity.name, "tool");
        assert_eq!(identity.git_url, "git+https://github.com/acme/tool.git");
    }

    #[test]
    fn scope_override_is_used_verbatim() {
        let identity =
            Identity::from_repository("https://github.com/gglinnk/tool", Some("@other")).unwrap();
        assert_eq!(identity.scope, "@other");
    }

    #[test]
    fn missing_owner_is_rejected() {
        assert!(Identity::from_repository("nocmd", None).is_err());
        assert!(Identity::from_repository("", None).is_err());
    }
}
