use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use crate::client::AdoClient;
use crate::fields::{coerce_value, split_field_arg};
use crate::output::{self, OutputFormat};

#[derive(Args)]
pub struct PrArgs {
    #[command(subcommand)]
    pub command: PrCommand,
}

#[derive(Subcommand)]
pub enum PrCommand {
    /// Create a new pull request
    Create(CreateArgs),

    /// List pull requests
    List(ListArgs),

    /// View details of a pull request
    View(ViewArgs),

    /// Edit title / description / arbitrary fields on a pull request
    Update(UpdateArgs),

    /// Approve a pull request (vote = 10, configurable via --vote)
    Approve(ApproveArgs),

    /// Complete (merge) a pull request
    Complete(CompleteArgs),

    /// Abandon a pull request (close without merging)
    Abandon(AbandonArgs),

    /// Reactivate an abandoned pull request
    Reactivate(ReactivateArgs),

    /// Open a pull request in the browser
    Open(OpenArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Pull request title
    #[arg(long)]
    pub title: String,

    /// Source branch (defaults to current git branch)
    #[arg(long)]
    pub source: Option<String>,

    /// Target branch
    #[arg(long, default_value = "main")]
    pub target: String,

    /// Description / body text
    #[arg(long)]
    pub description: Option<String>,

    /// Create as a draft PR
    #[arg(long)]
    pub draft: bool,

    /// Comma-separated list of reviewer display names or emails
    #[arg(long, value_delimiter = ',')]
    pub reviewers: Vec<String>,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,

    /// Generic field set, repeatable. e.g. --field isDraft=false
    #[arg(long, value_name = "NAME=VALUE")]
    pub field: Vec<String>,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by status (active, completed, abandoned, all)
    #[arg(long, default_value = "active")]
    pub status: String,

    /// Repository name (omit to list across the whole project)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Pull request ID
    pub id: u32,

    /// New title
    #[arg(long)]
    pub title: Option<String>,

    /// New description
    #[arg(long)]
    pub description: Option<String>,

    /// Generic field set, repeatable. Use either short alias or full ADO key.
    /// Examples: --field draft=false  --field status=active  --field autoCompleteSetBy=<id>
    #[arg(long, value_name = "NAME=VALUE")]
    pub field: Vec<String>,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ApproveArgs {
    /// Pull request ID
    pub id: u32,

    /// Vote value: 10=approve, 5=approve with suggestions, 0=no vote, -5=waiting, -10=reject
    #[arg(long, default_value_t = 10, allow_hyphen_values = true)]
    pub vote: i32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct CompleteArgs {
    /// Pull request ID
    pub id: u32,

    /// Delete the source branch after merge
    #[arg(long)]
    pub delete_source_branch: bool,

    /// Merge strategy: noFastForward | squash | rebase | rebaseMerge
    #[arg(long, default_value = "squash")]
    pub merge_strategy: String,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct AbandonArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ReactivateArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long)]
    pub repo: Option<String>,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PullRequest {
    #[serde(rename = "pullRequestId")]
    pub id: u32,

    pub title: String,
    pub status: String,

    #[serde(rename = "createdBy")]
    pub created_by: IdentityRef,

    #[serde(rename = "sourceRefName")]
    pub source_ref: String,

    #[serde(rename = "targetRefName")]
    pub target_ref: String,

    #[serde(default, rename = "isDraft")]
    pub is_draft: bool,

    #[serde(default, rename = "mergeStatus")]
    pub merge_status: Option<String>,

    pub url: String,

    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityRef {
    #[serde(rename = "displayName")]
    pub display_name: String,

    #[serde(default, rename = "uniqueName")]
    pub unique_name: String,

    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrListResponse {
    pub value: Vec<PullRequest>,
    pub count: u32,
}

#[derive(Debug, Deserialize)]
struct IdentitiesResponse {
    value: Vec<IdentityRef>,
}

// ── Command dispatch ─────────────────────────────────────────────────────────

pub async fn run(args: PrArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        PrCommand::Create(a) => create(a, client, output).await,
        PrCommand::List(a) => list(a, client, output).await,
        PrCommand::View(a) => view(a, client, output).await,
        PrCommand::Update(a) => update(a, client, output).await,
        PrCommand::Approve(a) => approve(a, client).await,
        PrCommand::Complete(a) => complete(a, client, output).await,
        PrCommand::Abandon(a) => abandon(a, client).await,
        PrCommand::Reactivate(a) => reactivate(a, client).await,
        PrCommand::Open(a) => open(a, client).await,
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_optional(args.repo.as_deref());
    let status = &args.status;
    let path = match &repo {
        Some(r) => format!(
            "{}/_apis/git/repositories/{}/pullrequests?searchCriteria.status={}&api-version=7.1",
            client.project, r, status
        ),
        None => format!(
            "{}/_apis/git/pullrequests?searchCriteria.status={}&api-version=7.1",
            client.project, status
        ),
    };

    let resp: PrListResponse = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                let scope = repo.as_deref().unwrap_or(&client.project);
                println!("(no {} PRs in {scope})", status);
                return Ok(());
            }
            let lines: Vec<String> = resp.value.iter().map(|p| {
                let src = strip_refs_heads(&p.source_ref);
                let tgt = strip_refs_heads(&p.target_ref);
                let draft = if p.is_draft { " [draft]" } else { "" };
                format!(
                    "#{:<5} [{}]{} {}  ({src} -> {tgt})  by {}",
                    p.id, p.status, draft, p.title, p.created_by.display_name
                )
            }).collect();
            output::print_text(&lines);
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                let scope = repo.as_deref().unwrap_or(&client.project);
                println!("(no {} PRs in {scope})", status);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp.value.iter().map(|p| {
                let title = if p.is_draft {
                    format!("{} [draft]", p.title)
                } else {
                    p.title.clone()
                };
                vec![
                    format!("#{}", p.id),
                    p.status.clone(),
                    title,
                    strip_refs_heads(&p.source_ref).to_string(),
                    strip_refs_heads(&p.target_ref).to_string(),
                    p.created_by.display_name.clone(),
                ]
            }).collect();
            output::print_table(
                &["ID", "Status", "Title", "Source", "Target", "Author"],
                &rows,
            );
        }
    }
    Ok(())
}

// ── view ────────────────────────────────────────────────────────────────────

async fn view(args: ViewArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        client.project, repo, args.id
    );
    let pr: PullRequest = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("PR #{}: {}", pr.id, pr.title);
            println!("Status:   {}", pr.status);
            println!(
                "Author:   {} ({})",
                pr.created_by.display_name, pr.created_by.unique_name
            );
            println!("Source:   {}", strip_refs_heads(&pr.source_ref));
            println!("Target:   {}", strip_refs_heads(&pr.target_ref));
            println!("Draft:    {}", if pr.is_draft { "yes" } else { "no" });
            if let Some(ms) = &pr.merge_status {
                println!("Merge:    {ms}");
            }
            if let Some(desc) = &pr.description {
                if !desc.is_empty() {
                    println!("Body:     {desc}");
                }
            }
            println!("URL:      {}", web_url(client, &repo, pr.id));
        }
    }
    Ok(())
}

// ── create ──────────────────────────────────────────────────────────────────

async fn create(args: CreateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let source = match args.source {
        Some(s) => s,
        None => current_branch().context("could not detect source branch — pass --source")?,
    };

    let mut body = json!({
        "title": args.title,
        "description": args.description.unwrap_or_default(),
        "sourceRefName": format!("refs/heads/{}", source),
        "targetRefName": format!("refs/heads/{}", args.target),
        "isDraft": args.draft,
    });

    // Resolve reviewer names → identity IDs. Skip (with a warning) on no match
    // so a typo in one name doesn't block PR creation.
    if !args.reviewers.is_empty() {
        let mut resolved = Vec::with_capacity(args.reviewers.len());
        for name in &args.reviewers {
            match resolve_identity(client, name).await {
                Ok(id) => resolved.push(json!({ "id": id })),
                Err(e) => eprintln!("warning: skipping reviewer '{name}': {e}"),
            }
        }
        if !resolved.is_empty() {
            body["reviewers"] = serde_json::Value::Array(resolved);
        }
    }

    // Apply --field overrides last so they win.
    apply_fields(&mut body, &args.field)?;

    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests?api-version=7.1",
        client.project, repo
    );
    let pr: PullRequest = client.post_json(&path, &body).await?;

    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Created PR #{}: {}", pr.id, pr.title);
            println!("URL: {}", web_url(client, &repo, pr.id));
        }
    }
    Ok(())
}

// ── update ──────────────────────────────────────────────────────────────────

async fn update(args: UpdateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let mut body = serde_json::Map::new();

    if let Some(t) = args.title {
        body.insert("title".into(), json!(t));
    }
    if let Some(d) = args.description {
        body.insert("description".into(), json!(d));
    }
    let mut value = serde_json::Value::Object(body);
    apply_fields(&mut value, &args.field)?;

    if value.as_object().map(|m| m.is_empty()).unwrap_or(true) {
        println!("Nothing to update.");
        return Ok(());
    }

    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        client.project, repo, args.id
    );
    let pr: PullRequest = patch_json(client, &path, &value).await?;

    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => println!("Updated PR #{}", pr.id),
    }
    Ok(())
}

// ── approve ─────────────────────────────────────────────────────────────────

async fn approve(args: ApproveArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let user_id = self_identity_id(client).await?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}/reviewers/{}?api-version=7.1",
        client.project, repo, args.id, user_id
    );
    let resp = client
        .put(&path) // helper added below
        .header("Content-Type", "application/json")
        .json(&json!({ "vote": args.vote }))
        .send()
        .await?;
    AdoClient::check_response(resp).await?;

    let action = match args.vote {
        v if v >= 10 => "Approved",
        v if v >= 5 => "Approved (with suggestions)",
        v if v == 0 => "Reset vote on",
        v if v >= -5 => "Marked waiting on",
        _ => "Rejected",
    };
    println!("{action} PR #{} (vote={})", args.id, args.vote);
    Ok(())
}

// ── complete / abandon / reactivate ─────────────────────────────────────────

async fn complete(args: CompleteArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let body = json!({
        "status": "completed",
        "completionOptions": {
            "deleteSourceBranch": args.delete_source_branch,
            "mergeStrategy": args.merge_strategy,
        }
    });
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        client.project, repo, args.id
    );
    let pr: PullRequest = patch_json(client, &path, &body).await?;
    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Completed PR #{} ({})", pr.id, args.merge_strategy)
        }
    }
    Ok(())
}

async fn abandon(args: AbandonArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        client.project, repo, args.id
    );
    let _: PullRequest = patch_json(client, &path, &json!({ "status": "abandoned" })).await?;
    println!("Abandoned PR #{}", args.id);
    Ok(())
}

async fn reactivate(args: ReactivateArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        client.project, repo, args.id
    );
    let _: PullRequest = patch_json(client, &path, &json!({ "status": "active" })).await?;
    println!("Reactivated PR #{}", args.id);
    Ok(())
}

// ── open ────────────────────────────────────────────────────────────────────

async fn open(args: OpenArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let url = web_url(client, &repo, args.id);
    println!("Opening PR #{} in browser...", args.id);
    AdoClient::open_in_browser(&url)
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn web_url(client: &AdoClient, repo: &str, pr_id: u32) -> String {
    format!(
        "{}/{}/_git/{}/pullrequest/{}",
        client.org, client.project, repo, pr_id
    )
}

fn strip_refs_heads(s: &str) -> &str {
    s.strip_prefix("refs/heads/").unwrap_or(s)
}

/// PATCH with application/json (PR endpoints take a flat JSON body, not JSON Patch).
async fn patch_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    client: &AdoClient,
    path: &str,
    body: &B,
) -> Result<T> {
    let resp = client
        .patch(path)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .context("PATCH request failed")?;
    let resp = AdoClient::check_response(resp).await?;
    resp.json::<T>().await.context("failed to parse JSON response")
}

/// Apply `--field name=value` entries into a JSON object body. Names are
/// resolved through `resolve_pr_field` (alias map) before insertion.
fn apply_fields(body: &mut serde_json::Value, fields: &[String]) -> Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    let map = body.as_object_mut().context("apply_fields: body is not an object")?;
    for entry in fields {
        let (name, value) = split_field_arg(entry)?;
        let key = resolve_pr_field(name)?;
        map.insert(key, coerce_value(value));
    }
    Ok(())
}

/// Map a short alias (e.g. "draft", "status") to its ADO PR field name.
/// If `name` contains '.' or any uppercase letter, treat it as a literal ADO key.
fn resolve_pr_field(name: &str) -> Result<String> {
    if name.contains('.') || name.chars().any(|c| c.is_ascii_uppercase()) {
        return Ok(name.to_string());
    }
    let key = name.trim().to_ascii_lowercase().replace('_', "-");
    Ok(match key.as_str() {
        "title"                => "title",
        "description"          => "description",
        "draft" | "is-draft"   => "isDraft",
        "status"               => "status",
        "auto-complete"
      | "auto-complete-set-by" => "autoCompleteSetBy",
        other => bail!("unknown PR field alias '{other}' — pass the full ADO key (e.g. isDraft) or one of: title, description, draft, status, auto-complete"),
    }.to_string())
}

/// Get the authenticated user's identity UUID — used to vote on PRs.
async fn self_identity_id(client: &AdoClient) -> Result<String> {
    let v: serde_json::Value = client
        .get_json("_apis/connectionData?api-version=7.1-preview.1")
        .await
        .context("could not fetch connectionData")?;
    v["authenticatedUser"]["id"]
        .as_str()
        .map(String::from)
        .context("connectionData missing authenticatedUser.id")
}

/// Resolve a reviewer display name / email to an identity UUID. Returns the
/// first match; warns to stderr if multiple.
async fn resolve_identity(client: &AdoClient, name: &str) -> Result<String> {
    let path = format!(
        "_apis/identities?searchFilter=General&filterValue={}&api-version=6.0",
        url_encode(name)
    );
    let resp: IdentitiesResponse = client.get_json(&path).await?;
    let mut iter = resp.value.into_iter().filter(|i| !i.id.is_empty());
    let first = iter.next().with_context(|| format!("no identity matched '{name}'"))?;
    if iter.next().is_some() {
        eprintln!("warning: multiple identities matched '{name}', using first: {}", first.display_name);
    }
    Ok(first.id)
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".into(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

// ── repo / branch resolution ────────────────────────────────────────────────

fn current_branch() -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("failed to invoke git")?;
    if !out.status.success() {
        bail!("git rev-parse --abbrev-ref HEAD failed (not in a git repo?)");
    }
    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        bail!("could not determine current branch");
    }
    Ok(branch)
}

fn repo_from_remote() -> Option<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let url = url.trim_end_matches(".git");
    url.rsplit('/').next().map(String::from).filter(|s| !s.is_empty())
}

fn resolve_repo_optional(cli: Option<&str>) -> Option<String> {
    if let Some(r) = cli {
        return Some(r.to_string());
    }
    if let Ok(r) = std::env::var("ADO_REPO") {
        if !r.trim().is_empty() {
            return Some(r);
        }
    }
    repo_from_remote()
}

fn resolve_repo_required(cli: Option<&str>) -> Result<String> {
    resolve_repo_optional(cli).context(
        "could not determine repo — pass --repo, set ADO_REPO in .env, or run from a git repo with an ADO origin",
    )
}
