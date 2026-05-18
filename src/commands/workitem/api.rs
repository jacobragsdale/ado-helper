//! Shared Work Item Tracking helpers used by `wi` and higher-level commands
//! such as `sprint`.

use anyhow::{Context, Result};
use serde_json::json;

use crate::client::AdoClient;

use super::helpers::encode_path;
use super::types::{
    PatchOp, WiComment, WiHistoryResponse, WiqlResult, WiqlWorkItemRef, WorkItem, WorkItemBatch,
};

/// Run a WIQL query and return the raw work item refs.
pub(crate) async fn run_wiql(
    client: &AdoClient,
    project: &str,
    wiql: &str,
    top: Option<u32>,
) -> Result<WiqlResult> {
    let mut wiql_path = format!(
        "{project}/_apis/wit/wiql?api-version=7.1",
        project = encode_path(project)
    );
    if let Some(top) = top {
        wiql_path.push_str(&format!("&$top={top}"));
    }
    client
        .post_json(&wiql_path, &json!({ "query": wiql }))
        .await
}

/// Hydrate WIQL refs into full work items using the requested field list.
pub(crate) async fn fetch_work_items(
    client: &AdoClient,
    refs: &[WiqlWorkItemRef],
    fields: &[&str],
) -> Result<Vec<WorkItem>> {
    let ids: Vec<u32> = refs.iter().map(|w| w.id).collect();
    fetch_work_items_by_ids(client, &ids, fields, None).await
}

/// Hydrate work item IDs in batches of 200, which is the ADO REST API limit.
pub(crate) async fn fetch_work_items_by_ids(
    client: &AdoClient,
    ids: &[u32],
    fields: &[&str],
    expand: Option<&str>,
) -> Result<Vec<WorkItem>> {
    let mut items: Vec<WorkItem> = Vec::with_capacity(ids.len());
    let fields = fields.join(",");
    for chunk in ids.chunks(200) {
        if chunk.is_empty() {
            continue;
        }
        let ids = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut path = format!("_apis/wit/workitems?ids={ids}");
        if !fields.is_empty() {
            path.push_str("&fields=");
            path.push_str(&fields);
        }
        if let Some(expand) = expand {
            path.push_str("&$expand=");
            path.push_str(expand);
        }
        path.push_str("&api-version=7.1");
        let batch: WorkItemBatch = client.get_json(&path).await?;
        items.extend(batch.value);
    }
    Ok(items)
}

/// Patch one work item using ADO's JSON Patch media type.
pub(crate) async fn patch_work_item(
    client: &AdoClient,
    id: u32,
    ops: &[PatchOp],
) -> Result<WorkItem> {
    let path = format!("_apis/wit/workitems/{id}?api-version=7.1");
    client.patch_json_patch(&path, ops).await
}

/// Add a work item comment.
pub(crate) async fn add_comment(
    client: &AdoClient,
    project: &str,
    id: u32,
    text: &str,
) -> Result<WiComment> {
    let path = format!(
        "{project}/_apis/wit/workItems/{id}/comments?api-version=7.1-preview.4",
        project = encode_path(project)
    );
    client.post_json(&path, &json!({ "text": text })).await
}

/// List work item update deltas.
pub(crate) async fn list_updates(
    client: &AdoClient,
    project: &str,
    id: u32,
    top: Option<u32>,
) -> Result<WiHistoryResponse> {
    let mut path = format!(
        "{project}/_apis/wit/workItems/{id}/updates?api-version=7.1",
        project = encode_path(project)
    );
    if let Some(top) = top {
        path.push_str(&format!("&$top={top}"));
    }
    client
        .get_json(&path)
        .await
        .with_context(|| format!("fetching updates for work item #{id}"))
}
