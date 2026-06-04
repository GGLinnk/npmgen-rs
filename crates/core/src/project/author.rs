/// An author entry split out of `Name <email>` form. `full` is the verbatim
/// entry used as npm's string `author`; `name`/`email` feed manifests that want
/// the structured object form.
#[derive(Debug, Clone, Default)]
pub struct Author {
    pub full: String,
    pub name: String,
    pub email: Option<String>,
}

impl Author {
    /// Parse a single author entry. An empty input yields empty fields.
    pub fn parse(full: &str) -> Self {
        let full = full.trim().to_owned();
        match full.split_once('<') {
            Some((name, email)) => Self {
                name: name.trim().to_owned(),
                email: Some(email.trim_end_matches('>').trim().to_owned()),
                full,
            },
            None => Self {
                name: full.clone(),
                email: None,
                full,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_name_and_email() {
        let author = Author::parse("Gabriel GRONDIN <gglinnk@protonmail.com>");
        assert_eq!(author.full, "Gabriel GRONDIN <gglinnk@protonmail.com>");
        assert_eq!(author.name, "Gabriel GRONDIN");
        assert_eq!(author.email.as_deref(), Some("gglinnk@protonmail.com"));
    }

    #[test]
    fn name_only_has_no_email() {
        let author = Author::parse("Gabriel GRONDIN");
        assert_eq!(author.name, "Gabriel GRONDIN");
        assert_eq!(author.email, None);
    }

    #[test]
    fn empty_is_empty() {
        let author = Author::parse("");
        assert_eq!(author.full, "");
        assert_eq!(author.email, None);
    }
}
