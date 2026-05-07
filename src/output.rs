use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    /// Plain text, one item per line (default)
    Text,
    /// JSON — full API response, useful for scripting
    Json,
}

/*
 * IMPLEMENTATION NOTES — print_text() / print_json()
 *
 * Every command handler receives an &OutputFormat and calls one of these
 * functions to print its results. This keeps formatting logic out of commands.
 *
 * print_text(lines):
 *   Iterate over `lines` and println! each one. The caller is responsible for
 *   formatting each item into a human-readable string beforehand.
 *   Example output for `ado repo list`:
 *     my-repo
 *     another-repo
 *     third-repo
 *
 * print_json(value):
 *   Use serde_json::to_string_pretty(value)? and println! the result.
 *   The caller passes the raw deserialized API response struct (which derives
 *   Serialize) so the output matches the full ADO response shape.
 */

pub fn print_text(lines: &[String]) {
    todo!("println! each line")
}

pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    todo!("serde_json::to_string_pretty and println!")
}
