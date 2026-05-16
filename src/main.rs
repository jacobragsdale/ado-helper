use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod fields;
mod output;

use client::AdoClient;
use commands::{pipeline, pr, repo, workitem};
use config::Config;

/// Azure DevOps CLI — manage repos, PRs, pipelines, and work items
#[derive(Parser)]
#[command(name = "ado", version, about, long_about = None)]
struct Cli {
    /// Override the organization URL from config (e.g. https://dev.azure.com/myorg)
    #[arg(long, global = true)]
    org: Option<String>,

    /// Override the default project from config
    #[arg(long, global = true)]
    project: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "text")]
    output: output::OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configuration (org URL, default project)
    Config(config::ConfigArgs),

    /// Manage Git repositories
    Repo(repo::RepoArgs),

    /// Manage pull requests
    Pr(pr::PrArgs),

    /// Manage pipelines
    Pipeline(pipeline::PipelineArgs),

    /// Manage work items (alias: wi)
    #[command(alias = "wi")]
    WorkItem(workitem::WorkItemArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from the current dir (and walk upward) before parsing CLI/config.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match cli.command {
        Commands::Config(args) => config::run(args).await,
        Commands::Repo(args) => {
            let client = build_client(cli.org, cli.project)?;
            repo::run(args, &client, &cli.output).await
        }
        Commands::Pr(args) => {
            let client = build_client(cli.org, cli.project)?;
            pr::run(args, &client, &cli.output).await
        }
        Commands::Pipeline(args) => {
            let client = build_client(cli.org, cli.project)?;
            pipeline::run(args, &client, &cli.output).await
        }
        Commands::WorkItem(args) => {
            let client = build_client(cli.org, cli.project)?;
            workitem::run(args, &client, &cli.output).await
        }
    }
}

/// Resolve org/project/PAT with precedence: CLI flag → env (.env or shell) → TOML config.
fn build_client(cli_org: Option<String>, cli_project: Option<String>) -> Result<AdoClient> {
    let cfg = Config::load()?;

    let org = cli_org
        .or_else(|| std::env::var("ADO_ORG_URL").ok())
        .or(cfg.org)
        .context("ADO org URL not set — pass --org, set ADO_ORG_URL in .env, or run `ado config set --org <url>`")?;

    let project = cli_project
        .or_else(|| std::env::var("ADO_PROJECT").ok())
        .or(cfg.project)
        .context("ADO project not set — pass --project, set ADO_PROJECT in .env, or run `ado config set --project <name>`")?;

    let pat =
        std::env::var("ADO_PAT").context("ADO_PAT not set — add it to .env (see .env.example)")?;

    AdoClient::new(org, project, pat)
}
