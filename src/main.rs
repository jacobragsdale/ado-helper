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
#[command(
    name = "ado",
    version,
    about = "Manage Azure DevOps repos, PRs, pipelines, and work items",
    long_about = "ado is a small Azure DevOps CLI for day-to-day project work: list and clone repos, create and review pull requests, run pipelines, and manage work items.\n\nFirst run:\n  1. Create a Personal Access Token in Azure DevOps.\n  2. Set ADO_PAT in your shell or a local .env file.\n  3. Set ADO_ORG_URL and ADO_PROJECT in .env, or save them with ado config set.\n\nConfiguration precedence:\n  CLI flags (--org, --project) override environment variables loaded from .env, which override the saved TOML config.",
    after_help = "Examples:\n  ado config set --org https://dev.azure.com/myorg --project MyProject\n  ado repo list --output table\n  ado pr list --repo my-service --status active\n  ado wi create --title \"Fix login redirect\" --type Bug --assigned-to me\n  ado pipeline run build-main --branch main\n\nUse `ado help <command>` or `ado <command> --help` for workflow-specific examples."
)]
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
    #[command(
        after_help = "Examples:\n  ado config set --org https://dev.azure.com/myorg --project MyProject\n  ado config show\n\nADO_PAT is intentionally not stored in config; set it in your shell or .env."
    )]
    Config(config::ConfigArgs),

    /// Manage Git repositories
    #[command(
        after_help = "Examples:\n  ado repo list --output table\n  ado repo create --name my-service\n  ado repo clone my-service\n  ado repo delete old-service --yes\n\nRepo deletion is permanent in Azure DevOps."
    )]
    Repo(repo::RepoArgs),

    /// Manage pull requests
    #[command(
        after_help = "Examples:\n  ado pr create --repo my-service --title \"Add health check\" --target main\n  ado pr list --repo my-service --status active\n  ado pr view 42 --repo my-service\n  ado pr link-work-item 42 --repo my-service --work-item 123\n  ado pr complete 42 --repo my-service --delete-source-branch\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
    )]
    Pr(pr::PrArgs),

    /// Manage pipelines
    #[command(
        after_help = "Examples:\n  ado pipeline list --output table\n  ado pipeline run build-main --branch main --var smoke=true\n  ado pipeline status 12345 --pipeline-id 67 --watch"
    )]
    Pipeline(pipeline::PipelineArgs),

    /// Manage work items (alias: wi)
    #[command(
        visible_alias = "wi",
        after_help = "Examples:\n  ado wi create --title \"Fix login redirect\" --type Bug --assigned-to me\n  ado wi list --assigned-to me --state Active\n  ado wi update 123 --state Closed --field priority=2\n  ado wi link 123 --child 456\n  ado wi attach 123 ./screenshot.png\n\nUse field aliases like title, state, assigned-to, tags, priority, story-points, and acceptance-criteria with --field."
    )]
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
