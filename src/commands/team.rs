//! `ado team` — list teams, view membership, persist the default team.
//!
//! Teams are scoped to a project; the chosen team is persisted under
//! `[config].team` and consumed by every team-scoped command (iteration,
//! capacity, board, sprint).

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::client::encode_path_segment;
use crate::config::Config;
use crate::context::CmdCtx;
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado team list\n  ado team current\n  ado team members\n  ado team members --output json\n  ado team set \"My Team\"\n\nUse --team on any command to override the saved team for that call."
)]
pub struct TeamArgs {
    #[command(subcommand)]
    pub command: TeamCommand,
}

#[derive(Subcommand)]
pub enum TeamCommand {
    /// List all teams in the project
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Show the team currently selected (CLI flag / env / saved config)
    Current,

    /// List members of the resolved team
    Members(MembersArgs),

    /// Persist the default team for this project
    Set(SetArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct MembersArgs {
    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct SetArgs {
    /// Team name to save
    #[arg(value_name = "NAME")]
    pub name: String,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Team {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "projectName")]
    pub project_name: String,
    #[serde(default, rename = "projectId")]
    pub project_id: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TeamListResponse {
    pub value: Vec<Team>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TeamMember {
    pub identity: MemberIdentity,
    #[serde(default, rename = "isTeamAdmin")]
    pub is_team_admin: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemberIdentity {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "displayName")]
    pub display_name: String,
    #[serde(default, rename = "uniqueName")]
    pub unique_name: String,
    #[serde(default)]
    pub descriptor: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TeamMembersResponse {
    pub value: Vec<TeamMember>,
    pub count: u32,
}

// ── Dispatch ────────────────────────────────────────────────────────────────

pub async fn run(args: TeamArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        TeamCommand::List(a) => list(a, ctx).await,
        TeamCommand::Current => current(ctx),
        TeamCommand::Members(a) => members(a, ctx).await,
        TeamCommand::Set(a) => set(a),
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let path = format!(
        "_apis/projects/{project}/teams?api-version=7.1",
        project = encode_path_segment(project)
    );
    let mut resp: TeamListResponse = ctx.client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match ctx.output {
        OutputFormat::Json => output::print_json(&resp),
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no teams in {project})");
                return Ok(());
            }
            for t in &resp.value {
                if t.description.is_empty() {
                    println!("{}", t.name);
                } else {
                    println!("{}  — {}", t.name, t.description);
                }
            }
            Ok(())
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no teams in {project})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|t| vec![t.name.clone(), t.description.clone(), t.id.clone()])
                .collect();
            output::print_table(&["Name", "Description", "ID"], &rows);
            Ok(())
        }
    }
}

// ── current ─────────────────────────────────────────────────────────────────

fn current(ctx: &CmdCtx<'_>) -> Result<()> {
    match &ctx.team {
        Some(t) => println!("{t}"),
        None => println!(
            "(no team set — pass --team, set ADO_TEAM in .env, or run `ado team set <name>`)"
        ),
    }
    Ok(())
}

// ── members ─────────────────────────────────────────────────────────────────

async fn members(args: MembersArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let team = require_team(ctx)?;
    let path = format!(
        "_apis/projects/{project}/teams/{team}/members?api-version=7.1",
        project = encode_path_segment(project),
        team = encode_path_segment(team)
    );
    let mut resp: TeamMembersResponse = ctx.client.get_json(&path).await?;
    resp.value
        .sort_by(|a, b| a.identity.display_name.cmp(&b.identity.display_name));

    match ctx.output {
        OutputFormat::Json => output::print_json(&resp),
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no members in {team})");
                return Ok(());
            }
            for m in &resp.value {
                let admin = if m.is_team_admin { " (admin)" } else { "" };
                if m.identity.unique_name.is_empty() {
                    println!("{}{admin}", m.identity.display_name);
                } else {
                    println!(
                        "{} <{}>{admin}",
                        m.identity.display_name, m.identity.unique_name
                    );
                }
            }
            Ok(())
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no members in {team})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|m| {
                    vec![
                        m.identity.display_name.clone(),
                        m.identity.unique_name.clone(),
                        if m.is_team_admin {
                            "yes".into()
                        } else {
                            "".into()
                        },
                    ]
                })
                .collect();
            output::print_table(&["Display Name", "Unique Name", "Admin"], &rows);
            Ok(())
        }
    }
}

// ── set ─────────────────────────────────────────────────────────────────────

fn set(args: SetArgs) -> Result<()> {
    let mut cfg = Config::load()?;
    cfg.team = Some(args.name.clone());
    cfg.save()?;
    println!("Saved team: {}", args.name);
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Borrow the resolved team or bail with the canonical "no team set" message.
/// Used by team-scoped commands (members here, iteration/capacity elsewhere).
pub fn require_team<'a>(ctx: &'a CmdCtx<'_>) -> Result<&'a str> {
    match ctx.team.as_deref() {
        Some(t) if !t.is_empty() => Ok(t),
        _ => bail!("no team set — pass --team, set ADO_TEAM in .env, or run `ado team set <name>`"),
    }
}
