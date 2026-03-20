use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Trait for loading raw email templates (frontmatter + body).
/// Implementations must be `Send + Sync` for use in `Arc<dyn TemplateSource>`.
pub trait TemplateSource: Send + Sync {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String>;
}

/// Loads templates from the filesystem with locale fallback.
///
/// Fallback chain:
/// 1. `{path}/{locale}/{name}.md`
/// 2. `{path}/{default_locale}/{name}.md`
/// 3. `{path}/{name}.md`
/// 4. Error
pub struct FileSource {
    path: PathBuf,
}

impl FileSource {
    pub fn new(templates_path: impl Into<PathBuf>) -> Self {
        Self {
            path: templates_path.into(),
        }
    }

    fn try_load(&self, file_path: &Path) -> Option<String> {
        std::fs::read_to_string(file_path).ok()
    }
}

impl TemplateSource for FileSource {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String> {
        let filename = format!("{name}.md");

        // 1. Exact locale
        let path = self.path.join(locale).join(&filename);
        if let Some(content) = self.try_load(&path) {
            return Ok(content);
        }

        // 2. Default locale (skip if same as exact)
        if locale != default_locale {
            let path = self.path.join(default_locale).join(&filename);
            if let Some(content) = self.try_load(&path) {
                return Ok(content);
            }
        }

        // 3. No-locale fallback
        let path = self.path.join(&filename);
        if let Some(content) = self.try_load(&path) {
            return Ok(content);
        }

        // 4. Error
        Err(Error::not_found(format!(
            "email template '{name}' not found for locale '{locale}'"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_templates(dir: &std::path::Path) {
        // en/welcome.md
        std::fs::create_dir_all(dir.join("en")).unwrap();
        std::fs::write(
            dir.join("en/welcome.md"),
            "---\nsubject: Welcome EN\n---\nEnglish body",
        )
        .unwrap();

        // uk/welcome.md
        std::fs::create_dir_all(dir.join("uk")).unwrap();
        std::fs::write(
            dir.join("uk/welcome.md"),
            "---\nsubject: Welcome UK\n---\nUkrainian body",
        )
        .unwrap();

        // fallback.md (no locale dir)
        std::fs::write(
            dir.join("fallback.md"),
            "---\nsubject: Fallback\n---\nFallback body",
        )
        .unwrap();
    }

    #[test]
    fn load_exact_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("welcome", "uk", "en").unwrap();
        assert!(content.contains("Ukrainian body"));
    }

    #[test]
    fn load_falls_back_to_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("welcome", "fr", "en").unwrap();
        assert!(content.contains("English body"));
    }

    #[test]
    fn load_falls_back_to_no_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("fallback", "fr", "en").unwrap();
        assert!(content.contains("Fallback body"));
    }

    #[test]
    fn load_not_found() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let result = source.load("nonexistent", "en", "en");
        assert!(result.is_err());
    }

    #[test]
    fn load_same_locale_as_default_skips_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        // locale == default_locale, should still find it on first try
        let content = source.load("welcome", "en", "en").unwrap();
        assert!(content.contains("English body"));
    }
}
