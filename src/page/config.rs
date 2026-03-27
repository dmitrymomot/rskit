use serde::Deserialize;

/// Pagination defaults applied by [`super::PageRequest`] and
/// [`super::CursorRequest`] extractors.
///
/// Loaded from the `pagination:` section of the YAML config. All fields
/// have sensible defaults, so the section can be omitted entirely.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PaginationConfig {
    /// Default number of items per page when `per_page` is not specified.
    pub default_per_page: u32,
    /// Maximum allowed value for `per_page`. Values above this are clamped.
    pub max_per_page: u32,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_per_page: 20,
            max_per_page: 100,
        }
    }
}
