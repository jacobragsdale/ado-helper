use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod context;
mod error;
mod fields;
mod output;
mod stdin_ids;

use client::AdoClient;
use commands::{area, iteration, me, pipeline, pr, repo, schema, team, workitem};
use config::Config;
use context::CmdCtx;
use error::CliError;

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

    /// Override the default team from config (used by iteration, capacity, board, sprint commands)
    #[arg(long, global = true)]
    team: Option<String>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "text")]
    output: output::OutputFormat,

    /// Suppress decorative output (banners, progress hints). Result lines and errors still print.
    #[arg(long, global = true)]
    quiet: bool,

    /// Dry-run: print the would-be REST call for any mutation and exit without touching ADO.
    #[arg(long, global = true)]
    explain: bool,

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

    /// Show the caller's ADO identity
    #[command(
        after_help = "Examples:\n  ado me\n  ado me --output json\n  ado me refresh\n\nThe identity is cached in your config so other commands resolve \"me\" without an extra round-trip."
    )]
    Me(me::MeArgs),

    /// Manage teams (list, members, set default)
    #[command(
        after_help = "Examples:\n  ado team list\n  ado team current\n  ado team members\n  ado team set \"My Team\"\n\nUse --team on any command to override the saved team for that call."
    )]
    Team(team::TeamArgs),

    /// Manage iterations (list, current, next, view)
    #[command(
        after_help = "Examples:\n  ado iteration list\n  ado iteration current\n  ado iteration next\n  ado iteration view @current\n  ado iteration view @previous\n\nRequires a team — pass --team, set ADO_TEAM, or run `ado team set <name>`."
    )]
    Iteration(iteration::IterationArgs),

    /// Manage area paths (list, tree)
    #[command(
        after_help = "Examples:\n  ado area tree\n  ado area tree --depth 3 --output json\n  ado area list\n\nOutput strings are paste-ready into `--area` or `--field area=...`."
    )]
    Area(area::AreaArgs),

    /// Manage Git repositories
    #[command(
        after_help = "Examples:\n  ado repo list --output table\n  ado repo branches --repo my-service\n  ado repo tags --repo my-service\n  ado repo commits --repo my-service --branch main --max 10\n  ado repo create --name my-service\n  ado repo clone my-service\n  ado repo delete old-service --yes\n\nRepo deletion is permanent in Azure DevOps."
    )]
    Repo(repo::RepoArgs),

    /// Manage pull requests
    #[command(
        after_help = "Examples:\n  ado pr create --repo my-service --title \"Add health check\" --target main\n  ado pr list --repo my-service --status active\n  ado pr list --status all\n  ado pr view 42 --repo my-service\n  ado pr link-work-item 42 --repo my-service --work-item 123\n  ado pr checks 42 --repo my-service\n  ado pr checkout 42 --repo my-service\n  ado pr checkout-clean --all\n  ado pr complete 42 --repo my-service --delete-source-branch\n\n`ado pr list` searches the whole project when --repo is omitted. Repo-specific PR commands use ADO_REPO or the current git origin remote when --repo is omitted."
    )]
    Pr(pr::PrArgs),

    /// Manage pipelines
    #[command(
        after_help = "Examples:\n  ado pipeline list --output table\n  ado pipeline run build-main --branch main --var smoke=true\n  ado pipeline runs build-main --max 5\n  ado pipeline logs 12345 --pipeline-id 67 2\n  ado pipeline preview build-main --branch main"
    )]
    Pipeline(pipeline::PipelineArgs),

    /// Manage work items (alias: wi)
    #[command(
        visible_alias = "wi",
        after_help = "Examples:\n  ado wi create --title \"Fix login redirect\" --type Bug --assigned-to me\n  ado wi list --assigned-to me --state Active\n  ado wi query --wiql \"SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project\"\n  ado wi update 123 --state Closed --field priority=2\n  ado wi link 123 --child 456\n  ado wi attach 123 ./screenshot.png\n\nUse field aliases like title, state, assigned-to, tags, priority, story-points, and acceptance-criteria with --field."
    )]
    WorkItem(workitem::WorkItemArgs),

    /// Print the JSON output schema for a given command path
    #[command(
        after_help = "Examples:\n  ado schema --list\n  ado schema me\n  ado schema wi view\n  ado schema iteration current\n\nUse this to introspect the shape of `--output json` for any command."
    )]
    Schema(schema::SchemaArgs),
}

fn main() {
    // Load .env from the current dir (and walk upward) before parsing CLI/config.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to start tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    let result = runtime.block_on(dispatch(cli));
    std::process::exit(exit_code(result));
}

async fn dispatch(cli: Cli) -> Result<()> {
    let quiet = cli.quiet;
    let explain = cli.explain;
    let output = cli.output;
    let team = resolve_team(cli.team.clone());

    match cli.command {
        Commands::Config(args) => config::run(args).await,
        Commands::Me(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            me::run(args, &ctx).await
        }
        Commands::Team(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            team::run(args, &ctx).await
        }
        Commands::Iteration(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            iteration::run(args, &ctx).await
        }
        Commands::Area(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            area::run(args, &ctx).await
        }
        Commands::Repo(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            repo::run(args, &ctx).await
        }
        Commands::Pr(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            pr::run(args, &ctx).await
        }
        Commands::Pipeline(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            pipeline::run(args, &ctx).await
        }
        Commands::WorkItem(args) => {
            let client = build_client(cli.org, cli.project, explain)?;
            let ctx = CmdCtx {
                client: &client,
                output,
                quiet,
                team,
            };
            workitem::run(args, &ctx).await
        }
        // `ado schema` doesn't hit the network, so it skips build_client
        // entirely — runnable in environments without a configured PAT.
        Commands::Schema(args) => schema::run(args, output).await,
    }
}

/// Resolve the default team: CLI flag → ADO_TEAM env → saved config.
fn resolve_team(cli_team: Option<String>) -> Option<String> {
    cli_team
        .or_else(|| {
            std::env::var("ADO_TEAM")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| Config::load().ok().and_then(|c| c.team))
}

fn exit_code(result: Result<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            if let Some(cli_err) = e.downcast_ref::<CliError>() {
                let code = cli_err.exit_code();
                if code != 0 {
                    eprintln!("error: {cli_err}");
                }
                code
            } else {
                eprintln!("error: {e:#}");
                1
            }
        }
    }
}

// Compile-time fallbacks baked in at build time via environment variables.
const COMPILED_ORG: Option<&str> = option_env!("ADO_ORG_URL");
const COMPILED_PROJECT: Option<&str> = option_env!("ADO_PROJECT");
const COMPILED_PAT: Option<&str> = option_env!("ADO_PAT");

/// Resolve org/project/PAT with precedence: CLI flag → env (.env or shell) → TOML config → compiled-in default.
fn build_client(
    cli_org: Option<String>,
    cli_project: Option<String>,
    explain: bool,
) -> Result<AdoClient> {
    let cfg = Config::load()?;

    let org = cli_org
        .or_else(|| std::env::var("ADO_ORG_URL").ok())
        .or(cfg.org)
        .or_else(|| COMPILED_ORG.map(str::to_owned))
        .context("ADO org URL not set — pass --org, set ADO_ORG_URL in .env, or run `ado config set --org <url>`")?;

    let project = cli_project
        .or_else(|| std::env::var("ADO_PROJECT").ok())
        .or(cfg.project)
        .or_else(|| COMPILED_PROJECT.map(str::to_owned))
        .context("ADO project not set — pass --project, set ADO_PROJECT in .env, or run `ado config set --project <name>`")?;

    let pat = std::env::var("ADO_PAT")
        .ok()
        .or_else(|| COMPILED_PAT.map(str::to_owned))
        .context("ADO_PAT not set — add it to .env (see .env.example)")?;

    let client = AdoClient::new(org, project, pat)?;
    client.set_explain(explain);
    Ok(client)
}
