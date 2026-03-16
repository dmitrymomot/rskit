use super::{EmailTemplate, TemplateProvider};
use std::path::PathBuf;

/// Loads email templates from the filesystem with locale-based fallback.
///
/// Templates are stored as `.md` files with YAML frontmatter. Localized
/// variants live in subdirectories named after the locale (e.g. `de/welcome.md`).
/// When a localized file is not found, the provider falls back to the root
/// template (`welcome.md`).
///
/// Path traversal attempts (names or locales containing `..`, `/`, or `\`)
/// are rejected and return an error.
pub struct FilesystemProvider {
    base_dir: PathBuf,
}

impl FilesystemProvider {
    /// Create a provider rooted at `base_dir`.
    ///
    /// The directory does not need to exist at construction time; errors are
    /// returned when a template is requested and the file cannot be found.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn resolve_path(&self, name: &str, locale: &str) -> Option<PathBuf> {
        // Reject path traversal attempts
        if name.contains("..")
            || name.contains('/')
            || name.contains('\\')
            || locale.contains("..")
            || locale.contains('/')
            || locale.contains('\\')
        {
            return None;
        }

        if !locale.is_empty() {
            let localized = self.base_dir.join(locale).join(format!("{name}.md"));
            if localized.is_file() {
                return Some(localized);
            }
        }

        let root = self.base_dir.join(format!("{name}.md"));
        if root.is_file() {
            return Some(root);
        }

        None
    }
}

impl TemplateProvider for FilesystemProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        let path = self
            .resolve_path(name, locale)
            .ok_or_else(|| modo::Error::internal(format!("email template not found: {name}")))?;

        let raw = std::fs::read_to_string(&path).map_err(|e| {
            modo::Error::internal(format!("failed to read template {}: {e}", path.display()))
        })?;

        EmailTemplate::parse(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateProvider;
    use std::fs;

    #[test]
    fn load_template_no_locale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        )
        .unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "").unwrap();
        assert_eq!(tpl.subject, "Hi");
    }

    #[test]
    fn load_template_with_locale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::create_dir_all(path.join("de")).unwrap();
        fs::write(
            path.join("de/welcome.md"),
            "---\nsubject: \"Hallo\"\n---\n\nHallo!",
        )
        .unwrap();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        )
        .unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "de").unwrap();
        assert_eq!(tpl.subject, "Hallo");
    }

    #[test]
    fn locale_fallback_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        )
        .unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "fr").unwrap();
        assert_eq!(tpl.subject, "Hi");
    }

    #[test]
    fn template_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("missing", "");
        assert!(result.is_err());
    }

    #[test]
    fn path_traversal_in_name_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("../secret", "");
        assert!(result.is_err());
    }

    #[test]
    fn path_traversal_in_locale_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("welcome", "../../etc");
        assert!(result.is_err());
    }

    #[test]
    fn backslash_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("..\\secret", "");
        assert!(result.is_err());
    }

    #[test]
    fn forward_slash_in_name_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("sub/template", "");
        assert!(result.is_err());
    }

    #[test]
    fn empty_locale_uses_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Root\"\n---\n\nRoot body",
        )
        .unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "").unwrap();
        assert_eq!(tpl.subject, "Root");
    }

    #[test]
    fn name_with_md_extension() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        // "welcome.md" → tries "welcome.md.md" → not found
        let result = provider.get("welcome.md", "");
        assert!(result.is_err());
    }
}
