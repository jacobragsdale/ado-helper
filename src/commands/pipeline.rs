use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::client::{AdoClient, encode_path_segment};
use crate::context::CmdCtx;
use crate::fields::{coerce_value, split_field_arg};
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado pipeline list --output table\n  ado pipeline run build-main --branch main --var smoke=true\n  ado pipeline runs build-main --max 5\n  ado pipeline logs 12345 --pipeline-id 67\n  ado pipeline preview build-main --branch main"
)]
pub struct PipelineArgs {
    #[command(subcommand)]
    pub command: PipelineCommand,
}

#[derive(Subcommand)]
pub enum PipelineCommand {
    /// List all pipelines in the project
    #[command(
        visible_alias = "ls",
        after_help = "Examples:\n  ado pipeline list\n  ado pipeline ls --output table\n  ado pipeline list --project OtherProject --output json"
    )]
    List(ListArgs),

    /// Trigger a pipeline run
    #[command(
        after_help = "Examples:\n  ado pipeline run build-main --branch main\n  ado pipeline run 67 --branch feature/login --var environment=dev --var smoke=true\n\nThe pipeline argument accepts either a numeric pipeline ID or an exact pipeline name."
    )]
    Run(RunArgs),

    /// List recent runs for a pipeline
    #[command(
        after_help = "Examples:\n  ado pipeline runs build-main\n  ado pipeline runs 67 --branch main --state completed --result succeeded --max 20 --output table\n\nThe pipeline argument accepts either a numeric pipeline ID or an exact pipeline name."
    )]
    Runs(RunsArgs),

    /// Get the status of a pipeline run
    #[command(
        after_help = "Examples:\n  ado pipeline status 12345 --pipeline-id 67\n  ado pipeline status 12345 --pipeline-id 67 --watch\n\nUse the pipeline ID shown by `ado pipeline list` or printed after `ado pipeline run`."
    )]
    Status(StatusArgs),

    /// List or print logs for a pipeline run
    #[command(
        after_help = "Examples:\n  ado pipeline logs 12345 --pipeline-id 67\n  ado pipeline logs 12345 --pipeline-id 67 2\n  ado pipeline logs 12345 --pipeline-id 67 2 --follow\n\nOmit LOG_ID to list available logs. Pass LOG_ID to print plain log content."
    )]
    Logs(LogsArgs),

    /// Preview the final YAML for a pipeline without starting a run
    #[command(
        after_help = "Examples:\n  ado pipeline preview build-main --branch main\n  ado pipeline preview 67 --ref refs/heads/main --var smoke=true --param deploy=false\n  ado pipeline preview build-main --yaml-file azure-pipelines.yml --output json\n\nText output prints final YAML. JSON output prints the full preview response."
    )]
    Preview(PreviewArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct RunArgs {
    /// Pipeline name or numeric ID
    #[arg(value_name = "PIPELINE")]
    pub pipeline: String,

    /// Branch to run the pipeline on
    #[arg(long, value_name = "BRANCH", default_value = "main")]
    pub branch: String,

    /// Pipeline variables in key=value format (repeatable)
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub variables: Vec<String>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct RunsArgs {
    /// Pipeline name or numeric ID
    #[arg(value_name = "PIPELINE")]
    pub pipeline: String,

    /// Only show runs for this branch or ref
    #[arg(long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Only show runs with this state
    #[arg(long, value_name = "STATE")]
    pub state: Option<String>,

    /// Only show runs with this result
    #[arg(long, value_name = "RESULT")]
    pub result: Option<String>,

    /// Maximum number of runs to show after filtering
    #[arg(long, value_name = "N", default_value_t = 10)]
    pub max: usize,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Run ID returned by `ado pipeline run`
    #[arg(value_name = "RUN_ID")]
    pub run_id: u32,

    /// Pipeline ID (required to look up a specific run)
    #[arg(long, value_name = "PIPELINE_ID")]
    pub pipeline_id: u32,

    /// Poll every 10 seconds until the run finishes
    #[arg(long)]
    pub watch: bool,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct LogsArgs {
    /// Run ID returned by `ado pipeline run`
    #[arg(value_name = "RUN_ID")]
    pub run_id: u32,

    /// Optional log ID. Omit to list available logs for the run.
    #[arg(value_name = "LOG_ID")]
    pub log_id: Option<u32>,

    /// Pipeline ID (required to look up logs for a specific run)
    #[arg(long, value_name = "PIPELINE_ID")]
    pub pipeline_id: u32,

    /// Poll an active run and stream new log output until completion
    #[arg(long)]
    pub follow: bool,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct PreviewArgs {
    /// Pipeline name or numeric ID
    #[arg(value_name = "PIPELINE")]
    pub pipeline: String,

    /// Branch to preview
    #[arg(long, value_name = "BRANCH", default_value = "main")]
    pub branch: String,

    /// Full ref to preview, such as refs/heads/main
    #[arg(long = "ref", value_name = "REF")]
    pub ref_name: Option<String>,

    /// Pipeline variables in key=value format (repeatable)
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub variables: Vec<String>,

    /// Template parameters in key=value format (repeatable)
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub parameters: Vec<String>,

    /// YAML override file to preview instead of the committed pipeline YAML
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub yaml_file: Option<PathBuf>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Pipeline {
    pub id: u32,
    pub name: String,

    #[serde(default, rename = "folder")]
    pub folder: Option<String>,

    #[serde(default)]
    pub revision: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PipelineListResponse {
    pub value: Vec<Pipeline>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PipelineRun {
    pub id: u32,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub result: Option<String>,

    #[serde(default, rename = "createdDate")]
    pub created_date: Option<String>,

    #[serde(default, rename = "finishedDate")]
    pub finished_date: Option<String>,

    #[serde(default)]
    pub resources: Option<RunResources>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PipelineRunListResponse {
    pub value: Vec<PipelineRun>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunResources {
    #[serde(default)]
    pub repositories: Option<RunRepositories>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunRepositories {
    #[serde(default, rename = "self")]
    pub self_repo: Option<RunRepository>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunRepository {
    #[serde(default, rename = "refName")]
    pub ref_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PipelineLogListResponse {
    #[serde(default)]
    pub logs: Vec<PipelineLog>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PipelineLog {
    pub id: u32,

    #[serde(default, rename = "createdOn")]
    pub created_on: Option<String>,

    #[serde(default, rename = "lastChangedOn")]
    pub last_changed_on: Option<String>,

    #[serde(default, rename = "lineCount")]
    pub line_count: Option<u32>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default, rename = "signedContent")]
    pub signed_content: Option<SignedContent>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SignedContent {
    pub url: String,

    #[serde(default, rename = "signatureExpires")]
    pub signature_expires: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PreviewRun {
    #[serde(default, rename = "finalYaml")]
    pub final_yaml: Option<String>,
}

// ── Command dispatch ────────────────────────────────────────────────────────

pub async fn run(args: PipelineArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        PipelineCommand::List(a) => list(a, ctx.client, &ctx.output).await,
        PipelineCommand::Run(a) => run_pipeline(a, ctx.client, &ctx.output).await,
        PipelineCommand::Runs(a) => runs(a, ctx.client, &ctx.output).await,
        PipelineCommand::Status(a) => status(a, ctx.client, &ctx.output).await,
        PipelineCommand::Logs(a) => logs(a, ctx.client, &ctx.output).await,
        PipelineCommand::Preview(a) => preview(a, ctx.client, &ctx.output).await,
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let resp = fetch_pipelines(client, project).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no pipelines in {project})");
                return Ok(());
            }
            let id_width = resp
                .value
                .iter()
                .map(|p| p.id.to_string().len())
                .max()
                .unwrap_or(1);
            let name_width = resp.value.iter().map(|p| p.name.len()).max().unwrap_or(0);
            for p in &resp.value {
                let folder = p.folder.as_deref().unwrap_or("/");
                println!(
                    "{:>id$}  {:name$}  (folder: {})",
                    p.id,
                    p.name,
                    folder,
                    id = id_width,
                    name = name_width
                );
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no pipelines in {project})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|p| {
                    vec![
                        p.id.to_string(),
                        p.name.clone(),
                        p.folder.clone().unwrap_or_else(|| "/".to_string()),
                    ]
                })
                .collect();
            output::print_table(&["ID", "Name", "Folder"], &rows);
        }
    }
    Ok(())
}

// ── run ─────────────────────────────────────────────────────────────────────

async fn run_pipeline(args: RunArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let pipeline = resolve_pipeline(client, project, &args.pipeline).await?;
    let body = build_run_body(&args.branch, &args.variables)?;

    let path = format!(
        "{project}/_apis/pipelines/{}/runs?api-version=7.1",
        pipeline.id,
        project = encode_path_segment(project)
    );
    let started: PipelineRun = client.post_json(&path, &body).await?;

    match output {
        OutputFormat::Json => output::print_json(&started)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!(
                "Started run #{} for pipeline '{}'",
                started.id, pipeline.name
            );
            println!(
                "Track status: ado pipeline status {} --pipeline-id {}",
                started.id, pipeline.id
            );
        }
    }
    Ok(())
}

// ── runs ────────────────────────────────────────────────────────────────────

async fn runs(args: RunsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let pipeline = resolve_pipeline(client, project, &args.pipeline).await?;
    let path = format!(
        "{project}/_apis/pipelines/{}/runs?api-version=7.1",
        pipeline.id,
        project = encode_path_segment(project)
    );
    let mut resp: PipelineRunListResponse = client.get_json(&path).await?;
    if args.branch.is_some() {
        hydrate_run_resources(client, project, pipeline.id, &mut resp.value).await?;
    }
    apply_run_filters(
        &mut resp,
        args.branch.as_deref(),
        args.state.as_deref(),
        args.result.as_deref(),
        args.max,
    );

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no runs for pipeline '{}')", pipeline.name);
                return Ok(());
            }
            for run in &resp.value {
                print_run_summary(run);
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no runs for pipeline '{}')", pipeline.name);
                return Ok(());
            }
            output::print_table(
                &[
                    "ID", "Name", "Branch", "State", "Result", "Created", "Finished",
                ],
                &run_table_rows(&resp.value),
            );
        }
    }
    Ok(())
}

// ── status ──────────────────────────────────────────────────────────────────

async fn status(args: StatusArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/pipelines/{}/runs/{}?api-version=7.1",
        args.pipeline_id,
        args.run_id,
        project = encode_path_segment(project)
    );

    if !args.watch {
        let run: PipelineRun = client.get_json(&path).await?;
        match output {
            OutputFormat::Json => output::print_json(&run)?,
            OutputFormat::Text | OutputFormat::Table => print_run_text(&run),
        }
        return Ok(());
    }

    // --watch: poll every 10s, clearing the screen between polls until the
    // run reaches a terminal state. Terminal states per ADO: completed,
    // cancelling (no further updates expected once seen).
    loop {
        let run: PipelineRun = client.get_json(&path).await?;
        // Clear screen + home cursor.
        print!("\x1B[2J\x1B[1;1H");
        match output {
            OutputFormat::Json => output::print_json(&run)?,
            OutputFormat::Text | OutputFormat::Table => {
                print_run_text(&run);
                println!();
                println!("(watching — Ctrl-C to stop)");
            }
        }
        if run.state.eq_ignore_ascii_case("completed") {
            println!();
            println!(
                "Run finished: {}",
                run.result.as_deref().unwrap_or("(no result)")
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

fn print_run_text(run: &PipelineRun) {
    println!("Run #{}: {}", run.id, run.name);
    println!(
        "Branch:   {}",
        run_branch(run).unwrap_or_else(|| "?".to_string())
    );
    println!("State:    {}", run.state);
    println!("Result:   {}", run.result.as_deref().unwrap_or("-"));
    println!("Started:  {}", run.created_date.as_deref().unwrap_or("?"));
    println!(
        "Finished: {}",
        run.finished_date.as_deref().unwrap_or("still running")
    );
}

// ── logs ────────────────────────────────────────────────────────────────────

async fn logs(args: LogsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    if args.follow && args.log_id.is_none() {
        bail!("--follow requires LOG_ID; omit --follow first to list available logs");
    }
    if args.follow && matches!(output, OutputFormat::Json) {
        bail!("--follow streams text output and cannot be combined with --output json");
    }

    match args.log_id {
        Some(log_id) if args.follow => {
            follow_log(client, project, args.pipeline_id, args.run_id, log_id).await
        }
        Some(log_id) => {
            let log = fetch_log(client, project, args.pipeline_id, args.run_id, log_id).await?;
            match output {
                OutputFormat::Json => output::print_json(&log)?,
                OutputFormat::Text | OutputFormat::Table => print_log_content(client, &log).await?,
            }
            Ok(())
        }
        None => {
            let resp = fetch_logs(client, project, args.pipeline_id, args.run_id).await?;
            match output {
                OutputFormat::Json => output::print_json(&resp)?,
                OutputFormat::Text => {
                    if resp.logs.is_empty() {
                        println!("(no logs for run #{})", args.run_id);
                        return Ok(());
                    }
                    for line in log_list_lines(&resp.logs) {
                        println!("{line}");
                    }
                }
                OutputFormat::Table => {
                    if resp.logs.is_empty() {
                        println!("(no logs for run #{})", args.run_id);
                        return Ok(());
                    }
                    output::print_table(
                        &["ID", "Lines", "Created", "Last Changed"],
                        &log_table_rows(&resp.logs),
                    );
                }
            }
            Ok(())
        }
    }
}

// ── preview ─────────────────────────────────────────────────────────────────

async fn preview(args: PreviewArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let pipeline = resolve_pipeline(client, project, &args.pipeline).await?;
    let ref_name = args
        .ref_name
        .as_deref()
        .map(normalize_ref)
        .unwrap_or_else(|| normalize_ref(&args.branch));
    let yaml_override = match args.yaml_file {
        Some(path) => Some(
            std::fs::read_to_string(&path)
                .with_context(|| format!("reading YAML override {}", path.display()))?,
        ),
        None => None,
    };
    let body = build_preview_body(
        &ref_name,
        &args.variables,
        &args.parameters,
        yaml_override.as_deref(),
    )?;

    let path = format!(
        "{project}/_apis/pipelines/{}/preview?api-version=7.1",
        pipeline.id,
        project = encode_path_segment(project)
    );
    let resp: PreviewRun = client.post_json(&path, &body).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!(
                "{}",
                resp.final_yaml
                    .as_deref()
                    .unwrap_or("(preview response did not include finalYaml)")
            );
        }
    }
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────────────

async fn fetch_pipelines(client: &AdoClient, project: &str) -> Result<PipelineListResponse> {
    let path = format!(
        "{project}/_apis/pipelines?api-version=7.1",
        project = encode_path_segment(project)
    );
    let mut resp: PipelineListResponse = client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(resp)
}

fn build_run_body(branch: &str, variables: &[String]) -> Result<serde_json::Value> {
    let ref_name = normalize_ref(branch);
    let mut body = json!({
        "resources": {
            "repositories": {
                "self": {
                    "refName": ref_name,
                }
            }
        }
    });

    let variables = build_variables(variables)?;
    if !variables.is_empty() {
        body["variables"] = serde_json::Value::Object(variables);
    }

    Ok(body)
}

fn build_preview_body(
    ref_name: &str,
    variables: &[String],
    parameters: &[String],
    yaml_override: Option<&str>,
) -> Result<serde_json::Value> {
    let mut body = json!({
        "previewRun": true,
        "resources": {
            "repositories": {
                "self": {
                    "refName": normalize_ref(ref_name),
                }
            }
        }
    });

    let variables = build_variables(variables)?;
    if !variables.is_empty() {
        body["variables"] = serde_json::Value::Object(variables);
    }

    let parameters = build_template_parameters(parameters)?;
    if !parameters.is_empty() {
        body["templateParameters"] = serde_json::Value::Object(parameters);
    }

    if let Some(yaml) = yaml_override {
        body["yamlOverride"] = json!(yaml);
    }

    Ok(body)
}

fn build_variables(entries: &[String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut variables = serde_json::Map::new();
    for entry in entries {
        let (key, value) = split_field_arg(entry)?;
        variables.insert(key.to_string(), json!({ "value": value }));
    }
    Ok(variables)
}

fn build_template_parameters(
    entries: &[String],
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut parameters = serde_json::Map::new();
    for entry in entries {
        let (key, value) = split_field_arg(entry)?;
        parameters.insert(key.to_string(), coerce_value(value));
    }
    Ok(parameters)
}

fn normalize_ref(input: &str) -> String {
    if input.starts_with("refs/") {
        input.to_string()
    } else {
        format!("refs/heads/{input}")
    }
}

fn run_branch(run: &PipelineRun) -> Option<String> {
    run.resources
        .as_ref()
        .and_then(|r| r.repositories.as_ref())
        .and_then(|repos| repos.self_repo.as_ref())
        .and_then(|repo| repo.ref_name.as_deref())
        .map(short_ref)
}

fn short_ref(ref_name: &str) -> String {
    ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/tags/"))
        .unwrap_or(ref_name)
        .to_string()
}

fn apply_run_filters(
    resp: &mut PipelineRunListResponse,
    branch: Option<&str>,
    state: Option<&str>,
    result: Option<&str>,
    max: usize,
) {
    let branch = branch.map(normalize_ref);
    resp.value.retain(|run| {
        let branch_matches = branch.as_deref().is_none_or(|expected| {
            run_ref_name(run).is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        });
        let state_matches = state.is_none_or(|expected| run.state.eq_ignore_ascii_case(expected));
        let result_matches = result.is_none_or(|expected| {
            run.result
                .as_deref()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        });
        branch_matches && state_matches && result_matches
    });
    resp.value.truncate(max);
    resp.count = resp.value.len() as u32;
}

fn run_ref_name(run: &PipelineRun) -> Option<&str> {
    run.resources
        .as_ref()
        .and_then(|r| r.repositories.as_ref())
        .and_then(|repos| repos.self_repo.as_ref())
        .and_then(|repo| repo.ref_name.as_deref())
}

fn print_run_summary(run: &PipelineRun) {
    println!(
        "{}  {}  {}  {}  {}  {}  {}",
        run.id,
        run.name,
        run_branch(run).unwrap_or_else(|| "?".to_string()),
        run.state,
        run.result.as_deref().unwrap_or("-"),
        run.created_date.as_deref().unwrap_or("?"),
        run.finished_date.as_deref().unwrap_or("still running")
    );
}

fn run_table_rows(runs: &[PipelineRun]) -> Vec<Vec<String>> {
    runs.iter()
        .map(|run| {
            vec![
                run.id.to_string(),
                run.name.clone(),
                run_branch(run).unwrap_or_else(|| "?".to_string()),
                run.state.clone(),
                run.result.clone().unwrap_or_else(|| "-".to_string()),
                run.created_date.clone().unwrap_or_else(|| "?".to_string()),
                run.finished_date
                    .clone()
                    .unwrap_or_else(|| "still running".to_string()),
            ]
        })
        .collect()
}

async fn fetch_logs(
    client: &AdoClient,
    project: &str,
    pipeline_id: u32,
    run_id: u32,
) -> Result<PipelineLogListResponse> {
    let path = format!(
        "{project}/_apis/pipelines/{pipeline_id}/runs/{run_id}/logs?api-version=7.1",
        project = encode_path_segment(project)
    );
    client.get_json(&path).await
}

async fn hydrate_run_resources(
    client: &AdoClient,
    project: &str,
    pipeline_id: u32,
    runs: &mut [PipelineRun],
) -> Result<()> {
    for run in runs.iter_mut().filter(|run| run.resources.is_none()) {
        let path = format!(
            "{project}/_apis/pipelines/{pipeline_id}/runs/{}?api-version=7.1",
            run.id,
            project = encode_path_segment(project)
        );
        let detailed: PipelineRun = client.get_json(&path).await?;
        run.resources = detailed.resources;
    }
    Ok(())
}

async fn fetch_log(
    client: &AdoClient,
    project: &str,
    pipeline_id: u32,
    run_id: u32,
    log_id: u32,
) -> Result<PipelineLog> {
    let path = format!(
        "{project}/_apis/pipelines/{pipeline_id}/runs/{run_id}/logs/{log_id}?$expand=signedContent&api-version=7.1",
        project = encode_path_segment(project)
    );
    client.get_json(&path).await
}

async fn print_log_content(client: &AdoClient, log: &PipelineLog) -> Result<()> {
    let content = download_log_content(client, log).await?;
    print!("{content}");
    io::stdout().flush().context("flushing stdout")?;
    Ok(())
}

async fn download_log_content(client: &AdoClient, log: &PipelineLog) -> Result<String> {
    let url = log
        .signed_content
        .as_ref()
        .map(|signed| signed.url.as_str())
        .with_context(|| {
            format!(
                "log {} did not include signedContent.url; try again without --output json or list logs first",
                log.id
            )
        })?;
    client.get_absolute_text(url).await
}

async fn follow_log(
    client: &AdoClient,
    project: &str,
    pipeline_id: u32,
    run_id: u32,
    log_id: u32,
) -> Result<()> {
    let run_path = format!(
        "{project}/_apis/pipelines/{pipeline_id}/runs/{run_id}?api-version=7.1",
        project = encode_path_segment(project)
    );
    let mut printed = String::new();

    loop {
        let log = fetch_log(client, project, pipeline_id, run_id, log_id).await?;
        let content = download_log_content(client, &log).await?;
        let new_content = new_log_content(&printed, &content);
        if !new_content.is_empty() {
            print!("{new_content}");
            io::stdout().flush().context("flushing stdout")?;
        }
        printed = content;

        let run: PipelineRun = client.get_json(&run_path).await?;
        if run.state.eq_ignore_ascii_case("completed") {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

fn new_log_content<'a>(printed: &str, current: &'a str) -> &'a str {
    current.strip_prefix(printed).unwrap_or(current)
}

fn log_list_lines(logs: &[PipelineLog]) -> Vec<String> {
    logs.iter()
        .map(|log| {
            format!(
                "{:>3}  {:>5} lines  created: {}  changed: {}",
                log.id,
                log.line_count.unwrap_or(0),
                log.created_on.as_deref().unwrap_or("?"),
                log.last_changed_on.as_deref().unwrap_or("?")
            )
        })
        .collect()
}

fn log_table_rows(logs: &[PipelineLog]) -> Vec<Vec<String>> {
    logs.iter()
        .map(|log| {
            vec![
                log.id.to_string(),
                log.line_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                log.created_on.clone().unwrap_or_else(|| "?".to_string()),
                log.last_changed_on
                    .clone()
                    .unwrap_or_else(|| "?".to_string()),
            ]
        })
        .collect()
}

/// Accepts a numeric pipeline ID or a (case-insensitive) name. Returns the
/// matching Pipeline. On multiple name matches, lists them and bails so the
/// user can pick by ID.
async fn resolve_pipeline(client: &AdoClient, project: &str, input: &str) -> Result<Pipeline> {
    if let Ok(id) = input.parse::<u32>() {
        let path = format!(
            "{project}/_apis/pipelines/{id}?api-version=7.1",
            project = encode_path_segment(project)
        );
        return client
            .get_json(&path)
            .await
            .with_context(|| format!("could not find pipeline {id}"));
    }

    let resp = fetch_pipelines(client, project).await?;
    select_pipeline(project, input, &resp.value)
}

fn select_pipeline(project: &str, input: &str, pipelines: &[Pipeline]) -> Result<Pipeline> {
    let matches: Vec<&Pipeline> = pipelines
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(input))
        .collect();

    match matches.len() {
        0 => bail!("no pipeline named '{input}' in {project}"),
        1 => Ok(matches[0].clone()),
        _ => {
            let ids: Vec<String> = matches
                .iter()
                .map(|p| format!("  {} (id {})", p.name, p.id))
                .collect();
            bail!(
                "multiple pipelines match '{input}':\n{}\nuse a numeric ID instead",
                ids.join("\n")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_run_body_normalizes_branch_and_variables() {
        let body = build_run_body(
            "refs/heads/main",
            &["smoke=true".to_string(), "environment=dev".to_string()],
        )
        .unwrap();

        assert_eq!(
            body,
            json!({
                "resources": {
                    "repositories": {
                        "self": {
                            "refName": "refs/heads/main"
                        }
                    }
                },
                "variables": {
                    "smoke": { "value": "true" },
                    "environment": { "value": "dev" }
                }
            })
        );
    }

    #[test]
    fn build_preview_body_includes_variables_parameters_and_yaml_override() {
        let body = build_preview_body(
            "main",
            &["smoke=true".to_string()],
            &[
                "deploy=false".to_string(),
                "retries=2".to_string(),
                "environment=dev".to_string(),
            ],
            Some("steps:\n- script: echo hi\n"),
        )
        .unwrap();

        assert_eq!(
            body,
            json!({
                "previewRun": true,
                "resources": {
                    "repositories": {
                        "self": {
                            "refName": "refs/heads/main"
                        }
                    }
                },
                "variables": {
                    "smoke": { "value": "true" }
                },
                "templateParameters": {
                    "deploy": false,
                    "retries": 2,
                    "environment": "dev"
                },
                "yamlOverride": "steps:\n- script: echo hi\n"
            })
        );
    }

    #[test]
    fn normalize_ref_accepts_branch_names_and_full_refs() {
        assert_eq!(normalize_ref("main"), "refs/heads/main");
        assert_eq!(normalize_ref("refs/heads/main"), "refs/heads/main");
        assert_eq!(normalize_ref("refs/tags/v1"), "refs/tags/v1");
    }

    #[test]
    fn apply_run_filters_matches_case_insensitively_and_applies_max() {
        let mut resp = PipelineRunListResponse {
            count: 3,
            value: vec![
                sample_run(1, "refs/heads/main", "completed", Some("succeeded")),
                sample_run(2, "refs/heads/main", "completed", Some("failed")),
                sample_run(3, "refs/heads/feature", "inProgress", None),
            ],
        };

        apply_run_filters(
            &mut resp,
            Some("MAIN"),
            Some("COMPLETED"),
            Some("SUCCEEDED"),
            1,
        );

        assert_eq!(resp.count, 1);
        assert_eq!(resp.value.len(), 1);
        assert_eq!(resp.value[0].id, 1);
    }

    #[test]
    fn new_log_content_returns_only_appended_text() {
        assert_eq!(new_log_content("line 1\n", "line 1\nline 2\n"), "line 2\n");
        assert_eq!(new_log_content("line 1\n", "reset\n"), "reset\n");
        assert_eq!(new_log_content("same\n", "same\n"), "");
    }

    fn sample_run(id: u32, ref_name: &str, state: &str, result: Option<&str>) -> PipelineRun {
        PipelineRun {
            id,
            name: format!("run-{id}"),
            state: state.to_string(),
            result: result.map(str::to_string),
            created_date: Some("2026-05-15T00:00:00Z".to_string()),
            finished_date: None,
            resources: Some(RunResources {
                repositories: Some(RunRepositories {
                    self_repo: Some(RunRepository {
                        ref_name: Some(ref_name.to_string()),
                    }),
                }),
            }),
        }
    }
}
