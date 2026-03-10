/// Read a cookie value by name from HTTP headers.
///
/// Handles multiple Cookie headers and semicolon-separated pairs.
pub(crate) fn read_cookie(headers: &http::HeaderMap, cookie_name: &str) -> Option<String> {
    let prefix = format!("{cookie_name}=");
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .find_map(|val| {
            let val = val.to_str().ok()?;
            for pair in val.split(';') {
                let pair = pair.trim();
                if let Some(value) = pair.strip_prefix(&prefix)
                    && !value.is_empty()
                {
                    return Some(value.to_string());
                }
            }
            None
        })
}
