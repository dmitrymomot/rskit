use std::fmt;

#[derive(Debug)]
pub enum I18nError {
    DirectoryNotFound {
        path: String,
    },
    DefaultLangMissing {
        lang: String,
        path: String,
    },
    ParseError {
        lang: String,
        file: String,
        source: serde_yaml_ng::Error,
    },
    PluralMissingOther {
        lang: String,
        key: String,
    },
    ReadError {
        lang: String,
        file: String,
        source: std::io::Error,
    },
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryNotFound { path } => {
                write!(f, "i18n: translations directory not found: {path}")
            }
            Self::DefaultLangMissing { lang, path } => {
                write!(
                    f,
                    "i18n: default language '{lang}' directory not found in {path}"
                )
            }
            Self::ParseError { lang, file, source } => {
                write!(f, "i18n: failed to parse {lang}/{file}: {source}")
            }
            Self::PluralMissingOther { lang, key } => {
                write!(
                    f,
                    "i18n: plural entry '{key}' in '{lang}' missing required 'other' key"
                )
            }
            Self::ReadError { lang, file, source } => {
                write!(f, "i18n: failed to read {lang}/{file}: {source}")
            }
        }
    }
}

impl std::error::Error for I18nError {}
