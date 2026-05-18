//! `ado iteration` — list and inspect iterations for the resolved team.
//!
//! Exposes `parse_iteration_ref`, the canonical resolver for the
//! `@current` / `@next` / `@previous` shorthands. Future sprint commands
//! should consume it from here rather than re-implementing the lookup.

use anyhow::Result;
use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::client::{AdoClient, encode_path_segment};
use crate::commands::team::require_team;
use crate::context::CmdCtx;
use crate::error::CliError;
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado iteration list\n  ado iteration current\n  ado iteration next\n  ado iteration view @current\n  ado iteration view @previous\n  ado iteration view <iteration-id>\n\nRequires a team — pass --team, set ADO_TEAM, or run `ado team set <name>`."
)]
pub struct IterationArgs {
    #[command(subcommand)]
    pub command: IterationCommand,
}

#[derive(Subcommand)]
pub enum IterationCommand {
    /// List all iterations for the resolved team
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Show the current iteration (alias for `view @current`)
    Current(ScopeArgs),

    /// Show the next iteration (alias for `view @next`)
    Next(ScopeArgs),

    /// Show a specific iteration. REF accepts an id or the literals
    /// `@current`, `@next`, `@previous`.
    View(ViewArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ScopeArgs {
    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Iteration id, or one of `@current` / `@next` / `@previous`
    #[arg(value_name = "REF")]
    pub iteration: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

// ── ADO API response shapes ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeamIteration {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub attributes: IterationAttributes,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IterationAttributes {
    #[serde(default, rename = "startDate")]
    pub start_date: Option<String>,
    #[serde(default, rename = "finishDate")]
    pub finish_date: Option<String>,
    /// One of `past`, `current`, `future` (or absent for iterations without dates).
    #[serde(default, rename = "timeFrame")]
    pub time_frame: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct IterationListResponse {
    pub value: Vec<TeamIteration>,
    pub count: u32,
}

// ── Dispatch ────────────────────────────────────────────────────────────────

pub async fn run(args: IterationArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        IterationCommand::List(a) => list(a, ctx).await,
        IterationCommand::Current(a) => view_scope(a, ctx, "@current").await,
        IterationCommand::Next(a) => view_scope(a, ctx, "@next").await,
        IterationCommand::View(a) => view(a, ctx).await,
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let iters = fetch_all(ctx.client, project, team).await?;

    match ctx.output {
        OutputFormat::Json => {
            let resp = IterationListResponse {
                count: iters.len() as u32,
                value: iters,
            };
            output::print_json(&resp)
        }
        OutputFormat::Text => {
            if iters.is_empty() {
                println!("(no iterations for {team})");
                return Ok(());
            }
            for it in &iters {
                println!("{}", iteration_line(it));
            }
            Ok(())
        }
        OutputFormat::Table => {
            if iters.is_empty() {
                println!("(no iterations for {team})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = iters
                .iter()
                .map(|it| {
                    vec![
                        it.name.clone(),
                        it.attributes
                            .time_frame
                            .clone()
                            .unwrap_or_else(|| "-".into()),
                        it.attributes
                            .start_date
                            .clone()
                            .unwrap_or_else(|| "-".into()),
                        it.attributes
                            .finish_date
                            .clone()
                            .unwrap_or_else(|| "-".into()),
                        it.path.clone(),
                    ]
                })
                .collect();
            output::print_table(&["Name", "Timeframe", "Start", "Finish", "Path"], &rows);
            Ok(())
        }
    }
}

// ── view ────────────────────────────────────────────────────────────────────

async fn view_scope(args: ScopeArgs, ctx: &CmdCtx<'_>, alias: &str) -> Result<()> {
    let view_args = ViewArgs {
        iteration: alias.to_string(),
        project: args.project,
    };
    view(view_args, ctx).await
}

async fn view(args: ViewArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let it = parse_iteration_ref(ctx.client, project, team, &args.iteration).await?;
    render_one(&it, ctx.output)
}

fn render_one(it: &TeamIteration, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => output::print_json(it),
        OutputFormat::Text | OutputFormat::Table => {
            println!("name:      {}", it.name);
            println!("id:        {}", it.id);
            println!("path:      {}", it.path);
            if let Some(tf) = &it.attributes.time_frame {
                println!("timeframe: {tf}");
            }
            if let Some(start) = &it.attributes.start_date {
                println!("start:     {start}");
            }
            if let Some(finish) = &it.attributes.finish_date {
                println!("finish:    {finish}");
            }
            Ok(())
        }
    }
}

// ── shared resolver ─────────────────────────────────────────────────────────

/// Resolve an iteration reference for the given team.
///
/// Accepted shapes:
/// - `@current` — the iteration ADO reports as `timeFrame = current`.
/// - `@next`    — the iteration that follows the current one in the team's list.
/// - `@previous` / `@prev` — the iteration that precedes the current one.
/// - anything else — treated as an iteration id (UUID) and fetched directly.
pub async fn parse_iteration_ref(
    client: &AdoClient,
    project: &str,
    team: &str,
    input: &str,
) -> Result<TeamIteration> {
    match input {
        "@current" | "@now" => match find_current(client, project, team).await? {
            Some(it) => Ok(it),
            None => Err(CliError::NotFound("no current iteration for this team".into()).into()),
        },
        "@next" => find_adjacent(client, project, team, 1).await,
        "@previous" | "@prev" => find_adjacent(client, project, team, -1).await,
        other => fetch_one(client, project, team, other).await,
    }
}

async fn find_current(
    client: &AdoClient,
    project: &str,
    team: &str,
) -> Result<Option<TeamIteration>> {
    let path = format!(
        "{project}/{team}/_apis/work/teamsettings/iterations?$timeframe=current&api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team)
    );
    let resp: IterationListResponse = client.get_json(&path).await?;
    Ok(resp.value.into_iter().next())
}

async fn find_adjacent(
    client: &AdoClient,
    project: &str,
    team: &str,
    delta: i32,
) -> Result<TeamIteration> {
    let all = fetch_all(client, project, team).await?;
    let current_pos = all
        .iter()
        .position(|i| {
            i.attributes
                .time_frame
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("current"))
        })
        .ok_or_else(|| CliError::NotFound("no current iteration for this team".into()))?;
    let target = current_pos as i32 + delta;
    if target < 0 || (target as usize) >= all.len() {
        let label = if delta > 0 { "next" } else { "previous" };
        return Err(CliError::NotFound(format!("no @{label} iteration")).into());
    }
    Ok(all.into_iter().nth(target as usize).unwrap())
}

async fn fetch_one(
    client: &AdoClient,
    project: &str,
    team: &str,
    id: &str,
) -> Result<TeamIteration> {
    let path = format!(
        "{project}/{team}/_apis/work/teamsettings/iterations/{id}?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team),
        id = encode_path_segment(id)
    );
    client.get_json(&path).await
}

async fn fetch_all(client: &AdoClient, project: &str, team: &str) -> Result<Vec<TeamIteration>> {
    let path = format!(
        "{project}/{team}/_apis/work/teamsettings/iterations?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team)
    );
    let resp: IterationListResponse = client.get_json(&path).await?;
    Ok(resp.value)
}

fn iteration_line(it: &TeamIteration) -> String {
    let tf = it
        .attributes
        .time_frame
        .as_deref()
        .map(|t| format!("[{t}]"))
        .unwrap_or_default();
    let dates = match (
        it.attributes.start_date.as_deref(),
        it.attributes.finish_date.as_deref(),
    ) {
        (Some(s), Some(f)) => format!("  {s} → {f}"),
        _ => String::new(),
    };
    format!("{:<10} {}{}", tf, it.name, dates)
}
