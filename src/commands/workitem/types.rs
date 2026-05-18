//! ADO API response shapes for work item endpoints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkItem {
    pub id: u32,
    pub url: String,
    /// Raw ADO field dictionary — keys are full reference names like `System.Title`.
    pub fields: serde_json::Value,
    #[serde(default)]
    pub relations: Vec<Relation>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    pub rel: String,
    pub url: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkItemBatch {
    pub value: Vec<WorkItem>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiqlResult {
    #[serde(default, rename = "workItems")]
    pub work_items: Vec<WiqlWorkItemRef>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiqlWorkItemRef {
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentRef {
    pub id: String,
    pub url: String,
}

/// Single comment on a work item.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiComment {
    pub id: u64,
    #[serde(default, rename = "workItemId")]
    pub work_item_id: Option<u32>,
    #[serde(default)]
    pub text: String,
    #[serde(default, rename = "createdBy")]
    pub created_by: Option<IdentityRef>,
    #[serde(default, rename = "createdDate")]
    pub created_date: Option<String>,
    #[serde(default, rename = "modifiedDate")]
    pub modified_date: Option<String>,
    #[serde(default)]
    pub version: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Comments endpoint response wrapper.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiCommentList {
    #[serde(default)]
    pub comments: Vec<WiComment>,
    #[serde(default)]
    pub count: u32,
    #[serde(default, rename = "totalCount")]
    pub total_count: Option<u32>,
}

/// Reusable identity reference. Mirrors the shape ADO returns under `createdBy`,
/// `revisedBy`, etc.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct IdentityRef {
    #[serde(default, rename = "displayName")]
    pub display_name: String,
    #[serde(default, rename = "uniqueName")]
    pub unique_name: String,
    #[serde(default)]
    pub id: String,
}

/// Combined result of `ado wi attach`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AttachResult {
    pub attachment: AttachmentRef,
    #[serde(rename = "workItem")]
    pub work_item: WorkItem,
}

/// One revision in a work item's history.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiRevision {
    #[serde(default)]
    pub rev: u32,
    #[serde(default, rename = "revisedBy")]
    pub revised_by: Option<IdentityRef>,
    #[serde(default, rename = "revisedDate")]
    pub revised_date: Option<String>,
    /// Map of field name → `{ oldValue, newValue }`. Field names are dynamic, so this is left raw.
    #[serde(default)]
    pub fields: serde_json::Value,
    #[serde(default)]
    pub relations: serde_json::Value,
}

/// `ado wi history` envelope.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WiHistoryResponse {
    #[serde(default)]
    pub value: Vec<WiRevision>,
    #[serde(default)]
    pub count: u32,
}

/// One JSON Patch operation (`{op, path, value}`). All work item write
/// endpoints accept arrays of these with `Content-Type: application/json-patch+json`.
#[derive(Serialize)]
pub struct PatchOp {
    pub op: String,
    pub path: String,
    pub value: serde_json::Value,
}

// ── metadata: types / states / fields ───────────────────────────────────────

/// A work item type definition (`Bug`, `Task`, `User Story`, …) as returned
/// by `_apis/wit/workitemtypes`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkItemTypeInfo {
    pub name: String,
    #[serde(default, rename = "referenceName")]
    pub reference_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default, rename = "isDisabled")]
    pub is_disabled: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkItemTypeListResponse {
    pub value: Vec<WorkItemTypeInfo>,
    pub count: u32,
}

/// A single state on a work item type (returned by `…/workitemtypes/{type}/states`).
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StateInfo {
    pub name: String,
    #[serde(default)]
    pub color: String,
    /// One of `Proposed`, `InProgress`, `Resolved`, `Completed`, `Removed`.
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub order: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StateListResponse {
    pub value: Vec<StateInfo>,
    pub count: u32,
}

/// A field definition (returned by `_apis/wit/fields` or by the per-type
/// fields endpoint). `reference_name` is the canonical identifier used in
/// WIQL queries and JSON Patch ops.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FieldInfo {
    pub name: String,
    #[serde(default, rename = "referenceName")]
    pub reference_name: String,
    #[serde(default, rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub usage: String,
    #[serde(default, rename = "readOnly")]
    pub read_only: bool,
    #[serde(default, rename = "canSortBy")]
    pub can_sort_by: bool,
    #[serde(default, rename = "isIdentity")]
    pub is_identity: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FieldListResponse {
    pub value: Vec<FieldInfo>,
    pub count: u32,
}
