//! Shared helpers for `--field NAME=VALUE` parsing and value coercion. Both
//! `wi` and `pr` use these — keep them dependency-free so they compose cleanly
//! with each module's own alias map.

use anyhow::{Context, Result};
use serde_json::json;

/// Split a `NAME=VALUE` argument into its parts. Errors with a clear message
/// if no `=` is present.
pub fn split_field_arg(entry: &str) -> Result<(&str, &str)> {
    entry
        .split_once('=')
        .with_context(|| format!("--field expects NAME=VALUE, got: {entry}"))
}

/// Coerce a stringly-typed value into the JSON shape ADO expects:
/// `true`/`false` → bool, integer-parseable → integer, float-parseable → float,
/// otherwise → string. An empty input becomes an empty string (not null) so
/// callers can clear fields by passing `--field foo=`.
pub fn coerce_value(raw: &str) -> serde_json::Value {
    if raw.is_empty() {
        return json!("");
    }
    if raw.eq_ignore_ascii_case("true") {
        return json!(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return json!(false);
    }
    if let Ok(i) = raw.parse::<i64>() {
        return json!(i);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return json!(f);
    }
    json!(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_field_arg_basic() {
        assert_eq!(split_field_arg("a=b").unwrap(), ("a", "b"));
        assert_eq!(split_field_arg("a=b=c").unwrap(), ("a", "b=c"));
        assert!(split_field_arg("nope").is_err());
    }

    #[test]
    fn coerce_bools_numbers_strings() {
        assert_eq!(coerce_value("true"), json!(true));
        assert_eq!(coerce_value("FALSE"), json!(false));
        assert_eq!(coerce_value("42"), json!(42));
        assert_eq!(coerce_value("3.14"), json!(3.14));
        assert_eq!(coerce_value("hello"), json!("hello"));
        assert_eq!(coerce_value(""), json!(""));
    }
}
