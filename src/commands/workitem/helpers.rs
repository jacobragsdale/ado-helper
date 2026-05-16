//! Small wi-specific helpers used across `flags` and `handlers`.

use anyhow::{Context, Result, bail};

use crate::client::{AdoClient, encode_path_segment};

/// Read a string field from a work item's `fields` JSON value.
pub fn field_str<'a>(fields: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    fields.get(key).and_then(|v| v.as_str())
}

/// Build the API URL for a work item — used as the `url` of a relation.
pub fn workitem_url(client: &AdoClient, id: u32) -> String {
    format!("{}/_apis/wit/workItems/{id}", client.org)
}

/// Percent-encode a path segment (work item type, file name).
pub fn encode_path(s: &str) -> String {
    encode_path_segment(s)
}

/// Escape single quotes for inclusion in a WIQL string literal.
pub fn escape_wiql(s: &str) -> String {
    s.replace('\'', "''")
}

/// Resolve a "me" placeholder via `_apis/connectionData`. Other inputs pass
/// through unchanged.
pub async fn resolve_user(client: &AdoClient, who: &str) -> Result<String> {
    if !who.eq_ignore_ascii_case("me") {
        return Ok(who.to_string());
    }
    let v: serde_json::Value = client
        .get_json("_apis/connectionData?api-version=7.1-preview.1")
        .await
        .context("could not fetch connectionData to resolve 'me'")?;
    let user = &v["authenticatedUser"];
    if let Some(email) = user["properties"]["Account"]["$value"].as_str() {
        return Ok(email.to_string());
    }
    if let Some(name) = user["providerDisplayName"].as_str() {
        return Ok(name.to_string());
    }
    bail!("could not resolve current user from connectionData")
}
