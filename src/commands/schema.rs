//! `ado schema` — emit the JSON output schema for a given command path.
//!
//! Agents can introspect the shape of any `--output json` payload without
//! scraping the README. The registry is a static array of
//! `(command_path, fn() -> RootSchema)` pairs. Each handler's typed output
//! struct contributes one entry; unregistered paths return
//! `CliError::NotFound` (exit 2).
//!
//! Adding a new command:
//!  1. Make sure the handler's output struct derives `Serialize + JsonSchema`.
//!  2. Add a `("<command path>", || schema_for!(<Type>))` row below.

use anyhow::Result;
use clap::Args;
use schemars::schema::RootSchema;
use schemars::schema_for;

use crate::commands::{area, iteration, me, pipeline, pr, repo, sprint, team, workitem};
use crate::error::CliError;
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado schema --list\n  ado schema me\n  ado schema wi view\n  ado schema iteration current\n\nThe command path matches what you'd type after `ado` (e.g. `wi view`, `pipeline runs`). Use --list to discover every registered path."
)]
pub struct SchemaArgs {
    /// Command path to look up (e.g. `wi view`, `iteration list`, `me`).
    /// Omit and pass `--list` to enumerate all registered schemas.
    #[arg(value_name = "COMMAND", conflicts_with = "list")]
    pub command: Vec<String>,

    /// List every command path with a registered schema
    #[arg(long, conflicts_with = "command")]
    pub list: bool,
}

type SchemaFn = fn() -> RootSchema;

/// Single source of truth for `--output json` schemas. Order is the order
/// `--list` prints. Group entries by top-level command for readability.
fn registry() -> Vec<(&'static str, SchemaFn)> {
    vec![
        // Foundation
        ("me", || schema_for!(me::MeInfo)),
        ("team list", || schema_for!(team::TeamListResponse)),
        ("team members", || schema_for!(team::TeamMembersResponse)),
        ("iteration list", || {
            schema_for!(iteration::IterationListResponse)
        }),
        ("iteration current", || {
            schema_for!(iteration::TeamIteration)
        }),
        ("iteration next", || schema_for!(iteration::TeamIteration)),
        ("iteration view", || schema_for!(iteration::TeamIteration)),
        ("area list", || schema_for!(Vec<String>)),
        ("area tree", || schema_for!(area::AreaNode)),
        // Repos
        ("repo list", || schema_for!(repo::RepoListResponse)),
        ("repo create", || schema_for!(repo::Repository)),
        ("repo branches", || schema_for!(repo::GitRefListResponse)),
        ("repo tags", || schema_for!(repo::GitRefListResponse)),
        ("repo commits", || schema_for!(repo::GitCommitListResponse)),
        // Pull requests
        ("pr list", || schema_for!(pr::PrListResponse)),
        ("pr view", || schema_for!(pr::PullRequest)),
        ("pr create", || schema_for!(pr::PullRequest)),
        ("pr update", || schema_for!(pr::PullRequest)),
        ("pr approve", || schema_for!(pr::PrReviewer)),
        ("pr complete", || schema_for!(pr::PullRequest)),
        ("pr abandon", || schema_for!(pr::PullRequest)),
        ("pr reactivate", || schema_for!(pr::PullRequest)),
        ("pr checks", || schema_for!(pr::PolicyEvaluationsResponse)),
        ("pr threads", || schema_for!(pr::PrThreadListResponse)),
        ("pr thread-reply", || schema_for!(pr::PrThread)),
        ("pr thread-resolve", || schema_for!(pr::PrThread)),
        ("pr comment", || schema_for!(pr::PrThread)),
        ("pr link-work-item", || {
            schema_for!(Vec<workitem::types::WorkItem>)
        }),
        // Pipelines
        ("pipeline list", || {
            schema_for!(pipeline::PipelineListResponse)
        }),
        ("pipeline run", || schema_for!(pipeline::PipelineRun)),
        ("pipeline runs", || {
            schema_for!(pipeline::PipelineRunListResponse)
        }),
        ("pipeline status", || schema_for!(pipeline::PipelineRun)),
        ("pipeline logs", || {
            schema_for!(pipeline::PipelineLogListResponse)
        }),
        ("pipeline preview", || schema_for!(pipeline::PreviewRun)),
        // Sprint planning
        ("sprint backlog", || {
            schema_for!(sprint::SprintBacklogResponse)
        }),
        ("sprint board", || schema_for!(sprint::SprintBoardResponse)),
        ("sprint plan-into", || {
            schema_for!(sprint::SprintPlanIntoResponse)
        }),
        ("sprint capacity", || {
            schema_for!(sprint::SprintCapacityResponse)
        }),
        ("sprint capacity set", || {
            schema_for!(sprint::SprintCapacitySetResponse)
        }),
        ("sprint burndown", || {
            schema_for!(sprint::SprintBurndownResponse)
        }),
        ("sprint rollover", || {
            schema_for!(sprint::SprintRolloverResponse)
        }),
        ("sprint summary", || {
            schema_for!(sprint::SprintSummaryResponse)
        }),
        // Work items
        ("wi list", || schema_for!(Vec<workitem::types::WorkItem>)),
        ("wi query", || schema_for!(Vec<workitem::types::WorkItem>)),
        ("wi view", || schema_for!(workitem::types::WorkItem)),
        ("wi create", || schema_for!(workitem::types::WorkItem)),
        ("wi update", || schema_for!(Vec<workitem::types::WorkItem>)),
        ("wi comment", || schema_for!(workitem::types::WiComment)),
        ("wi comments", || {
            schema_for!(workitem::types::WiCommentList)
        }),
        ("wi comment-edit", || {
            schema_for!(workitem::types::WiComment)
        }),
        ("wi link", || schema_for!(workitem::types::WorkItem)),
        ("wi links", || schema_for!(Vec<workitem::types::Relation>)),
        ("wi link-rm", || schema_for!(workitem::types::WorkItem)),
        ("wi attach", || schema_for!(workitem::types::AttachResult)),
        ("wi attachments", || {
            schema_for!(workitem::types::WiAttachmentList)
        }),
        ("wi attachment-download", || {
            schema_for!(workitem::types::WiAttachmentDownloadResult)
        }),
        ("wi history", || {
            schema_for!(workitem::types::WiHistoryResponse)
        }),
        ("wi types", || {
            schema_for!(workitem::types::WorkItemTypeListResponse)
        }),
        ("wi states", || {
            schema_for!(workitem::types::StateListResponse)
        }),
        ("wi fields", || {
            schema_for!(workitem::types::FieldListResponse)
        }),
    ]
}

pub async fn run(args: SchemaArgs, output_fmt: OutputFormat) -> Result<()> {
    let reg = registry();

    if args.list {
        match output_fmt {
            OutputFormat::Json => {
                let paths: Vec<&str> = reg.iter().map(|(k, _)| *k).collect();
                output::print_json(&paths)?;
            }
            OutputFormat::Text | OutputFormat::Table => {
                for (path, _) in &reg {
                    println!("{path}");
                }
            }
        }
        return Ok(());
    }

    if args.command.is_empty() {
        return Err(CliError::Validation(
            "expected a command path or --list (try `ado schema --list`)".into(),
        )
        .into());
    }
    let path = args.command.join(" ");

    let entry = reg.iter().find(|(k, _)| *k == path).ok_or_else(|| {
        CliError::NotFound(format!(
            "no schema registered for `{path}` — try `ado schema --list`"
        ))
    })?;
    let schema = (entry.1)();
    output::print_json(&schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_no_duplicate_paths() {
        let reg = registry();
        let mut seen: Vec<&str> = Vec::new();
        for (path, _) in &reg {
            assert!(
                !seen.contains(path),
                "duplicate registry entry for `{path}`"
            );
            seen.push(*path);
        }
    }

    #[test]
    fn registry_covers_foundation_commands() {
        let reg = registry();
        let paths: Vec<&str> = reg.iter().map(|(k, _)| *k).collect();
        for expected in [
            "me",
            "team list",
            "iteration list",
            "iteration current",
            "area tree",
            "wi list",
            "wi view",
            "wi types",
            "wi states",
            "wi fields",
            "pr list",
            "pr view",
            "pipeline list",
            "repo list",
            "sprint backlog",
            "sprint summary",
        ] {
            assert!(paths.contains(&expected), "registry missing `{expected}`");
        }
    }

    #[test]
    fn registry_entries_generate_valid_schemas() {
        for (path, gen_schema) in registry() {
            let schema = gen_schema();
            let serialized = serde_json::to_string(&schema)
                .unwrap_or_else(|e| panic!("failed to serialize schema for `{path}`: {e}"));
            assert!(!serialized.is_empty(), "empty schema for `{path}`");
        }
    }
}
