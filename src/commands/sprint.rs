//! `ado sprint` - planning helpers built on team iterations and work items.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use clap::{Args, Subcommand, ValueEnum};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::client::{AdoClient, encode_path_segment};
use crate::commands::iteration::{self, TeamIteration};
use crate::commands::team::require_team;
use crate::commands::workitem::api as wi_api;
use crate::commands::workitem::helpers::{escape_wiql, field_str, resolve_user};
use crate::commands::workitem::types::{PatchOp, WiHistoryResponse, WorkItem};
use crate::context::CmdCtx;
use crate::error::CliError;
use crate::output::{self, OutputFormat};

const SPRINT_FIELDS: &[&str] = &[
    "System.Id",
    "System.Title",
    "System.State",
    "System.WorkItemType",
    "System.AssignedTo",
    "System.Tags",
    "System.AreaPath",
    "System.IterationPath",
    "Microsoft.VSTS.Common.StackRank",
    "Microsoft.VSTS.Scheduling.StoryPoints",
    "Microsoft.VSTS.Scheduling.Effort",
    "Microsoft.VSTS.Scheduling.OriginalEstimate",
    "Microsoft.VSTS.Scheduling.RemainingWork",
    "Microsoft.VSTS.Scheduling.CompletedWork",
];

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado sprint backlog\n  ado sprint backlog --iteration @next --type \"User Story\" --top 20\n  ado sprint board --iteration @current --output table\n  ado sprint plan-into 123 124 --iteration @next --assigned-to me\n  ado sprint capacity\n  ado sprint capacity set --member me --hours-per-day 6 --activity Development\n  ado sprint burndown --by member\n  ado sprint rollover --dry-run\n  ado sprint summary --iteration @current\n\nRequires a team - pass --team, set ADO_TEAM, or run `ado team set <name>`."
)]
pub struct SprintArgs {
    #[command(subcommand)]
    pub command: SprintCommand,
}

#[derive(Subcommand)]
pub enum SprintCommand {
    /// List work items planned for an iteration
    Backlog(BacklogArgs),

    /// Render the iteration taskboard grouped by taskboard column
    Board(BoardArgs),

    /// Move one or more work items into an iteration
    PlanInto(PlanIntoArgs),

    /// Show or update iteration capacity
    Capacity(CapacityArgs),

    /// Reconstruct remaining-work burndown from work item history
    Burndown(BurndownArgs),

    /// Move unfinished work from one iteration to another
    Rollover(RolloverArgs),

    /// Summarize planned, completed, carryover, and added work
    Summary(SummaryArgs),
}

#[derive(Args)]
pub struct BacklogArgs {
    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@next")]
    pub iteration: String,

    /// Filter by work item type
    #[arg(long, value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Filter by state
    #[arg(long, value_name = "STATE")]
    pub state: Option<String>,

    /// Filter by tag
    #[arg(long, value_name = "TAG")]
    pub tag: Option<String>,

    /// Filter by area path
    #[arg(long, value_name = "PATH")]
    pub area: Option<String>,

    /// Only show unassigned work
    #[arg(long)]
    pub unassigned: bool,

    /// Limit the number of returned items
    #[arg(long, value_name = "N")]
    pub top: Option<u32>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct BoardArgs {
    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub iteration: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct PlanIntoArgs {
    /// Work item ID(s), or omit and pipe ids on stdin
    #[arg(value_name = "WI_ID", num_args = 0..)]
    pub ids: Vec<u32>,

    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@next")]
    pub iteration: String,

    /// Assign to user while planning (use "me" for yourself)
    #[arg(long, value_name = "USER")]
    pub assigned_to: Option<String>,

    /// Set state while planning
    #[arg(long, value_name = "STATE")]
    pub state: Option<String>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CapacityArgs {
    #[command(subcommand)]
    pub command: Option<CapacityCommand>,

    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub iteration: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Subcommand)]
pub enum CapacityCommand {
    /// Set one team member's activity capacity for an iteration
    Set(CapacitySetArgs),
}

#[derive(Args)]
pub struct CapacitySetArgs {
    /// Team member id, unique name, display name, or "me"
    #[arg(long, value_name = "MEMBER")]
    pub member: String,

    /// Hours per day for this activity
    #[arg(long, value_name = "HOURS")]
    pub hours_per_day: f64,

    /// Activity bucket name (for example, Development)
    #[arg(long, value_name = "ACTIVITY")]
    pub activity: String,

    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub iteration: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct BurndownArgs {
    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub iteration: String,

    /// Break out the burndown by dimension
    #[arg(long, value_enum, value_name = "DIMENSION")]
    pub by: Option<BurndownBy>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum BurndownBy {
    Member,
}

#[derive(Args)]
pub struct RolloverArgs {
    /// Source iteration id or shorthand
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub from: String,

    /// Target iteration id or shorthand
    #[arg(long, value_name = "REF", default_value = "@next")]
    pub to: String,

    /// Preview what would move without mutating work items
    #[arg(long)]
    pub dry_run: bool,

    /// Comma-separated states to move
    #[arg(long, value_name = "STATES", default_value = "Active,New")]
    pub state_filter: String,

    /// Reset CompletedWork to 0 and RemainingWork to OriginalEstimate when available
    #[arg(long)]
    pub reset_remaining: bool,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct SummaryArgs {
    /// Iteration id or shorthand (`@current`, `@next`, `@previous`)
    #[arg(long, value_name = "REF", default_value = "@current")]
    pub iteration: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SprintIterationRef {
    pub id: String,
    pub name: String,
    pub path: String,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub time_frame: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SprintIdentity {
    pub display_name: String,
    pub unique_name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SprintWorkItem {
    pub id: u32,
    pub work_item_type: String,
    pub title: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<SprintIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_work: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_work: Option<f64>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_path: Option<String>,
    pub url: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintBacklogResponse {
    pub iteration: SprintIterationRef,
    pub count: u32,
    pub value: Vec<SprintWorkItem>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintBoardResponse {
    pub iteration: SprintIterationRef,
    pub columns: Vec<SprintBoardColumn>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintBoardColumn {
    pub id: String,
    pub name: String,
    pub order: i32,
    pub count: u32,
    pub items: Vec<SprintWorkItem>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintPlanIntoResponse {
    pub iteration: SprintIterationRef,
    pub count: u32,
    pub updated: Vec<SprintWorkItem>,
    pub failures: Vec<SprintMutationFailure>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintMutationFailure {
    pub id: u32,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeamCapacity {
    #[serde(default, rename = "teamMembers")]
    pub team_members: Vec<TeamMemberCapacity>,
    #[serde(default, rename = "totalCapacityPerDay")]
    pub total_capacity_per_day: f64,
    #[serde(default, rename = "totalDaysOff")]
    pub total_days_off: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeamMemberCapacity {
    #[serde(rename = "teamMember")]
    pub team_member: CapacityIdentity,
    #[serde(default)]
    pub activities: Vec<CapacityActivity>,
    #[serde(default, rename = "daysOff")]
    pub days_off: Vec<CapacityDateRange>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacityIdentity {
    #[serde(default, rename = "displayName")]
    pub display_name: String,
    #[serde(default, rename = "uniqueName")]
    pub unique_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub descriptor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacityActivity {
    pub name: String,
    #[serde(rename = "capacityPerDay")]
    pub capacity_per_day: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacityDateRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintCapacityResponse {
    pub iteration: SprintIterationRef,
    pub capacity: TeamCapacity,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintCapacitySetResponse {
    pub iteration: SprintIterationRef,
    pub member: TeamMemberCapacity,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintBurndownResponse {
    pub iteration: SprintIterationRef,
    pub by: Option<String>,
    pub points: Vec<SprintBurndownPoint>,
    pub members: Vec<MemberBurndown>,
}

#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct SprintBurndownPoint {
    pub date: String,
    pub remaining_hours: f64,
    pub completed_hours: f64,
    pub scope_hours: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemberBurndown {
    pub member: String,
    pub points: Vec<SprintBurndownPoint>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintRolloverResponse {
    pub from: SprintIterationRef,
    pub to: SprintIterationRef,
    pub dry_run: bool,
    pub count: u32,
    pub moved: Vec<SprintWorkItem>,
    pub failures: Vec<SprintMutationFailure>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SprintSummaryResponse {
    pub iteration: SprintIterationRef,
    pub planned_count: u32,
    pub planned_points: f64,
    pub planned_hours: f64,
    pub completed_count: u32,
    pub completed_points: f64,
    pub completed_hours: f64,
    pub carryover_count: u32,
    pub additions_mid_sprint_count: u32,
    pub additions_mid_sprint: Vec<u32>,
    pub per_member: Vec<MemberSummary>,
}

#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct MemberSummary {
    pub member: String,
    pub total_count: u32,
    pub completed_count: u32,
    pub carryover_count: u32,
    pub points: f64,
    pub hours: f64,
}

#[derive(Debug, Deserialize)]
struct TaskboardColumnsResponse {
    #[serde(default)]
    columns: Vec<TaskboardColumnRaw>,
}

#[derive(Debug, Deserialize)]
struct TaskboardColumnRaw {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    order: i32,
}

#[derive(Debug, Deserialize)]
struct TaskboardWorkItemColumnRaw {
    #[serde(default)]
    column: String,
    #[serde(default, rename = "columnId")]
    column_id: String,
    #[serde(default, rename = "workItemId")]
    work_item_id: u32,
}

pub async fn run(args: SprintArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        SprintCommand::Backlog(a) => backlog(a, ctx).await,
        SprintCommand::Board(a) => board(a, ctx).await,
        SprintCommand::PlanInto(a) => plan_into(a, ctx).await,
        SprintCommand::Capacity(a) => capacity(a, ctx).await,
        SprintCommand::Burndown(a) => burndown(a, ctx).await,
        SprintCommand::Rollover(a) => rollover(a, ctx).await,
        SprintCommand::Summary(a) => summary(a, ctx).await,
    }
}

async fn backlog(args: BacklogArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    validate_top(args.top)?;
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let wiql = build_backlog_wiql(&args, &iteration.path);
    let refs = wi_api::run_wiql(ctx.client, project, &wiql, args.top).await?;
    let items = hydrate_sprint_items(ctx.client, &refs.work_items).await?;
    let response = SprintBacklogResponse {
        iteration: sprint_iteration(&iteration),
        count: items.len() as u32,
        value: items,
    };
    render_backlog(&response, ctx.output)
}

async fn board(args: BoardArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;

    let columns_path = format!(
        "{project}/{team}/_apis/work/taskboardcolumns?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team)
    );
    let mut columns_resp: TaskboardColumnsResponse = ctx
        .client
        .get_json(&columns_path)
        .await
        .map_err(normalize_taskboard_setup_error)?;
    columns_resp.columns.sort_by_key(|c| c.order);

    let items_path = format!(
        "{project}/{team}/_apis/work/taskboardworkitems/{iteration_id}?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team),
        iteration_id = encode_path_segment(&iteration.id)
    );
    let taskboard_items: Vec<TaskboardWorkItemColumnRaw> =
        ctx.client
            .get_json(&items_path)
            .await
            .map_err(normalize_taskboard_setup_error)?;
    let ids: Vec<u32> = taskboard_items
        .iter()
        .filter_map(|i| (i.work_item_id != 0).then_some(i.work_item_id))
        .collect();
    let hydrated = hydrate_sprint_items_by_ids(ctx.client, &ids).await?;
    let by_id: HashMap<u32, SprintWorkItem> = hydrated.into_iter().map(|i| (i.id, i)).collect();

    let mut response_columns: Vec<SprintBoardColumn> = columns_resp
        .columns
        .iter()
        .map(|c| SprintBoardColumn {
            id: c.id.clone(),
            name: c.name.clone(),
            order: c.order,
            count: 0,
            items: Vec::new(),
        })
        .collect();
    let mut col_index: HashMap<String, usize> = response_columns
        .iter()
        .enumerate()
        .map(|(idx, c)| (c.id.clone(), idx))
        .collect();

    for task_col in &taskboard_items {
        let Some(item) = by_id.get(&task_col.work_item_id).cloned() else {
            continue;
        };
        let idx = match col_index.get(&task_col.column_id).copied() {
            Some(idx) => idx,
            None => {
                let idx = response_columns.len();
                col_index.insert(task_col.column_id.clone(), idx);
                response_columns.push(SprintBoardColumn {
                    id: task_col.column_id.clone(),
                    name: if task_col.column.is_empty() {
                        "(unknown)".into()
                    } else {
                        task_col.column.clone()
                    },
                    order: idx as i32,
                    count: 0,
                    items: Vec::new(),
                });
                idx
            }
        };
        response_columns[idx].items.push(item);
    }
    for col in &mut response_columns {
        col.count = col.items.len() as u32;
    }

    let response = SprintBoardResponse {
        iteration: sprint_iteration(&iteration),
        columns: response_columns,
    };
    render_board(&response, ctx.output)
}

async fn plan_into(args: PlanIntoArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let ids = crate::stdin_ids::read_ids(&args.ids)?;

    let mut ops = vec![PatchOp {
        op: "add".into(),
        path: "/fields/System.IterationPath".into(),
        value: json!(iteration.path),
    }];
    if let Some(state) = args.state.as_deref() {
        ops.push(PatchOp {
            op: "add".into(),
            path: "/fields/System.State".into(),
            value: json!(state),
        });
    }
    if let Some(who) = args.assigned_to.as_deref() {
        let value = if who.is_empty() {
            json!("")
        } else {
            json!(resolve_user(ctx.client, who).await?)
        };
        ops.push(PatchOp {
            op: "add".into(),
            path: "/fields/System.AssignedTo".into(),
            value,
        });
    }

    let (updated, failures) = patch_many(ctx.client, &ids, &ops).await;
    let response = SprintPlanIntoResponse {
        iteration: sprint_iteration(&iteration),
        count: updated.len() as u32,
        updated: updated
            .into_iter()
            .map(|wi| sprint_work_item(&wi))
            .collect(),
        failures: failures_to_schema(failures),
    };
    render_plan_into(&response, ids.len(), ctx)?;
    Ok(())
}

async fn capacity(args: CapacityArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        Some(CapacityCommand::Set(mut set_args)) => {
            if set_args.iteration == "@current" && args.iteration != "@current" {
                set_args.iteration = args.iteration;
            }
            if set_args.project.is_none() {
                set_args.project = args.project;
            }
            capacity_set(set_args, ctx).await
        }
        None => capacity_show(args, ctx).await,
    }
}

async fn capacity_show(args: CapacityArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let capacity = fetch_capacity(ctx.client, project, team, &iteration.id).await?;
    let response = SprintCapacityResponse {
        iteration: sprint_iteration(&iteration),
        capacity,
    };
    render_capacity(&response, ctx.output)
}

async fn capacity_set(args: CapacitySetArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    if args.hours_per_day < 0.0 {
        return Err(CliError::Validation("--hours-per-day cannot be negative".into()).into());
    }
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let capacity = fetch_capacity(ctx.client, project, team, &iteration.id).await?;
    let member = find_capacity_member(ctx.client, &capacity, &args.member).await?;

    let mut activities: Vec<CapacityActivity> = member
        .activities
        .iter()
        .filter(|a| !a.name.eq_ignore_ascii_case(&args.activity))
        .cloned()
        .collect();
    activities.push(CapacityActivity {
        name: args.activity.clone(),
        capacity_per_day: args.hours_per_day,
    });
    activities.sort_by(|a, b| a.name.cmp(&b.name));

    let body = json!({
        "activities": activities,
        "daysOff": member.days_off,
    });
    let path = format!(
        "{project}/{team}/_apis/work/teamsettings/iterations/{iteration_id}/capacities/{member_id}?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team),
        iteration_id = encode_path_segment(&iteration.id),
        member_id = encode_path_segment(&member.team_member.id),
    );
    let updated: TeamMemberCapacity = ctx.client.patch_json(&path, &body).await?;
    let response = SprintCapacitySetResponse {
        iteration: sprint_iteration(&iteration),
        member: updated,
    };
    render_capacity_set(&response, ctx.output)
}

async fn burndown(args: BurndownArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let items = fetch_iteration_history_items(ctx.client, project, &iteration.path).await?;
    let dates = iteration_dates(&iteration);
    let histories = fetch_histories(ctx.client, project, &items).await?;

    let points = build_burndown_points(&items, &histories, &iteration.path, &dates, None);
    let members = if matches!(args.by, Some(BurndownBy::Member)) {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for item in &items {
            names.insert(member_name(item));
        }
        names
            .into_iter()
            .map(|member| MemberBurndown {
                points: build_burndown_points(
                    &items,
                    &histories,
                    &iteration.path,
                    &dates,
                    Some(&member),
                ),
                member,
            })
            .collect()
    } else {
        Vec::new()
    };

    let response = SprintBurndownResponse {
        iteration: sprint_iteration(&iteration),
        by: args.by.map(|_| "member".to_string()),
        points,
        members,
    };
    render_burndown(&response, ctx.output)
}

async fn rollover(args: RolloverArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let from = iteration::parse_iteration_ref(ctx.client, project, team, &args.from).await?;
    let to = iteration::parse_iteration_ref(ctx.client, project, team, &args.to).await?;
    let states = parse_state_filter(&args.state_filter)?;
    let wiql = build_rollover_wiql(&from.path, &states);
    let refs = wi_api::run_wiql(ctx.client, project, &wiql, None).await?;
    let candidates = hydrate_sprint_items(ctx.client, &refs.work_items).await?;

    if args.dry_run {
        let response = SprintRolloverResponse {
            from: sprint_iteration(&from),
            to: sprint_iteration(&to),
            dry_run: true,
            count: candidates.len() as u32,
            moved: candidates,
            failures: Vec::new(),
        };
        return render_rollover(&response, ctx.output);
    }

    let batch_id = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let mut moved: Vec<SprintWorkItem> = Vec::new();
    let mut failures: Vec<SprintMutationFailure> = Vec::new();
    for item in &candidates {
        let mut ops = vec![PatchOp {
            op: "add".into(),
            path: "/fields/System.IterationPath".into(),
            value: json!(to.path),
        }];
        if args.reset_remaining {
            ops.push(PatchOp {
                op: "add".into(),
                path: "/fields/Microsoft.VSTS.Scheduling.CompletedWork".into(),
                value: json!(0),
            });
            if let Some(original) = item.original_estimate {
                ops.push(PatchOp {
                    op: "add".into(),
                    path: "/fields/Microsoft.VSTS.Scheduling.RemainingWork".into(),
                    value: json!(original),
                });
            }
        }

        match wi_api::patch_work_item(ctx.client, item.id, &ops).await {
            Ok(wi) => {
                let comment = rollover_comment(&from, &to, &batch_id, candidates.len());
                if let Err(e) = wi_api::add_comment(ctx.client, project, item.id, &comment).await {
                    failures.push(SprintMutationFailure {
                        id: item.id,
                        error: format!("moved but comment failed: {e:#}"),
                    });
                }
                moved.push(sprint_work_item(&wi));
            }
            Err(e) => failures.push(SprintMutationFailure {
                id: item.id,
                error: format!("{e:#}"),
            }),
        }
    }

    let response = SprintRolloverResponse {
        from: sprint_iteration(&from),
        to: sprint_iteration(&to),
        dry_run: false,
        count: moved.len() as u32,
        moved,
        failures,
    };
    render_rollover(&response, ctx.output)?;
    if !response.failures.is_empty() {
        if ctx.client.explain_enabled() {
            return Err(CliError::Explain.into());
        }
        bail!(
            "{}/{} rollover updates failed",
            response.failures.len(),
            candidates.len()
        );
    }
    Ok(())
}

async fn summary(args: SummaryArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iteration =
        iteration::parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    let items = fetch_iteration_history_items(ctx.client, project, &iteration.path).await?;
    let histories = fetch_histories(ctx.client, project, &items).await?;
    let state_categories = fetch_state_categories(ctx.client, project, &items).await;
    let (start, end) = iteration_date_bounds(&iteration);

    let mut planned_count = 0_u32;
    let mut planned_points = 0.0;
    let mut planned_hours = 0.0;
    let mut completed_count = 0_u32;
    let mut completed_points = 0.0;
    let mut completed_hours = 0.0;
    let mut carryover_count = 0_u32;
    let mut additions: Vec<u32> = Vec::new();
    let mut per_member: BTreeMap<String, MemberSummary> = BTreeMap::new();

    for item in &items {
        let history = histories.get(&item.id);
        let start_snap = snapshot_for_date(item, history, start);
        let end_snap = snapshot_for_date(item, history, end);
        let in_start = in_iteration(&start_snap, item, &iteration.path);
        let in_end = in_iteration(&end_snap, item, &iteration.path);
        let added_mid_sprint = !in_start && in_end;
        let completed = is_completed_state(
            item,
            end_snap.state.as_deref(),
            state_categories.as_ref().ok(),
        );
        let points = estimate_points(item);
        let hours = estimate_hours(item);

        if in_start {
            planned_count += 1;
            planned_points += points;
            planned_hours += hours;
        }
        if in_end && completed {
            completed_count += 1;
            completed_points += points;
            completed_hours += hours;
        }
        if in_end && !completed {
            carryover_count += 1;
        }
        if added_mid_sprint {
            additions.push(item.id);
        }
        if in_end {
            let member = member_name(item);
            let entry = per_member.entry(member.clone()).or_insert(MemberSummary {
                member,
                ..MemberSummary::default()
            });
            entry.total_count += 1;
            entry.points += points;
            entry.hours += hours;
            if completed {
                entry.completed_count += 1;
            } else {
                entry.carryover_count += 1;
            }
        }
    }

    let response = SprintSummaryResponse {
        iteration: sprint_iteration(&iteration),
        planned_count,
        planned_points,
        planned_hours,
        completed_count,
        completed_points,
        completed_hours,
        carryover_count,
        additions_mid_sprint_count: additions.len() as u32,
        additions_mid_sprint: additions,
        per_member: per_member.into_values().collect(),
    };
    render_summary(&response, ctx.output)
}

fn build_backlog_wiql(args: &BacklogArgs, iteration_path: &str) -> String {
    let mut clauses = vec![
        "[System.TeamProject] = @project".to_string(),
        "[System.State] <> 'Removed'".to_string(),
        format!(
            "[System.IterationPath] UNDER '{}'",
            escape_wiql(iteration_path)
        ),
    ];
    if let Some(t) = args.r#type.as_deref() {
        clauses.push(format!("[System.WorkItemType] = '{}'", escape_wiql(t)));
    }
    if let Some(s) = args.state.as_deref() {
        clauses.push(format!("[System.State] = '{}'", escape_wiql(s)));
    }
    if let Some(tag) = args.tag.as_deref() {
        clauses.push(format!("[System.Tags] CONTAINS '{}'", escape_wiql(tag)));
    }
    if let Some(area) = args.area.as_deref() {
        clauses.push(format!("[System.AreaPath] UNDER '{}'", escape_wiql(area)));
    }
    if args.unassigned {
        clauses.push("[System.AssignedTo] = ''".to_string());
    }
    format!(
        "SELECT [System.Id] FROM WorkItems WHERE {} ORDER BY [Microsoft.VSTS.Common.StackRank] ASC, [System.ChangedDate] DESC",
        clauses.join(" AND ")
    )
}

fn normalize_taskboard_setup_error(e: anyhow::Error) -> anyhow::Error {
    if format!("{e:#}").contains("Taskboard columns are not added") {
        CliError::NotFound("taskboard columns are not configured for this team".into()).into()
    } else {
        e
    }
}

fn build_rollover_wiql(iteration_path: &str, states: &[String]) -> String {
    let states = states
        .iter()
        .map(|s| format!("'{}'", escape_wiql(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.State] IN ({states}) AND [System.IterationPath] UNDER '{}' ORDER BY [System.ChangedDate] DESC",
        escape_wiql(iteration_path)
    )
}

async fn hydrate_sprint_items(
    client: &AdoClient,
    refs: &[crate::commands::workitem::types::WiqlWorkItemRef],
) -> Result<Vec<SprintWorkItem>> {
    let order: HashMap<u32, usize> = refs.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let mut items = wi_api::fetch_work_items(client, refs, SPRINT_FIELDS).await?;
    items.sort_by_key(|w| order.get(&w.id).copied().unwrap_or(usize::MAX));
    Ok(items.iter().map(sprint_work_item).collect())
}

async fn hydrate_sprint_items_by_ids(
    client: &AdoClient,
    ids: &[u32],
) -> Result<Vec<SprintWorkItem>> {
    let order: HashMap<u32, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut items = wi_api::fetch_work_items_by_ids(client, ids, SPRINT_FIELDS, None).await?;
    items.sort_by_key(|w| order.get(&w.id).copied().unwrap_or(usize::MAX));
    Ok(items.iter().map(sprint_work_item).collect())
}

async fn patch_many(
    client: &AdoClient,
    ids: &[u32],
    ops: &[PatchOp],
) -> (Vec<WorkItem>, Vec<(u32, anyhow::Error)>) {
    let mut updated = Vec::with_capacity(ids.len());
    let mut failures = Vec::new();
    for id in ids {
        match wi_api::patch_work_item(client, *id, ops).await {
            Ok(wi) => updated.push(wi),
            Err(e) => failures.push((*id, e)),
        }
    }
    (updated, failures)
}

async fn fetch_capacity(
    client: &AdoClient,
    project: &str,
    team: &str,
    iteration_id: &str,
) -> Result<TeamCapacity> {
    let path = format!(
        "{project}/{team}/_apis/work/teamsettings/iterations/{iteration_id}/capacities?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team),
        iteration_id = encode_path_segment(iteration_id)
    );
    client.get_json(&path).await
}

async fn find_capacity_member<'a>(
    client: &AdoClient,
    capacity: &'a TeamCapacity,
    input: &str,
) -> Result<&'a TeamMemberCapacity> {
    let mut candidates = vec![input.to_string()];
    if input.eq_ignore_ascii_case("me") {
        if let Ok(resolved) = resolve_user(client, input).await {
            candidates.push(resolved);
        }
    }
    capacity
        .team_members
        .iter()
        .find(|m| {
            candidates.iter().any(|candidate| {
                candidate.eq_ignore_ascii_case(&m.team_member.id)
                    || candidate.eq_ignore_ascii_case(&m.team_member.unique_name)
                    || candidate.eq_ignore_ascii_case(&m.team_member.display_name)
            })
        })
        .ok_or_else(|| {
            CliError::NotFound(format!(
                "team member `{input}` was not found in this iteration's capacity"
            ))
            .into()
        })
}

async fn fetch_iteration_history_items(
    client: &AdoClient,
    project: &str,
    iteration_path: &str,
) -> Result<Vec<WorkItem>> {
    let wiql = format!(
        "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND EVER [System.IterationPath] = '{}' ORDER BY [System.Id]",
        escape_wiql(iteration_path)
    );
    let refs = wi_api::run_wiql(client, project, &wiql, None).await?;
    wi_api::fetch_work_items(client, &refs.work_items, SPRINT_FIELDS).await
}

async fn fetch_histories(
    client: &AdoClient,
    project: &str,
    items: &[WorkItem],
) -> Result<HashMap<u32, WiHistoryResponse>> {
    let mut out = HashMap::with_capacity(items.len());
    for item in items {
        out.insert(
            item.id,
            wi_api::list_updates(client, project, item.id, None).await?,
        );
    }
    Ok(out)
}

async fn fetch_state_categories(
    client: &AdoClient,
    project: &str,
    items: &[WorkItem],
) -> Result<HashMap<(String, String), String>> {
    #[derive(Deserialize)]
    struct StateList {
        value: Vec<StateInfo>,
    }
    #[derive(Deserialize)]
    struct StateInfo {
        name: String,
        #[serde(default)]
        category: String,
    }

    let mut types = BTreeSet::new();
    for item in items {
        if let Some(ty) = field_str(&item.fields, "System.WorkItemType") {
            types.insert(ty.to_string());
        }
    }
    let mut out = HashMap::new();
    for ty in types {
        let path = format!(
            "{project}/_apis/wit/workitemtypes/{type}/states?api-version=7.1",
            project = encode_path_segment(project),
            r#type = encode_path_segment(&ty)
        );
        let states: StateList = client.get_json(&path).await?;
        for state in states.value {
            out.insert((ty.clone(), state.name), state.category);
        }
    }
    Ok(out)
}

fn build_burndown_points(
    items: &[WorkItem],
    histories: &HashMap<u32, WiHistoryResponse>,
    iteration_path: &str,
    dates: &[NaiveDate],
    member_filter: Option<&str>,
) -> Vec<SprintBurndownPoint> {
    let mut points: Vec<SprintBurndownPoint> = dates
        .iter()
        .map(|date| SprintBurndownPoint {
            date: date.to_string(),
            ..SprintBurndownPoint::default()
        })
        .collect();

    for item in items {
        if let Some(member) = member_filter {
            if member_name(item) != member {
                continue;
            }
        }
        let history = histories.get(&item.id);
        for (idx, date) in dates.iter().enumerate() {
            let snapshot = snapshot_for_date(item, history, *date);
            if !in_iteration(&snapshot, item, iteration_path) {
                continue;
            }
            let remaining = snapshot
                .remaining
                .or_else(|| number_field(&item.fields, "Microsoft.VSTS.Scheduling.RemainingWork"))
                .unwrap_or(0.0);
            let completed = snapshot
                .completed
                .or_else(|| number_field(&item.fields, "Microsoft.VSTS.Scheduling.CompletedWork"))
                .unwrap_or(0.0);
            let scope = snapshot
                .original
                .or_else(|| {
                    number_field(&item.fields, "Microsoft.VSTS.Scheduling.OriginalEstimate")
                })
                .unwrap_or(remaining + completed);
            points[idx].remaining_hours += remaining;
            points[idx].completed_hours += completed;
            points[idx].scope_hours += scope;
        }
    }
    points
}

#[derive(Debug, Clone, Default)]
struct ItemSnapshot {
    iteration_path: Option<String>,
    state: Option<String>,
    assigned_to: Option<String>,
    remaining: Option<f64>,
    completed: Option<f64>,
    original: Option<f64>,
}

fn snapshot_for_date(
    item: &WorkItem,
    history: Option<&WiHistoryResponse>,
    date: NaiveDate,
) -> ItemSnapshot {
    let Some(history) = history else {
        return ItemSnapshot::default();
    };
    let cutoff = date
        .and_time(NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap())
        .and_utc();
    let mut snapshot = ItemSnapshot::default();
    let mut revisions = history.value.iter().collect::<Vec<_>>();
    revisions.sort_by_key(|u| u.revised_date.as_deref().and_then(parse_ado_datetime));
    for update in &revisions {
        seed_snapshot_old_fields(&mut snapshot, &update.fields);
    }
    for update in revisions {
        let Some(revised) = update.revised_date.as_deref().and_then(parse_ado_datetime) else {
            continue;
        };
        if revised > cutoff {
            break;
        }
        apply_snapshot_fields(&mut snapshot, &update.fields);
    }
    if snapshot.iteration_path.is_none() {
        snapshot.iteration_path = string_field_owned(&item.fields, "System.IterationPath");
    }
    if snapshot.state.is_none() {
        snapshot.state = string_field_owned(&item.fields, "System.State");
    }
    if snapshot.assigned_to.is_none() {
        snapshot.assigned_to = assigned_name_from_fields(&item.fields);
    }
    if snapshot.remaining.is_none() {
        snapshot.remaining = number_field(&item.fields, "Microsoft.VSTS.Scheduling.RemainingWork");
    }
    if snapshot.completed.is_none() {
        snapshot.completed = number_field(&item.fields, "Microsoft.VSTS.Scheduling.CompletedWork");
    }
    if snapshot.original.is_none() {
        snapshot.original =
            number_field(&item.fields, "Microsoft.VSTS.Scheduling.OriginalEstimate");
    }
    snapshot
}

fn seed_snapshot_old_fields(snapshot: &mut ItemSnapshot, fields: &Value) {
    let Some(obj) = fields.as_object() else {
        return;
    };
    for (name, change) in obj {
        let Some(old_value) = change.get("oldValue") else {
            continue;
        };
        match name.as_str() {
            "System.IterationPath" if snapshot.iteration_path.is_none() => {
                snapshot.iteration_path = value_string(old_value)
            }
            "System.State" if snapshot.state.is_none() => snapshot.state = value_string(old_value),
            "System.AssignedTo" if snapshot.assigned_to.is_none() => {
                snapshot.assigned_to = value_identity_name(old_value)
            }
            "Microsoft.VSTS.Scheduling.RemainingWork" if snapshot.remaining.is_none() => {
                snapshot.remaining = value_f64(old_value)
            }
            "Microsoft.VSTS.Scheduling.CompletedWork" if snapshot.completed.is_none() => {
                snapshot.completed = value_f64(old_value)
            }
            "Microsoft.VSTS.Scheduling.OriginalEstimate" if snapshot.original.is_none() => {
                snapshot.original = value_f64(old_value)
            }
            _ => {}
        }
    }
}

fn apply_snapshot_fields(snapshot: &mut ItemSnapshot, fields: &Value) {
    let Some(obj) = fields.as_object() else {
        return;
    };
    for (name, change) in obj {
        let new_value = change.get("newValue").unwrap_or(change);
        match name.as_str() {
            "System.IterationPath" => snapshot.iteration_path = value_string(new_value),
            "System.State" => snapshot.state = value_string(new_value),
            "System.AssignedTo" => snapshot.assigned_to = value_identity_name(new_value),
            "Microsoft.VSTS.Scheduling.RemainingWork" => snapshot.remaining = value_f64(new_value),
            "Microsoft.VSTS.Scheduling.CompletedWork" => snapshot.completed = value_f64(new_value),
            "Microsoft.VSTS.Scheduling.OriginalEstimate" => {
                snapshot.original = value_f64(new_value)
            }
            _ => {}
        }
    }
}

fn sprint_work_item(wi: &WorkItem) -> SprintWorkItem {
    SprintWorkItem {
        id: wi.id,
        work_item_type: field_str(&wi.fields, "System.WorkItemType")
            .unwrap_or("")
            .to_string(),
        title: field_str(&wi.fields, "System.Title")
            .unwrap_or("")
            .to_string(),
        state: field_str(&wi.fields, "System.State")
            .unwrap_or("")
            .to_string(),
        assigned_to: identity_field(&wi.fields, "System.AssignedTo"),
        story_points: number_field(&wi.fields, "Microsoft.VSTS.Scheduling.StoryPoints"),
        effort: number_field(&wi.fields, "Microsoft.VSTS.Scheduling.Effort"),
        original_estimate: number_field(&wi.fields, "Microsoft.VSTS.Scheduling.OriginalEstimate"),
        remaining_work: number_field(&wi.fields, "Microsoft.VSTS.Scheduling.RemainingWork"),
        completed_work: number_field(&wi.fields, "Microsoft.VSTS.Scheduling.CompletedWork"),
        tags: tags_field(&wi.fields),
        area_path: string_field_owned(&wi.fields, "System.AreaPath"),
        iteration_path: string_field_owned(&wi.fields, "System.IterationPath"),
        url: wi.url.clone(),
    }
}

fn sprint_iteration(it: &TeamIteration) -> SprintIterationRef {
    SprintIterationRef {
        id: it.id.clone(),
        name: it.name.clone(),
        path: it.path.clone(),
        start_date: it.attributes.start_date.clone(),
        finish_date: it.attributes.finish_date.clone(),
        time_frame: it.attributes.time_frame.clone(),
    }
}

fn identity_field(fields: &Value, key: &str) -> Option<SprintIdentity> {
    let value = fields.get(key)?;
    if let Some(s) = value.as_str() {
        if s.is_empty() {
            return None;
        }
        return Some(SprintIdentity {
            display_name: s.to_string(),
            unique_name: String::new(),
            id: String::new(),
        });
    }
    Some(SprintIdentity {
        display_name: value
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        unique_name: value
            .get("uniqueName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
    .filter(|i| !i.display_name.is_empty() || !i.unique_name.is_empty() || !i.id.is_empty())
}

fn assigned_name_from_fields(fields: &Value) -> Option<String> {
    identity_field(fields, "System.AssignedTo").map(|i| {
        if !i.display_name.is_empty() {
            i.display_name
        } else if !i.unique_name.is_empty() {
            i.unique_name
        } else {
            i.id
        }
    })
}

fn member_name(item: &WorkItem) -> String {
    assigned_name_from_fields(&item.fields).unwrap_or_else(|| "unassigned".into())
}

fn number_field(fields: &Value, key: &str) -> Option<f64> {
    value_f64(fields.get(key)?)
}

fn value_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn string_field_owned(fields: &Value, key: &str) -> Option<String> {
    fields
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn value_string(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn value_identity_name(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return (!s.is_empty()).then(|| s.to_string());
    }
    value
        .get("displayName")
        .and_then(Value::as_str)
        .or_else(|| value.get("uniqueName").and_then(Value::as_str))
        .or_else(|| value.get("id").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn tags_field(fields: &Value) -> Vec<String> {
    field_str(fields, "System.Tags")
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn estimate_points(item: &WorkItem) -> f64 {
    number_field(&item.fields, "Microsoft.VSTS.Scheduling.StoryPoints")
        .or_else(|| number_field(&item.fields, "Microsoft.VSTS.Scheduling.Effort"))
        .unwrap_or(0.0)
}

fn estimate_hours(item: &WorkItem) -> f64 {
    number_field(&item.fields, "Microsoft.VSTS.Scheduling.OriginalEstimate")
        .or_else(|| {
            let remaining = number_field(&item.fields, "Microsoft.VSTS.Scheduling.RemainingWork")?;
            let completed = number_field(&item.fields, "Microsoft.VSTS.Scheduling.CompletedWork")
                .unwrap_or(0.0);
            Some(remaining + completed)
        })
        .unwrap_or(0.0)
}

fn in_iteration(snapshot: &ItemSnapshot, item: &WorkItem, iteration_path: &str) -> bool {
    let path = snapshot
        .iteration_path
        .as_deref()
        .or_else(|| field_str(&item.fields, "System.IterationPath"));
    path.is_some_and(|p| path_under_or_equal(p, iteration_path))
}

fn path_under_or_equal(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn is_completed_state(
    item: &WorkItem,
    state: Option<&str>,
    categories: Option<&HashMap<(String, String), String>>,
) -> bool {
    let state = state
        .or_else(|| field_str(&item.fields, "System.State"))
        .unwrap_or("");
    let ty = field_str(&item.fields, "System.WorkItemType").unwrap_or("");
    if let Some(category) = categories
        .and_then(|m| m.get(&(ty.to_string(), state.to_string())))
        .map(String::as_str)
    {
        return category.eq_ignore_ascii_case("Completed");
    }
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "completed" | "resolved"
    )
}

fn parse_state_filter(input: &str) -> Result<Vec<String>> {
    let states: Vec<String> = input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if states.is_empty() {
        return Err(
            CliError::Validation("--state-filter must include at least one state".into()).into(),
        );
    }
    Ok(states)
}

fn validate_top(top: Option<u32>) -> Result<()> {
    if matches!(top, Some(0)) {
        return Err(CliError::Validation("--top must be greater than 0".into()).into());
    }
    Ok(())
}

fn failures_to_schema(failures: Vec<(u32, anyhow::Error)>) -> Vec<SprintMutationFailure> {
    failures
        .into_iter()
        .map(|(id, e)| SprintMutationFailure {
            id,
            error: format!("{e:#}"),
        })
        .collect()
}

fn parse_ado_datetime(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn parse_ado_date(s: &str) -> Option<NaiveDate> {
    parse_ado_datetime(s)
        .map(|dt| dt.date_naive())
        .or_else(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

fn iteration_dates(iteration: &TeamIteration) -> Vec<NaiveDate> {
    let (start, end) = iteration_date_bounds(iteration);
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        out.push(cursor);
        cursor += Duration::days(1);
    }
    out
}

fn iteration_date_bounds(iteration: &TeamIteration) -> (NaiveDate, NaiveDate) {
    let today = Utc::now().date_naive();
    let start = iteration
        .attributes
        .start_date
        .as_deref()
        .and_then(parse_ado_date)
        .unwrap_or(today);
    let finish = iteration
        .attributes
        .finish_date
        .as_deref()
        .and_then(parse_ado_date)
        .unwrap_or(today);
    let end = std::cmp::min(std::cmp::max(start, finish), std::cmp::max(start, today));
    (start, end)
}

fn rollover_comment(
    from: &TeamIteration,
    to: &TeamIteration,
    batch_id: &str,
    count: usize,
) -> String {
    format!(
        "<p>Rolled over by <code>ado sprint rollover</code> batch <code>{batch_id}</code>.</p><p>Summary: moved {count} unfinished item(s) from <b>{}</b> to <b>{}</b>.</p>",
        from.name, to.name
    )
}

fn render_backlog(response: &SprintBacklogResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text => {
            if response.value.is_empty() {
                println!("(no work items in {})", response.iteration.name);
                return Ok(());
            }
            for item in &response.value {
                println!("{}", work_item_line(item));
            }
            Ok(())
        }
        OutputFormat::Table => {
            if response.value.is_empty() {
                println!("(no work items in {})", response.iteration.name);
                return Ok(());
            }
            output::print_table(
                &["ID", "Type", "State", "Assignee", "Pts", "Effort", "Title"],
                &response.value.iter().map(work_item_row).collect::<Vec<_>>(),
            );
            Ok(())
        }
    }
}

fn render_board(response: &SprintBoardResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text => {
            for col in &response.columns {
                println!("{} ({})", col.name, col.count);
                for item in &col.items {
                    println!("  {}", work_item_line(item));
                }
            }
            Ok(())
        }
        OutputFormat::Table => {
            let mut rows = Vec::new();
            for col in &response.columns {
                if col.items.is_empty() {
                    rows.push(vec![
                        col.name.clone(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ]);
                } else {
                    for item in &col.items {
                        rows.push(vec![
                            col.name.clone(),
                            format!("#{}", item.id),
                            item.work_item_type.clone(),
                            item.state.clone(),
                            item.title.clone(),
                        ]);
                    }
                }
            }
            output::print_table(&["Column", "ID", "Type", "State", "Title"], &rows);
            Ok(())
        }
    }
}

fn render_plan_into(
    response: &SprintPlanIntoResponse,
    total: usize,
    ctx: &CmdCtx<'_>,
) -> Result<()> {
    match ctx.output {
        OutputFormat::Json => output::print_json(response)?,
        OutputFormat::Text | OutputFormat::Table => {
            for item in &response.updated {
                println!("Planned #{} into {}", item.id, response.iteration.name);
            }
            if !ctx.client.explain_enabled() {
                for failure in &response.failures {
                    eprintln!("Failed #{}: {}", failure.id, failure.error);
                }
            }
        }
    }
    if !response.failures.is_empty() {
        if ctx.client.explain_enabled() {
            return Err(CliError::Explain.into());
        }
        bail!(
            "{}/{} plan-into updates failed",
            response.failures.len(),
            total
        );
    }
    Ok(())
}

fn render_capacity(response: &SprintCapacityResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text => {
            println!(
                "{}: total {:.2}h/day, {} day(s) off",
                response.iteration.name,
                response.capacity.total_capacity_per_day,
                response.capacity.total_days_off
            );
            for member in &response.capacity.team_members {
                println!(
                    "{}  {}  days off: {}",
                    member.team_member.display_name,
                    activities_text(&member.activities),
                    days_off_text(&member.days_off)
                );
            }
            Ok(())
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = response
                .capacity
                .team_members
                .iter()
                .map(|m| {
                    vec![
                        m.team_member.display_name.clone(),
                        m.team_member.unique_name.clone(),
                        activities_text(&m.activities),
                        days_off_text(&m.days_off),
                    ]
                })
                .collect();
            output::print_table(&["Member", "Unique Name", "Activities", "Days Off"], &rows);
            Ok(())
        }
    }
}

fn render_capacity_set(response: &SprintCapacitySetResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text | OutputFormat::Table => {
            println!(
                "Updated capacity for {} in {}",
                response.member.team_member.display_name, response.iteration.name
            );
            Ok(())
        }
    }
}

fn render_burndown(response: &SprintBurndownResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text | OutputFormat::Table => {
            print_burndown_points(&response.points);
            if !response.members.is_empty() {
                for member in &response.members {
                    println!();
                    println!("{}", member.member);
                    print_burndown_points(&member.points);
                }
            }
            Ok(())
        }
    }
}

fn render_rollover(response: &SprintRolloverResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text | OutputFormat::Table => {
            let verb = if response.dry_run {
                "Would move"
            } else {
                "Moved"
            };
            println!(
                "{verb} {} item(s) from {} to {}",
                response.count, response.from.name, response.to.name
            );
            for item in &response.moved {
                println!("{}", work_item_line(item));
            }
            for failure in &response.failures {
                eprintln!("Failed #{}: {}", failure.id, failure.error);
            }
            Ok(())
        }
    }
}

fn render_summary(response: &SprintSummaryResponse, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output::print_json(response),
        OutputFormat::Text | OutputFormat::Table => {
            println!("{} summary", response.iteration.name);
            println!(
                "planned:   {} item(s), {:.1} point(s), {:.1}h",
                response.planned_count, response.planned_points, response.planned_hours
            );
            println!(
                "completed: {} item(s), {:.1} point(s), {:.1}h",
                response.completed_count, response.completed_points, response.completed_hours
            );
            println!("carryover: {}", response.carryover_count);
            println!("added mid-sprint: {}", response.additions_mid_sprint_count);
            if !response.per_member.is_empty() {
                let rows: Vec<Vec<String>> = response
                    .per_member
                    .iter()
                    .map(|m| {
                        vec![
                            m.member.clone(),
                            m.total_count.to_string(),
                            m.completed_count.to_string(),
                            m.carryover_count.to_string(),
                            format!("{:.1}", m.points),
                            format!("{:.1}", m.hours),
                        ]
                    })
                    .collect();
                output::print_table(&["Member", "Total", "Done", "Carry", "Pts", "Hours"], &rows);
            }
            Ok(())
        }
    }
}

fn work_item_line(item: &SprintWorkItem) -> String {
    let assignee = item
        .assigned_to
        .as_ref()
        .map(|a| a.display_name.as_str())
        .unwrap_or("unassigned");
    let estimate = item
        .story_points
        .map(|p| format!(" pts:{}", fmt_num(p)))
        .or_else(|| item.effort.map(|e| format!(" effort:{}", fmt_num(e))))
        .unwrap_or_default();
    format!(
        "#{:<5} [{}] [{}] {}  (assigned: {assignee}{estimate})",
        item.id, item.work_item_type, item.state, item.title
    )
}

fn work_item_row(item: &SprintWorkItem) -> Vec<String> {
    vec![
        format!("#{}", item.id),
        item.work_item_type.clone(),
        item.state.clone(),
        item.assigned_to
            .as_ref()
            .map(|a| a.display_name.clone())
            .unwrap_or_else(|| "unassigned".into()),
        item.story_points.map(fmt_num).unwrap_or_default(),
        item.effort.map(fmt_num).unwrap_or_default(),
        item.title.clone(),
    ]
}

fn fmt_num(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn activities_text(activities: &[CapacityActivity]) -> String {
    if activities.is_empty() {
        return "-".into();
    }
    activities
        .iter()
        .map(|a| format!("{}={:.2}", a.name, a.capacity_per_day))
        .collect::<Vec<_>>()
        .join(", ")
}

fn days_off_text(days: &[CapacityDateRange]) -> String {
    if days.is_empty() {
        return "-".into();
    }
    days.iter()
        .map(|d| {
            if d.start == d.end {
                d.start.clone()
            } else {
                format!("{}..{}", d.start, d.end)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_burndown_points(points: &[SprintBurndownPoint]) {
    let max_remaining = points
        .iter()
        .map(|p| p.remaining_hours)
        .fold(0.0_f64, f64::max);
    for p in points {
        let width = if max_remaining <= 0.0 {
            0
        } else {
            ((p.remaining_hours / max_remaining) * 40.0).round() as usize
        };
        println!(
            "{}  rem {:>7.2}  done {:>7.2}  scope {:>7.2}  {}",
            p.date,
            p.remaining_hours,
            p.completed_hours,
            p.scope_hours,
            "#".repeat(width)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_state_filter_trims_and_rejects_empty() {
        assert_eq!(
            parse_state_filter("Active, New ,Doing").unwrap(),
            vec!["Active", "New", "Doing"]
        );
        assert!(parse_state_filter(" , ").is_err());
    }

    #[test]
    fn build_backlog_wiql_includes_filters() {
        let args = BacklogArgs {
            iteration: "@next".into(),
            r#type: Some("Bug".into()),
            state: Some("Active".into()),
            tag: Some("blocked".into()),
            area: Some("Proj\\Area".into()),
            unassigned: true,
            top: Some(10),
            project: None,
        };
        let wiql = build_backlog_wiql(&args, "Proj\\Sprint 1");
        assert!(wiql.contains("[System.WorkItemType] = 'Bug'"));
        assert!(wiql.contains("[System.State] = 'Active'"));
        assert!(wiql.contains("[System.Tags] CONTAINS 'blocked'"));
        assert!(wiql.contains("[System.AssignedTo] = ''"));
    }

    #[test]
    fn path_under_or_equal_matches_children_only() {
        assert!(path_under_or_equal("Proj\\Sprint 1", "Proj\\Sprint 1"));
        assert!(path_under_or_equal(
            "Proj\\Sprint 1\\Child",
            "Proj\\Sprint 1"
        ));
        assert!(!path_under_or_equal("Proj\\Sprint 10", "Proj\\Sprint 1"));
    }
}
