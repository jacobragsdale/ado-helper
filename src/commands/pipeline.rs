use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::client::{AdoClient, encode_path_segment};
use crate::fields::split_field_arg;
use crate::output::{self, OutputFormat};

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
    #[arg(long, default_value = "main")]
    pub branch: String,

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: u32,
    pub name: String,

    #[serde(default, rename = "folder")]
    pub folder: Option<String>,

    #[serde(default)]
    pub revision: u32,
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
    #[serde(default)]
    pub result: Option<String>,

    #[serde(default, rename = "createdDate")]
    pub created_date: Option<String>,

    #[serde(default, rename = "finishedDate")]
    pub finished_date: Option<String>,
}

// ── Command dispatch ────────────────────────────────────────────────────────

pub async fn run(args: PipelineArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        PipelineCommand::List(a) => list(a, client, output).await,
        PipelineCommand::Run(a) => run_pipeline(a, client, output).await,
        PipelineCommand::Status(a) => status(a, client, output).await,
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
    println!("State:    {}", run.state);
    println!("Result:   {}", run.result.as_deref().unwrap_or("—"));
    println!("Started:  {}", run.created_date.as_deref().unwrap_or("?"));
    println!(
        "Finished: {}",
        run.finished_date.as_deref().unwrap_or("still running")
    );
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
    let mut body = json!({
        "resources": {
            "repositories": {
                "self": {
                    "refName": format!("refs/heads/{branch}"),
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

fn build_variables(entries: &[String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut variables = serde_json::Map::new();
    for entry in entries {
        let (key, value) = split_field_arg(entry)?;
        variables.insert(key.to_string(), json!({ "value": value }));
    }
    Ok(variables)
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
