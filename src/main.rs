use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod output;

use commands::{pipeline, pr, repo, workitem};

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
    let cli = Cli::parse();

    /*
     * IMPLEMENTATION NOTES — main dispatch
     *
     * 1. Load config from disk (config::Config::load()), which reads
     *    %APPDATA%\ado\config.toml on Windows.
     *
     * 2. If --org or --project were passed on the command line, override the
     *    corresponding config field so every subcommand sees the resolved value.
     *
     * 3. Read the PAT token from the PAT environment variable. If it is not set,
     *    print a helpful error message and exit:
     *      eprintln!("Error: PAT environment variable is not set.");
     *      eprintln!("Set it with: $env:PAT = \"<your-token>\"  (PowerShell)");
     *      std::process::exit(1);
     *
     * 4. Build a client::AdoClient from (config.org, config.project, pat_token).
     *    Pass &client and &cli.output into each command handler.
     *
     * 5. Match on cli.command and call the appropriate handler:
     *      Commands::Config(args) => config::run(args, &config).await
     *      Commands::Repo(args)   => repo::run(args, &client, &cli.output).await
     *      Commands::Pr(args)     => pr::run(args, &client, &cli.output).await
     *      Commands::Pipeline(args) => pipeline::run(args, &client, &cli.output).await
     *      Commands::WorkItem(args) => workitem::run(args, &client, &cli.output).await
     *
     * 6. Propagate errors with `?`. Unhandled errors will be printed by anyhow's
     *    default formatter showing a clean message + cause chain.
     */

    match cli.command {
        Commands::Config(args) => config::run(args).await,
        Commands::Repo(args) => repo::run(args).await,
        Commands::Pr(args) => pr::run(args).await,
        Commands::Pipeline(args) => pipeline::run(args).await,
        Commands::WorkItem(args) => workitem::run(args).await,
    }
}
