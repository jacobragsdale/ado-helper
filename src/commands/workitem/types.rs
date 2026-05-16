//! ADO API response shapes for work item endpoints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: u32,
    pub url: String,
    pub fields: serde_json::Value,
    #[serde(default)]
    pub relations: Vec<Relation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Relation {
    pub rel: String,
    pub url: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkItemBatch {
    pub value: Vec<WorkItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WiqlResult {
    #[serde(rename = "workItems")]
    pub work_items: Vec<WiqlWorkItemRef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WiqlWorkItemRef {
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub id: String,
    pub url: String,
}

/// One JSON Patch operation (`{op, path, value}`). All work item write
/// endpoints accept arrays of these with `Content-Type: application/json-patch+json`.
#[derive(Serialize)]
pub struct PatchOp {
    pub op: String,
    pub path: String,
    pub value: serde_json::Value,
}
