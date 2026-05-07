use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Args)]
pub struct PipelineArgs {
    #[command(subcommand)]
    pub command: PipelineCommand,
}

#[derive(Subcommand)]
pub enum PipelineCommand {
    /// List all pipelines in the project
    List(ListArgs),

    /// Trigger a pipeline run
    Run(RunArgs),

    /// Get the status of a pipeline run
    Status(StatusArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct RunArgs {
    /// Pipeline name or numeric ID
    pub pipeline: String,

    /// Branch to run the pipeline on
    #[arg(long)]
    pub branch: Option<String>,

    /// Pipeline variables in key=value format (repeatable)
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub variables: Vec<String>,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Run ID returned by `ado pipeline run`
    pub run_id: u32,

    /// Pipeline ID (required to look up a specific run)
    #[arg(long)]
    pub pipeline_id: u32,

    /// Poll every 10 seconds until the run finishes
    #[arg(long)]
    pub watch: bool,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: u32,
    pub name: String,

    #[serde(rename = "folderPath")]
    pub folder: Option<String>,

    pub revision: u32,

    #[serde(rename = "_links")]
    pub links: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineListResponse {
    pub value: Vec<Pipeline>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: u32,
    pub name: String,
    pub state: String,
    pub result: Option<String>,

    #[serde(rename = "createdDate")]
    pub created_date: String,

    #[serde(rename = "finishedDate")]
    pub finished_date: Option<String>,

    #[serde(rename = "_links")]
    pub links: Option<serde_json::Value>,
}

// ── Command handlers ──────────────────────────────────────────────────────────

pub async fn run(args: PipelineArgs) -> Result<()> {
    match args.command {
        PipelineCommand::List(a) => list(a).await,
        PipelineCommand::Run(a) => run_pipeline(a).await,
        PipelineCommand::Status(a) => status(a).await,
    }
}

/*
 * IMPLEMENTATION NOTES — list()
 *
 * Endpoint: GET {org}/{project}/_apis/pipelines?api-version=7.1
 *
 * Deserialize as PipelineListResponse.
 * Sort by name before printing.
 *
 * Plain text output per pipeline (one line):
 *   {id}  {name}  (folder: {folderPath or "/"})
 *
 * With --output json, print the full PipelineListResponse.
 */
async fn list(args: ListArgs) -> Result<()> {
    todo!("GET pipelines, print one line per pipeline with ID and name")
}

/*
 * IMPLEMENTATION NOTES — run_pipeline()
 *
 * Step 1 — Resolve pipeline ID from name (if args.pipeline is not numeric):
 *   GET {org}/{project}/_apis/pipelines?api-version=7.1
 *   Search value[].name for a case-insensitive match.
 *   If multiple pipelines match, print them and ask the user to use the numeric ID.
 *   If args.pipeline parses as u32, use it directly as the pipeline ID.
 *
 * Step 2 — Parse variables from "KEY=VALUE" strings:
 *   Split each entry on the first '=' to get (key, value) pairs.
 *   Build a JSON object: { "variables": { "KEY": { "value": "VALUE" }, ... } }
 *
 * Step 3 — Trigger the run:
 *   POST {org}/{project}/_apis/pipelines/{id}/runs?api-version=7.1
 *   Request body:
 *   {
 *     "resources": {
 *       "repositories": {
 *         "self": {
 *           "refName": "refs/heads/<args.branch or 'main'>"
 *         }
 *       }
 *     },
 *     "variables": { ... }   // omit if no variables
 *   }
 *
 * On success, print:
 *   "Started run #{run-id} for pipeline '{name}'"
 *   "Track status: ado pipeline status {run-id} --pipeline-id {pipeline-id}"
 */
async fn run_pipeline(args: RunArgs) -> Result<()> {
    todo!("resolve pipeline ID by name if needed, POST to runs endpoint")
}

/*
 * IMPLEMENTATION NOTES — status()
 *
 * Endpoint: GET {org}/{project}/_apis/pipelines/{pipeline_id}/runs/{run_id}?api-version=7.1
 *
 * Plain text output:
 *   Run #<id>: <name>
 *   State:     <state>          // inProgress | completed | cancelling
 *   Result:    <result or "—">  // succeeded | failed | canceled | partiallySucceeded
 *   Started:   <createdDate>
 *   Finished:  <finishedDate or "still running">
 *
 * With --watch flag:
 *   Poll every 10 seconds using tokio::time::sleep(Duration::from_secs(10)).
 *   On each poll, clear the previous output (print "\x1B[2J\x1B[H" to clear screen)
 *   and re-print the status. Stop polling when state == "completed" or "cancelling".
 *   Print a final "Run finished: <result>" message.
 *
 * With --output json, print the full PipelineRun object.
 */
async fn status(args: StatusArgs) -> Result<()> {
    todo!("GET pipeline run by ID, optionally poll until finished")
}
