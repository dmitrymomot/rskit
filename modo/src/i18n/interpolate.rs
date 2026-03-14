/// Replace `{key}` placeholders in `template` with values from `vars`.
///
/// Replacements are applied sequentially via [`str::replace`], so:
/// - A substituted value containing `{key}` may be expanded by a later iteration.
/// - If one key is a substring of another (e.g. `name` vs `name_display`), the
///   shorter key may match inside the longer placeholder, causing unintended
///   replacements. Callers should avoid such overlapping key names.
pub(crate) fn interpolate<K, V>(template: &str, vars: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key.as_ref()), value.as_ref());
    }
    result
}
