use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::commands::me::MeInfo;

/// Top-level args for `ado config`
#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado config set --org https://dev.azure.com/myorg --project MyProject\n  ado config show\n\nSaved config lives in your OS config directory. Environment variables loaded from .env override saved values."
)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Set configuration values
    #[command(
        after_help = "Examples:\n  ado config set --org https://dev.azure.com/myorg\n  ado config set --project MyProject\n  ado config set --org https://dev.azure.com/myorg --project MyProject"
    )]
    Set(SetArgs),

    /// Print current configuration
    #[command(
        after_help = "Example:\n  ado config show\n\nADO_PAT is reported as set or not set, but the token value is never printed."
    )]
    Show,
}

#[derive(Args)]
pub struct SetArgs {
    /// Azure DevOps organization URL (e.g. https://dev.azure.com/myorg)
    #[arg(long, value_name = "URL", value_hint = clap::ValueHint::Url)]
    pub org: Option<String>,

    /// Default project name
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Default team name
    #[arg(long, value_name = "TEAM")]
    pub team: Option<String>,
}

/// Persisted configuration stored in the OS config dir (e.g. ~/.config/ado/config.toml)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Full organization URL, e.g. "https://dev.azure.com/myorg"
    pub org: Option<String>,

    /// Default project name used when --project is not provided
    pub project: Option<String>,

    /// Default team name used when --team is not provided. Consumed by
    /// commands that scope to a team (iteration, capacity, board, sprint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,

    /// Cached caller identity, written by `ado me` and read by commands that
    /// need to resolve "me" (e.g. `wi list --assigned-to me`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<MeInfo>,
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dir = dirs::config_dir().context("could not locate OS config directory")?;
        Ok(dir.join("ado").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str::<Self>(&content).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let content = toml::to_string(self)?;
        std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

pub async fn run(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Set(set) => {
            let mut cfg = Config::load()?;
            if set.org.is_some() {
                cfg.org = set.org;
            }
            if set.project.is_some() {
                cfg.project = set.project;
            }
            if set.team.is_some() {
                cfg.team = set.team;
            }
            cfg.save()?;
            println!("Configuration saved to {}", Config::path()?.display());
        }
        ConfigCommand::Show => {
            let cfg = Config::load()?;
            let path = Config::path()?;
            println!("config file: {}", path.display());
            println!("org:         {}", cfg.org.as_deref().unwrap_or("(not set)"));
            println!(
                "project:     {}",
                cfg.project.as_deref().unwrap_or("(not set)")
            );
            println!(
                "team:        {}",
                cfg.team.as_deref().unwrap_or("(not set)")
            );
            match &cfg.identity {
                Some(id) => println!(
                    "identity:    {} <{}> (cached — refresh with `ado me refresh`)",
                    id.display_name, id.unique_name
                ),
                None => println!("identity:    (not cached — run `ado me`)"),
            }
            println!();
            println!("environment overrides (loaded from .env if present):");
            print_env("ADO_ORG_URL");
            print_env("ADO_PROJECT");
            print_env("ADO_TEAM");
            match std::env::var("ADO_PAT") {
                Ok(_) => println!("  ADO_PAT     = (set, hidden)"),
                Err(_) => println!("  ADO_PAT     = (not set)"),
            }
        }
    }
    Ok(())
}

fn print_env(key: &str) {
    match std::env::var(key) {
        Ok(v) => println!("  {key:<11} = {v}"),
        Err(_) => println!("  {key:<11} = (not set)"),
    }
}
