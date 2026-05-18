//! Small wi-specific helpers used across `flags` and `handlers`.

use anyhow::{Context, Result};

use crate::client::{AdoClient, encode_path_segment};
use crate::commands::me;
use crate::config::Config;

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

/// Resolve a "me" placeholder. Prefers the on-disk cache written by
/// `ado me`; falls back to a live `_apis/connectionData` fetch (and warms
/// the cache while it's there). Other inputs pass through unchanged.
pub async fn resolve_user(client: &AdoClient, who: &str) -> Result<String> {
    if !who.eq_ignore_ascii_case("me") {
        return Ok(who.to_string());
    }
    if let Ok(cfg) = Config::load() {
        if let Some(id) = cfg.identity {
            if let Some(s) = preferred_handle(&id) {
                return Ok(s);
            }
        }
    }
    let me = me::fetch_identity(client)
        .await
        .context("could not fetch connectionData to resolve 'me'")?;
    if let Ok(mut cfg) = Config::load() {
        cfg.identity = Some(me.clone());
        let _ = cfg.save();
    }
    preferred_handle(&me).context("could not resolve current user from connectionData")
}

fn preferred_handle(id: &me::MeInfo) -> Option<String> {
    if !id.unique_name.is_empty() {
        return Some(id.unique_name.clone());
    }
    if !id.display_name.is_empty() {
        return Some(id.display_name.clone());
    }
    None
}
