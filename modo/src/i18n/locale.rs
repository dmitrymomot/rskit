/// Normalize a language tag to a bare lowercase language code.
/// "en-US" -> "en", "pt_BR" -> "pt", "DE" -> "de"
pub fn normalize_lang(tag: &str) -> String {
    tag.split(['-', '_']).next().unwrap_or(tag).to_lowercase()
}

/// Parse an Accept-Language header into a list of normalized language codes,
/// sorted by quality weight (descending), deduplicated, with "*" filtered out.
pub fn parse_accept_language(header: &str) -> Vec<String> {
    let mut entries: Vec<(String, f32)> = header
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (tag, q) = if let Some((tag, params)) = part.split_once(';') {
                let q = params
                    .trim()
                    .strip_prefix("q=")
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0);
                (tag.trim(), q)
            } else {
                (part, 1.0)
            };
            let normalized = normalize_lang(tag);
            if normalized == "*" {
                return None;
            }
            Some((normalized, q))
        })
        .collect();

    // Stable sort descending by weight
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Deduplicate, keeping first (highest weight)
    let mut seen = Vec::new();
    let mut result = Vec::new();
    for (lang, _) in entries {
        if !seen.contains(&lang) {
            seen.push(lang.clone());
            result.push(lang);
        }
    }
    result
}

/// Find the first language from Accept-Language header that matches an available locale.
pub fn resolve_from_accept_language(header: &str, available: &[String]) -> Option<String> {
    parse_accept_language(header)
        .into_iter()
        .find(|lang| available.contains(lang))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_region() {
        assert_eq!(normalize_lang("en-US"), "en");
        assert_eq!(normalize_lang("es-MX"), "es");
        assert_eq!(normalize_lang("pt_BR"), "pt");
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_lang("EN"), "en");
        assert_eq!(normalize_lang("De-AT"), "de");
    }

    #[test]
    fn normalize_plain() {
        assert_eq!(normalize_lang("fr"), "fr");
    }

    #[test]
    fn parse_accept_language_with_weights() {
        let result = parse_accept_language("fr-CH, fr;q=0.9, en;q=0.8, de;q=0.7, *;q=0.5");
        assert_eq!(result, vec!["fr", "en", "de"]);
    }

    #[test]
    fn parse_accept_language_deduplicates() {
        let result = parse_accept_language("en-US, en-GB;q=0.9, en;q=0.8");
        assert_eq!(result, vec!["en"]);
    }

    #[test]
    fn parse_accept_language_default_weight() {
        let result = parse_accept_language("es, en;q=0.5");
        assert_eq!(result, vec!["es", "en"]);
    }

    #[test]
    fn parse_accept_language_empty() {
        let result = parse_accept_language("");
        assert!(result.is_empty());
    }

    #[test]
    fn resolve_first_available_match() {
        let available = vec!["en".to_string(), "de".to_string()];
        let result = resolve_from_accept_language("fr, de;q=0.9, en;q=0.8", &available);
        assert_eq!(result, Some("de".to_string()));
    }

    #[test]
    fn resolve_no_match() {
        let available = vec!["en".to_string()];
        let result = resolve_from_accept_language("fr, de", &available);
        assert_eq!(result, None);
    }
}
