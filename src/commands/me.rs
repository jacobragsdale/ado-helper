//! `ado me` — show the caller's ADO identity, with on-disk caching so other
//! commands ("@me", `--assigned-to me`) can resolve the user without an extra
//! round-trip to `_apis/connectionData`.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::client::AdoClient;
use crate::config::Config;
use crate::context::CmdCtx;
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado me\n  ado me --output json\n  ado me refresh\n\nThe identity is cached in the config file; other commands consume it to resolve \"me\" without a round-trip."
)]
pub struct MeArgs {
    #[command(subcommand)]
    pub command: Option<MeCommand>,
}

#[derive(Subcommand)]
pub enum MeCommand {
    /// Force a fresh fetch from ADO and overwrite the cached identity
    Refresh,
}

/// The caller's ADO identity. Also persisted under `[identity]` in the config
/// file as a cache so commands like `wi list --assigned-to me` can resolve
/// without a round-trip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MeInfo {
    pub id: String,
    pub descriptor: String,
    pub display_name: String,
    pub unique_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

pub async fn run(args: MeArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let refresh = matches!(args.command, Some(MeCommand::Refresh));
    let me = load_identity(ctx.client, refresh).await?;
    render(&me, ctx.output)
}

/// Cache-aware loader: returns the cached identity when present (unless
/// `refresh`), otherwise fetches and writes the cache.
async fn load_identity(client: &AdoClient, refresh: bool) -> Result<MeInfo> {
    if !refresh {
        if let Ok(cfg) = Config::load() {
            if let Some(id) = cfg.identity {
                return Ok(id);
            }
        }
    }
    let me = fetch_identity(client).await?;
    let mut cfg = Config::load().unwrap_or_default();
    cfg.identity = Some(me.clone());
    if let Err(e) = cfg.save() {
        eprintln!("warning: could not cache identity: {e:#}");
    }
    Ok(me)
}

/// Fetch the caller's identity from `_apis/connectionData`. This is the same
/// endpoint that `workitem::helpers::resolve_user` has always used; centralising
/// the parsing here means the cache and the resolver agree on shape.
pub async fn fetch_identity(client: &AdoClient) -> Result<MeInfo> {
    let v: serde_json::Value = client
        .get_json("_apis/connectionData?api-version=7.1-preview.1")
        .await
        .context("fetching connectionData")?;
    let user = &v["authenticatedUser"];
    let id = user["id"].as_str().unwrap_or_default().to_string();
    let descriptor = user["descriptor"].as_str().unwrap_or_default().to_string();
    let display_name = user["providerDisplayName"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let unique_name = user["properties"]["Account"]["$value"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if id.is_empty() && unique_name.is_empty() && display_name.is_empty() {
        bail!("connectionData returned no identity fields");
    }
    let email = if unique_name.contains('@') {
        Some(unique_name.clone())
    } else {
        None
    };
    Ok(MeInfo {
        id,
        descriptor,
        display_name,
        unique_name,
        email,
    })
}

fn render(me: &MeInfo, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => output::print_json(me),
        OutputFormat::Text | OutputFormat::Table => {
            println!("display name: {}", me.display_name);
            println!("unique name:  {}", me.unique_name);
            if let Some(email) = &me.email {
                println!("email:        {email}");
            }
            println!("id:           {}", me.id);
            println!("descriptor:   {}", me.descriptor);
            Ok(())
        }
    }
}
