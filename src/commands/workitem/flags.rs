//! Field-bearing flags shared by `wi create` and `wi update`, plus the
//! alias map and `build_field_ops` that turns a `FieldFlags` into a JSON Patch
//! body.

use anyhow::{Result, bail};
use clap::Args;
use serde_json::json;

use crate::client::AdoClient;
use crate::fields::{coerce_value, split_field_arg};

use super::helpers::resolve_user;
use super::types::PatchOp;

/// Common field flags that map to specific ADO fields. Used by both create and update.
#[derive(Args, Default, Debug)]
pub struct FieldFlags {
    /// Title (System.Title)
    #[arg(long)]
    pub title: Option<String>,

    /// State (System.State) — e.g. "To Do", "Doing", "Done", "Active", "Closed"
    #[arg(long)]
    pub state: Option<String>,

    /// Reassign to user (use "me" for yourself, "" to unassign)
    #[arg(long)]
    pub assigned_to: Option<String>,

    /// Description (System.Description) — HTML allowed
    #[arg(long)]
    pub description: Option<String>,

    /// Iteration path
    #[arg(long)]
    pub iteration: Option<String>,

    /// Area path
    #[arg(long)]
    pub area: Option<String>,

    /// Tags — semicolon-separated, e.g. "ux;blocker;p0"
    #[arg(long)]
    pub tags: Option<String>,

    /// Priority (1–4)
    #[arg(long)]
    pub priority: Option<String>,

    /// Severity (Bug)
    #[arg(long)]
    pub severity: Option<String>,

    /// Story Points
    #[arg(long)]
    pub story_points: Option<String>,

    /// Acceptance Criteria — HTML allowed
    #[arg(long)]
    pub acceptance_criteria: Option<String>,

    /// Generic field set, repeatable. Use either short alias or full ADO name.
    /// Examples: --field tags=foo --field Microsoft.VSTS.Common.Activity=Development
    #[arg(long, value_name = "NAME=VALUE")]
    pub field: Vec<String>,
}

/// Map a short alias (e.g. "tags", "story-points") to its full ADO field name.
/// If `name` already contains a `.`, return it as-is.
pub fn resolve_field_name(name: &str) -> Result<String> {
    if name.contains('.') {
        return Ok(name.to_string());
    }
    let key = name.trim().to_ascii_lowercase().replace('_', "-");
    Ok(match key.as_str() {
        "title"               => "System.Title",
        "state"               => "System.State",
        "reason"              => "System.Reason",
        "description"         => "System.Description",
        "assigned-to"         => "System.AssignedTo",
        "iteration"           => "System.IterationPath",
        "iteration-path"      => "System.IterationPath",
        "area"                => "System.AreaPath",
        "area-path"           => "System.AreaPath",
        "tags"                => "System.Tags",
        "history"             => "System.History",
        "priority"            => "Microsoft.VSTS.Common.Priority",
        "severity"            => "Microsoft.VSTS.Common.Severity",
        "activity"            => "Microsoft.VSTS.Common.Activity",
        "value-area"          => "Microsoft.VSTS.Common.ValueArea",
        "risk"                => "Microsoft.VSTS.Common.Risk",
        "stack-rank"          => "Microsoft.VSTS.Common.StackRank",
        "acceptance-criteria" => "Microsoft.VSTS.Common.AcceptanceCriteria",
        "story-points"        => "Microsoft.VSTS.Scheduling.StoryPoints",
        "effort"              => "Microsoft.VSTS.Scheduling.Effort",
        "original-estimate"   => "Microsoft.VSTS.Scheduling.OriginalEstimate",
        "remaining-work"      => "Microsoft.VSTS.Scheduling.RemainingWork",
        "completed-work"      => "Microsoft.VSTS.Scheduling.CompletedWork",
        "start-date"          => "Microsoft.VSTS.Scheduling.StartDate",
        "target-date"         => "Microsoft.VSTS.Scheduling.TargetDate",
        "repro-steps"         => "Microsoft.VSTS.TCM.ReproSteps",
        "system-info"         => "Microsoft.VSTS.TCM.SystemInfo",
        other => bail!("unknown field alias '{other}' — pass the full ADO field name (e.g. Microsoft.VSTS.Common.Foo)"),
    }.to_string())
}

/// Build the full Vec<PatchOp> from a FieldFlags. Resolves "me" via connectionData
/// when assigned_to == "me". Generic --field entries override named flags if both
/// target the same ADO field (last writer wins through the BTreeMap).
pub async fn build_field_ops(f: &FieldFlags, client: &AdoClient) -> Result<Vec<PatchOp>> {
    use std::collections::BTreeMap;
    let mut by_field: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    if let Some(v) = &f.title {
        by_field.insert("System.Title".into(), json!(v));
    }
    if let Some(v) = &f.state {
        by_field.insert("System.State".into(), json!(v));
    }
    if let Some(v) = &f.description {
        by_field.insert("System.Description".into(), json!(v));
    }
    if let Some(v) = &f.iteration {
        by_field.insert("System.IterationPath".into(), json!(v));
    }
    if let Some(v) = &f.area {
        by_field.insert("System.AreaPath".into(), json!(v));
    }
    if let Some(v) = &f.tags {
        by_field.insert("System.Tags".into(), json!(v));
    }
    if let Some(v) = &f.priority {
        by_field.insert("Microsoft.VSTS.Common.Priority".into(), coerce_value(v));
    }
    if let Some(v) = &f.severity {
        by_field.insert("Microsoft.VSTS.Common.Severity".into(), json!(v));
    }
    if let Some(v) = &f.story_points {
        by_field.insert(
            "Microsoft.VSTS.Scheduling.StoryPoints".into(),
            coerce_value(v),
        );
    }
    if let Some(v) = &f.acceptance_criteria {
        by_field.insert("Microsoft.VSTS.Common.AcceptanceCriteria".into(), json!(v));
    }
    if let Some(who) = &f.assigned_to {
        let resolved = if who.is_empty() {
            json!("")
        } else {
            json!(resolve_user(client, who).await?)
        };
        by_field.insert("System.AssignedTo".into(), resolved);
    }

    for entry in &f.field {
        let (name, value) = split_field_arg(entry)?;
        let resolved_name = resolve_field_name(name)?;
        let coerced = coerce_value(value);
        // For System.AssignedTo via --field, also resolve "me".
        let final_value = if resolved_name == "System.AssignedTo" {
            if let Some(s) = coerced.as_str() {
                if s.is_empty() {
                    json!("")
                } else {
                    json!(resolve_user(client, s).await?)
                }
            } else {
                coerced
            }
        } else {
            coerced
        };
        by_field.insert(resolved_name, final_value);
    }

    Ok(by_field
        .into_iter()
        .map(|(name, value)| PatchOp {
            op: "add".into(),
            path: format!("/fields/{name}"),
            value,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_resolution() {
        assert_eq!(resolve_field_name("title").unwrap(), "System.Title");
        assert_eq!(
            resolve_field_name("story-points").unwrap(),
            "Microsoft.VSTS.Scheduling.StoryPoints"
        );
        assert_eq!(
            resolve_field_name("story_points").unwrap(),
            "Microsoft.VSTS.Scheduling.StoryPoints"
        );
        // already-qualified passes through
        assert_eq!(
            resolve_field_name("System.Custom.Foo").unwrap(),
            "System.Custom.Foo"
        );
        assert!(resolve_field_name("nope").is_err());
    }
}
