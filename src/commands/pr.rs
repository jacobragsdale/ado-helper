use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::client::{AdoClient, encode_path_segment};
use crate::commands::repo::{inject_pat, lookup_repo};
use crate::fields::{coerce_value, split_field_arg};
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado pr create --repo my-service --title \"Add health check\" --target main\n  ado pr list --repo my-service --status active --output table\n  ado pr view 42 --repo my-service\n  ado pr link-work-item 42 --repo my-service --work-item 123\n  ado pr checks 42 --repo my-service\n  ado pr threads 42 --repo my-service\n  ado pr checkout 42 --repo my-service\n  ado pr checkout-clean --all\n  ado pr complete 42 --repo my-service --merge-strategy squash --delete-source-branch\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
)]
pub struct PrArgs {
    #[command(subcommand)]
    pub command: PrCommand,
}

#[derive(Subcommand)]
pub enum PrCommand {
    /// Create a new pull request
    #[command(
        after_help = "Examples:\n  ado pr create --repo my-service --title \"Add health check\"\n  ado pr create --repo my-service --title \"Ship feature\" --source feature/login --target main --reviewers alice@example.com,bob@example.com\n  ado pr create --repo my-service --title \"Draft spike\" --draft --field auto-complete=false\n\nIf --source is omitted, ado uses the current git branch."
    )]
    Create(CreateArgs),

    /// List pull requests
    #[command(
        visible_alias = "ls",
        after_help = "Examples:\n  ado pr list --repo my-service\n  ado pr ls --repo my-service --status all --output table\n  ado pr list --status completed"
    )]
    List(ListArgs),

    /// View details of a pull request
    #[command(
        visible_alias = "show",
        after_help = "Examples:\n  ado pr view 42 --repo my-service\n  ado pr show 42 --repo my-service --output json"
    )]
    View(ViewArgs),

    /// Link a work item to a pull request
    #[command(
        after_help = "Examples:\n  ado pr link-work-item 42 --repo my-service --work-item 123\n  ado pr link-work-item 42 --work-item 123 --output json\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
    )]
    LinkWorkItem(LinkWorkItemArgs),

    /// Show policy/check evaluations on a pull request
    #[command(
        visible_alias = "check",
        after_help = "Examples:\n  ado pr checks 42 --repo my-service\n  ado pr check 42 --output table\n\nShows every branch-policy gate (build validation, required reviewers, comment resolution, status posts) on the PR."
    )]
    Checks(ChecksArgs),

    /// Edit title / description / arbitrary fields on a pull request
    Update(UpdateArgs),

    /// Approve a pull request (vote = 10, configurable via --vote)
    Approve(ApproveArgs),

    /// Post a top-level comment on a pull request
    Comment(CommentArgs),

    /// List comment threads on a pull request
    #[command(
        after_help = "Examples:\n  ado pr threads 42 --repo my-service\n  ado pr threads 42 --repo my-service --output table\n\nUse thread IDs from this command with `ado pr thread-reply` or `ado pr thread-resolve`."
    )]
    Threads(ThreadsArgs),

    /// Reply to an existing pull request comment thread
    ThreadReply(ThreadReplyArgs),

    /// Close an existing pull request comment thread
    ThreadResolve(ThreadResolveArgs),

    /// Complete (merge) a pull request
    #[command(
        after_help = "Examples:\n  ado pr complete 42 --repo my-service\n  ado pr complete 42 --repo my-service --merge-strategy squash --delete-source-branch\n  ado pr complete 42 --repo my-service --merge-strategy noFastForward"
    )]
    Complete(CompleteArgs),

    /// Abandon a pull request (close without merging)
    Abandon(AbandonArgs),

    /// Reactivate an abandoned pull request
    Reactivate(ReactivateArgs),

    /// Open a pull request in the browser
    #[command(
        visible_alias = "browse",
        after_help = "Examples:\n  ado pr open 42 --repo my-service\n  ado pr browse 42 --repo my-service"
    )]
    Open(OpenArgs),

    /// Check out a pull request's source branch locally
    #[command(
        after_help = "Examples:\n  ado pr checkout 42 --repo my-service\n  ado pr checkout 42 --branch review/alice-login\n  ado pr checkout 42 --dir ./review-42\n  ado pr checkout 42 --detach\n\nIf you're already inside a clone of the PR's repo, fetches and checks out in place. Otherwise, clones to ~/.ado/reviews/<repo>-pr-<id> (override with --dir). Same-repo PRs only — forked PRs are not yet supported."
    )]
    Checkout(CheckoutArgs),

    /// Remove cached PR-review clones from ~/.ado/reviews
    #[command(
        after_help = "Examples:\n  ado pr checkout-clean 42 --repo my-service\n  ado pr checkout-clean --all\n  ado pr checkout-clean --all --dry-run\n\nOnly removes directories under ~/.ado/reviews — never touches your in-place clones."
    )]
    CheckoutClean(CheckoutCleanArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Pull request title
    #[arg(long, value_name = "TEXT")]
    pub title: String,

    /// Source branch (defaults to current git branch)
    #[arg(long, value_name = "BRANCH")]
    pub source: Option<String>,

    /// Target branch
    #[arg(long, value_name = "BRANCH", default_value = "main")]
    pub target: String,

    /// Description / body text
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    /// Create as a draft PR
    #[arg(long)]
    pub draft: bool,

    /// Comma-separated list of reviewer display names or emails
    #[arg(long, value_delimiter = ',', value_name = "USER[,USER]")]
    pub reviewers: Vec<String>,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,

    /// Generic field set, repeatable. e.g. --field isDraft=false
    #[arg(long, value_name = "NAME=VALUE")]
    pub field: Vec<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum PrStatus {
    Active,
    Completed,
    Abandoned,
    All,
}

impl PrStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
            Self::All => "all",
        }
    }
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by status (active, completed, abandoned, all)
    #[arg(long, value_enum, default_value = "active")]
    pub status: PrStatus,

    /// Repository name (omit to list across the whole project)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct LinkWorkItemArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Work item ID to link to the pull request
    #[arg(long, value_name = "ID")]
    pub work_item: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ChecksArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// New title
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,

    /// New description
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    /// Generic field set, repeatable. Use either short alias or full ADO key.
    /// Examples: --field draft=false  --field status=active  --field autoCompleteSetBy=<id>
    #[arg(long, value_name = "NAME=VALUE")]
    pub field: Vec<String>,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ApproveArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Vote value: 10=approve, 5=approve with suggestions, 0=no vote, -5=waiting, -10=reject
    #[arg(long, default_value_t = 10, allow_hyphen_values = true)]
    pub vote: i32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct CommentArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Comment text (Markdown supported by Azure DevOps)
    #[arg(long, value_name = "MARKDOWN")]
    pub text: String,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ThreadsArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ThreadReplyArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Thread ID (from `ado pr threads`)
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: u32,

    /// Reply text (Markdown supported by Azure DevOps)
    #[arg(long, value_name = "MARKDOWN")]
    pub text: String,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ThreadResolveArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Thread ID (from `ado pr threads`)
    #[arg(value_name = "THREAD_ID")]
    pub thread_id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct CompleteArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Delete the source branch after merge
    #[arg(long)]
    pub delete_source_branch: bool,

    /// Merge strategy to use when completing the PR
    #[arg(long, value_enum, default_value = "squash")]
    pub merge_strategy: MergeStrategy,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum MergeStrategy {
    #[value(name = "noFastForward")]
    NoFastForward,
    Squash,
    Rebase,
    #[value(name = "rebaseMerge")]
    RebaseMerge,
}

impl MergeStrategy {
    fn as_ado(self) -> &'static str {
        match self {
            Self::NoFastForward => "noFastForward",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
            Self::RebaseMerge => "rebaseMerge",
        }
    }
}

#[derive(Args)]
pub struct AbandonArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ReactivateArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct CheckoutArgs {
    /// Pull request ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Local branch name to create (defaults to the PR's source branch name)
    #[arg(long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Check out as a detached HEAD instead of creating a local branch
    #[arg(long, conflicts_with = "branch")]
    pub detach: bool,

    /// Destination directory for fresh-clone mode (defaults to ~/.ado/reviews/<repo>-pr-<id>)
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub dir: Option<String>,

    /// Repository name (defaults to ADO_REPO env var or origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct CheckoutCleanArgs {
    /// PR ID to clean (omit with --all)
    #[arg(value_name = "ID")]
    pub id: Option<u32>,

    /// Remove all review checkouts under ~/.ado/reviews
    #[arg(long, conflicts_with = "id")]
    pub all: bool,

    /// Print what would be removed without removing it
    #[arg(long)]
    pub dry_run: bool,

    /// Repo name when removing a specific PR (defaults to ADO_REPO / origin)
    #[arg(long, value_name = "REPO")]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PrThreadListResponse {
    pub value: Vec<PrThread>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrThread {
    pub id: u32,

    #[serde(default)]
    pub status: serde_json::Value,

    #[serde(default)]
    pub comments: Vec<PrThreadComment>,

    #[serde(default, rename = "threadContext")]
    pub thread_context: Option<PrThreadContext>,

    #[serde(default, rename = "publishedDate")]
    pub published_date: Option<String>,

    #[serde(default, rename = "lastUpdatedDate")]
    pub last_updated_date: Option<String>,

    #[serde(default, rename = "isDeleted")]
    pub is_deleted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrThreadComment {
    pub id: u32,

    #[serde(default, rename = "parentCommentId")]
    pub parent_comment_id: Option<u32>,

    #[serde(default)]
    pub author: Option<IdentityRef>,

    #[serde(default)]
    pub content: Option<String>,

    #[serde(default, rename = "commentType")]
    pub comment_type: serde_json::Value,

    #[serde(default, rename = "publishedDate")]
    pub published_date: Option<String>,

    #[serde(default, rename = "lastUpdatedDate")]
    pub last_updated_date: Option<String>,

    #[serde(default, rename = "isDeleted")]
    pub is_deleted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrThreadContext {
    #[serde(default, rename = "filePath")]
    pub file_path: Option<String>,

    #[serde(default, rename = "leftFileStart")]
    pub left_file_start: Option<PrCommentPosition>,

    #[serde(default, rename = "leftFileEnd")]
    pub left_file_end: Option<PrCommentPosition>,

    #[serde(default, rename = "rightFileStart")]
    pub right_file_start: Option<PrCommentPosition>,

    #[serde(default, rename = "rightFileEnd")]
    pub right_file_end: Option<PrCommentPosition>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrCommentPosition {
    pub line: u32,

    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct IdentitiesResponse {
    value: Vec<IdentityRef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyEvaluationsResponse {
    pub value: Vec<PolicyEvaluation>,
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    #[serde(default, rename = "evaluationId")]
    pub evaluation_id: String,

    pub status: String,

    #[serde(default)]
    pub configuration: Option<PolicyConfiguration>,

    #[serde(default, rename = "startedDate")]
    pub started_date: Option<String>,

    #[serde(default, rename = "completedDate")]
    pub completed_date: Option<String>,

    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyConfiguration {
    #[serde(default, rename = "isBlocking")]
    pub is_blocking: bool,

    #[serde(default, rename = "isEnabled")]
    pub is_enabled: bool,

    #[serde(default, rename = "type")]
    pub policy_type: Option<PolicyType>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyType {
    #[serde(default, rename = "displayName")]
    pub display_name: String,

    #[serde(default)]
    pub id: String,
}

// ── Command dispatch ─────────────────────────────────────────────────────────

pub async fn run(args: PrArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        PrCommand::Create(a) => create(a, client, output).await,
        PrCommand::List(a) => list(a, client, output).await,
        PrCommand::View(a) => view(a, client, output).await,
        PrCommand::LinkWorkItem(a) => link_work_item(a, client, output).await,
        PrCommand::Checks(a) => checks(a, client, output).await,
        PrCommand::Update(a) => update(a, client, output).await,
        PrCommand::Approve(a) => approve(a, client, output).await,
        PrCommand::Comment(a) => comment(a, client, output).await,
        PrCommand::Threads(a) => threads(a, client, output).await,
        PrCommand::ThreadReply(a) => thread_reply(a, client, output).await,
        PrCommand::ThreadResolve(a) => thread_resolve(a, client, output).await,
        PrCommand::Complete(a) => complete(a, client, output).await,
        PrCommand::Abandon(a) => abandon(a, client, output).await,
        PrCommand::Reactivate(a) => reactivate(a, client, output).await,
        PrCommand::Open(a) => open(a, client).await,
        PrCommand::Checkout(a) => checkout(a, client).await,
        PrCommand::CheckoutClean(a) => checkout_clean(a).await,
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_optional(args.repo.as_deref());
    let status = args.status.as_str();
    let project = project_segment(client);
    let path = match &repo {
        Some(r) => format!(
            "{}/_apis/git/repositories/{}/pullrequests?searchCriteria.status={}&api-version=7.1",
            project,
            repo_segment(r),
            encode_path_segment(status)
        ),
        None => format!(
            "{}/_apis/git/pullrequests?searchCriteria.status={}&api-version=7.1",
            project,
            encode_path_segment(status)
        ),
    };

    let resp: PrListResponse = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                let scope = repo.as_deref().unwrap_or(&client.project);
                println!("(no {status} PRs in {scope})");
                return Ok(());
            }
            let lines: Vec<String> = resp
                .value
                .iter()
                .map(|p| {
                    let src = strip_refs_heads(&p.source_ref);
                    let tgt = strip_refs_heads(&p.target_ref);
                    let draft = if p.is_draft { " [draft]" } else { "" };
                    format!(
                        "#{:<5} [{}]{} {}  ({src} -> {tgt})  by {}",
                        p.id, p.status, draft, p.title, p.created_by.display_name
                    )
                })
                .collect();
            output::print_text(&lines);
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                let scope = repo.as_deref().unwrap_or(&client.project);
                println!("(no {status} PRs in {scope})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|p| {
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
                })
                .collect();
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
        project_segment(client),
        repo_segment(&repo),
        args.id
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

// ── link-work-item ───────────────────────────────────────────────────────────

async fn link_work_item(
    args: LinkWorkItemArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let pr_path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: serde_json::Value = client.get_json(&pr_path).await?;
    let artifact_id = pr
        .get("artifactId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .with_context(|| {
            format!(
                "PR #{} response missing artifactId; cannot link work item",
                args.id
            )
        })?;

    let ops = build_pr_artifact_link_patch(artifact_id);
    let wi_path = format!("_apis/wit/workitems/{}?api-version=7.1", args.work_item);
    let wi: serde_json::Value = client.patch_json_patch(&wi_path, &ops).await?;

    match output {
        OutputFormat::Json => output::print_json(&wi)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Linked work item #{} to PR #{}", args.work_item, args.id);
        }
    }
    Ok(())
}

// ── checks ──────────────────────────────────────────────────────────────────

async fn checks(args: ChecksArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let pr_path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: serde_json::Value = client.get_json(&pr_path).await?;
    let project_id = pr
        .pointer("/repository/project/id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .with_context(|| {
            format!(
                "PR #{} response missing repository.project.id; cannot fetch policy evaluations",
                args.id
            )
        })?;

    // Policy evaluations key off the CodeReview artifactId, not the Git one
    // returned by the PR endpoint. Build it from projectId + PR id.
    let artifact_id = format!("vstfs:///CodeReview/CodeReviewId/{project_id}/{}", args.id);
    let path = format!(
        "{}/_apis/policy/evaluations?artifactId={}&api-version=7.1-preview.1",
        project_segment(client),
        encode_path_segment(&artifact_id)
    );
    let resp: PolicyEvaluationsResponse = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no policies on PR #{})", args.id);
                return Ok(());
            }
            for ev in &resp.value {
                println!("{}", policy_text_line(ev));
            }
            println!("{}", policy_rollup_line(&resp.value));
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no policies on PR #{})", args.id);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp.value.iter().map(policy_table_row).collect();
            output::print_table(
                &["Status", "Policy", "Blocking", "Started", "Completed"],
                &rows,
            );
            println!("{}", policy_rollup_line(&resp.value));
        }
    }
    Ok(())
}

fn policy_type_name(ev: &PolicyEvaluation) -> &str {
    ev.configuration
        .as_ref()
        .and_then(|c| c.policy_type.as_ref())
        .map(|t| t.display_name.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("(unknown)")
}

fn policy_is_blocking(ev: &PolicyEvaluation) -> bool {
    ev.configuration
        .as_ref()
        .map(|c| c.is_blocking)
        .unwrap_or(false)
}

fn policy_text_line(ev: &PolicyEvaluation) -> String {
    let kind = if policy_is_blocking(ev) {
        "blocking"
    } else {
        "advisory"
    };
    let duration = match (ev.started_date.as_deref(), ev.completed_date.as_deref()) {
        (Some(s), Some(c)) => format!("  {s} → {c}"),
        (Some(s), None) => format!("  started {s}"),
        _ => String::new(),
    };
    format!(
        "[{}] {} ({kind}){duration}",
        ev.status,
        policy_type_name(ev)
    )
}

fn policy_table_row(ev: &PolicyEvaluation) -> Vec<String> {
    vec![
        ev.status.clone(),
        policy_type_name(ev).to_string(),
        if policy_is_blocking(ev) { "yes" } else { "no" }.to_string(),
        ev.started_date.clone().unwrap_or_default(),
        ev.completed_date.clone().unwrap_or_default(),
    ]
}

fn policy_rollup_line(evals: &[PolicyEvaluation]) -> String {
    let blocking: Vec<&PolicyEvaluation> = evals.iter().filter(|e| policy_is_blocking(e)).collect();
    let approved = blocking
        .iter()
        .filter(|e| e.status.eq_ignore_ascii_case("approved"))
        .count();
    format!("{}/{} blocking policies passed", approved, blocking.len())
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
        project_segment(client),
        repo_segment(&repo)
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
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: PullRequest = client.patch_json(&path, &value).await?;

    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => println!("Updated PR #{}", pr.id),
    }
    Ok(())
}

// ── approve ─────────────────────────────────────────────────────────────────

async fn approve(args: ApproveArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let user_id = self_identity_id(client).await?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}/reviewers/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id,
        encode_path_segment(&user_id)
    );
    let resp = client
        .put(&path) // helper added below
        .header("Content-Type", "application/json")
        .json(&json!({ "vote": args.vote }))
        .send()
        .await?;
    let resp = AdoClient::check_response(resp).await?;
    let reviewer: serde_json::Value = resp.json().await.context("failed to parse JSON response")?;

    match output {
        OutputFormat::Json => output::print_json(&reviewer)?,
        OutputFormat::Text | OutputFormat::Table => {
            let action = match args.vote {
                v if v >= 10 => "Approved",
                v if v >= 5 => "Approved (with suggestions)",
                0 => "Reset vote on",
                v if v >= -5 => "Marked waiting on",
                _ => "Rejected",
            };
            println!("{action} PR #{} (vote={})", args.id, args.vote);
        }
    }
    Ok(())
}

// ── comments / threads ─────────────────────────────────────────────────────

async fn comment(args: CommentArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = pr_threads_path(client, &repo, args.id);
    let body = json!({
        "comments": [
            {
                "parentCommentId": 0,
                "content": args.text,
                "commentType": 1,
            }
        ],
        "status": 1,
    });
    let thread: PrThread = client.post_json(&path, &body).await?;

    match output {
        OutputFormat::Json => output::print_json(&thread)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Added comment on PR #{} (thread #{})", args.id, thread.id);
        }
    }
    Ok(())
}

async fn threads(args: ThreadsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = pr_threads_path(client, &repo, args.id);
    let resp: PrThreadListResponse = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => print_threads_text(args.id, &resp.value),
        OutputFormat::Table => print_threads_table(args.id, &resp.value),
    }
    Ok(())
}

async fn thread_reply(
    args: ThreadReplyArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = pr_thread_comments_path(client, &repo, args.id, args.thread_id);
    let body = json!({
        "content": args.text,
        "parentCommentId": 1,
        "commentType": 1,
    });
    let comment: PrThreadComment = client.post_json(&path, &body).await?;

    match output {
        OutputFormat::Json => output::print_json(&comment)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!(
                "Replied to thread #{} on PR #{} (comment #{})",
                args.thread_id, args.id, comment.id
            );
        }
    }
    Ok(())
}

async fn thread_resolve(
    args: ThreadResolveArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = pr_thread_path(client, &repo, args.id, args.thread_id);
    let thread: PrThread = client.patch_json(&path, &json!({ "status": 4 })).await?;

    match output {
        OutputFormat::Json => output::print_json(&thread)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Closed thread #{} on PR #{}", args.thread_id, args.id);
        }
    }
    Ok(())
}

// ── complete / abandon / reactivate ─────────────────────────────────────────

async fn complete(args: CompleteArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let current: serde_json::Value = client.get_json(&path).await?;
    let last_merge_source_commit = current
        .get("lastMergeSourceCommit")
        .cloned()
        .context("PR response missing lastMergeSourceCommit; cannot complete safely")?;
    let body = json!({
        "status": "completed",
        "lastMergeSourceCommit": last_merge_source_commit,
        "completionOptions": {
            "deleteSourceBranch": args.delete_source_branch,
            "mergeStrategy": args.merge_strategy.as_ado(),
        }
    });
    let mut pr: PullRequest = client.patch_json(&path, &body).await?;
    for _ in 0..10 {
        if pr.status == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        pr = client.get_json(&path).await?;
    }
    if pr.status != "completed" {
        let merge = pr.merge_status.as_deref().unwrap_or("unknown");
        bail!(
            "PR #{} did not complete; status={}, mergeStatus={merge}",
            pr.id,
            pr.status
        );
    }
    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Completed PR #{} ({})", pr.id, args.merge_strategy.as_ado())
        }
    }
    Ok(())
}

async fn abandon(args: AbandonArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: PullRequest = client
        .patch_json(&path, &json!({ "status": "abandoned" }))
        .await?;
    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => println!("Abandoned PR #{}", args.id),
    }
    Ok(())
}

async fn reactivate(args: ReactivateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: PullRequest = client
        .patch_json(&path, &json!({ "status": "active" }))
        .await?;
    match output {
        OutputFormat::Json => output::print_json(&pr)?,
        OutputFormat::Text | OutputFormat::Table => println!("Reactivated PR #{}", args.id),
    }
    Ok(())
}

// ── open ────────────────────────────────────────────────────────────────────

async fn open(args: OpenArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let url = web_url(client, &repo, args.id);
    println!("Opening PR #{} in browser...", args.id);
    AdoClient::open_in_browser(&url)
}

// ── checkout / checkout-clean ───────────────────────────────────────────────

async fn checkout(args: CheckoutArgs, client: &AdoClient) -> Result<()> {
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let pr_path = format!(
        "{}/_apis/git/repositories/{}/pullrequests/{}?api-version=7.1",
        project_segment(client),
        repo_segment(&repo),
        args.id
    );
    let pr: serde_json::Value = client.get_json(&pr_path).await?;

    if pr.get("forkSource").is_some_and(|v| !v.is_null()) {
        bail!(
            "PR #{} is from a forked repository; cross-fork checkout is not supported yet",
            args.id
        );
    }

    let source_ref = pr
        .get("sourceRefName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .with_context(|| format!("PR #{} response missing sourceRefName", args.id))?;
    let source_branch = strip_refs_heads(source_ref);

    let local_branch = match args.branch.as_deref() {
        Some(b) => b.to_string(),
        None => source_branch.to_string(),
    };

    let in_place = repo_from_remote()
        .map(|r| r.eq_ignore_ascii_case(&repo))
        .unwrap_or(false)
        && args.dir.is_none();

    if in_place {
        let fetch_spec = if args.detach {
            format!("refs/heads/{source_branch}")
        } else {
            format!("refs/heads/{source_branch}:{local_branch}")
        };
        run_git(None, &["fetch", "origin", &fetch_spec])
            .with_context(|| format!("git fetch origin {fetch_spec} failed"))?;
        if args.detach {
            run_git(None, &["checkout", "--detach", "FETCH_HEAD"])
                .context("git checkout --detach FETCH_HEAD failed")?;
            println!(
                "Checked out PR #{} ({}) in place (detached HEAD)",
                args.id, source_branch
            );
        } else {
            run_git(None, &["checkout", &local_branch])
                .with_context(|| format!("git checkout {local_branch} failed"))?;
            println!(
                "Checked out PR #{} ({} -> {}) in place",
                args.id, source_branch, local_branch
            );
        }
        return Ok(());
    }

    // Mode B — fresh clone.
    let dest = match args.dir.as_deref() {
        Some(d) => PathBuf::from(d),
        None => default_review_dir(&repo, args.id)?,
    };

    if dest.exists() {
        let non_empty = std::fs::read_dir(&dest)
            .with_context(|| format!("could not read {}", dest.display()))?
            .next()
            .is_some();
        if non_empty {
            bail!(
                "destination {} already exists and is not empty — pass --dir <path> or run `ado pr checkout-clean {} --repo {}`",
                dest.display(),
                args.id,
                repo
            );
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let repo_info = lookup_repo(client, &client.project, &repo).await?;
    let auth_url = inject_pat(&repo_info.remote_url, client.pat())?;

    println!("Cloning {} into {}...", repo, dest.display());
    let dest_str = dest_to_string(&dest)?;
    run_git(None, &["clone", "--no-checkout", &auth_url, &dest_str]).context("git clone failed")?;

    // Fetch the PR source branch BEFORE rewriting origin to the credential-free
    // URL, so the fetch can reuse the cloned PAT-bearing remote.
    let fetch_spec = if args.detach {
        format!("refs/heads/{source_branch}")
    } else {
        format!("refs/heads/{source_branch}:{local_branch}")
    };
    run_git(Some(&dest), &["fetch", "origin", &fetch_spec])
        .with_context(|| format!("git fetch origin {fetch_spec} failed"))?;

    // Now strip the PAT from .git/config so it doesn't leak via `git remote -v`.
    run_git(
        Some(&dest),
        &["remote", "set-url", "origin", &repo_info.remote_url],
    )
    .context("could not rewrite origin URL after clone")?;

    if args.detach {
        run_git(Some(&dest), &["checkout", "--detach", "FETCH_HEAD"])
            .context("git checkout --detach FETCH_HEAD failed")?;
        println!(
            "Checked out PR #{} ({}) at {} (detached HEAD)",
            args.id,
            source_branch,
            dest.display()
        );
    } else {
        run_git(Some(&dest), &["checkout", &local_branch])
            .with_context(|| format!("git checkout {local_branch} failed"))?;
        println!(
            "Checked out PR #{} ({}) at {}",
            args.id,
            source_branch,
            dest.display()
        );
    }
    Ok(())
}

async fn checkout_clean(args: CheckoutCleanArgs) -> Result<()> {
    let reviews_root = reviews_root()?;
    let canonical_root = reviews_root.canonicalize().ok();

    if args.all {
        if !reviews_root.exists() {
            println!("(no review checkouts)");
            return Ok(());
        }
        let mut found = false;
        let entries = std::fs::read_dir(&reviews_root)
            .with_context(|| format!("could not read {}", reviews_root.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            found = true;
            if args.dry_run {
                println!("would remove: {}", path.display());
                continue;
            }
            if !path_is_under(&path, canonical_root.as_deref()) {
                eprintln!(
                    "warning: refusing to remove {} (not under {})",
                    path.display(),
                    reviews_root.display()
                );
                continue;
            }
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("could not remove {}", path.display()))?;
            println!("removed: {}", path.display());
        }
        if !found {
            println!("(no review checkouts)");
        }
        return Ok(());
    }

    let id = args
        .id
        .context("pass an ID or --all to ado pr checkout-clean")?;
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let path = reviews_root.join(format!("{repo}-pr-{id}"));

    if !path.exists() {
        println!("(no checkout for PR #{id})");
        return Ok(());
    }

    if args.dry_run {
        println!("would remove: {}", path.display());
        return Ok(());
    }

    if !path_is_under(&path, canonical_root.as_deref()) {
        bail!(
            "refusing to remove {} (not under {})",
            path.display(),
            reviews_root.display()
        );
    }
    std::fs::remove_dir_all(&path)
        .with_context(|| format!("could not remove {}", path.display()))?;
    println!("removed: {}", path.display());
    Ok(())
}

fn reviews_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not resolve $HOME")?;
    Ok(home.join(".ado").join("reviews"))
}

fn default_review_dir(repo: &str, id: u32) -> Result<PathBuf> {
    Ok(reviews_root()?.join(format!("{repo}-pr-{id}")))
}

fn dest_to_string(p: &Path) -> Result<String> {
    p.to_str()
        .map(String::from)
        .with_context(|| format!("destination path is not valid UTF-8: {}", p.display()))
}

fn path_is_under(path: &Path, canonical_root: Option<&Path>) -> bool {
    let Some(root) = canonical_root else {
        // The reviews root doesn't exist (yet); nothing under it to remove anyway.
        return false;
    };
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => return false,
    };
    canonical.starts_with(root)
}

fn run_git(cwd: Option<&Path>, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    if let Some(dir) = cwd {
        cmd.arg("-C").arg(dir);
    }
    cmd.args(args);
    let status = cmd
        .status()
        .context("failed to invoke git — is it installed and on PATH?")?;
    if !status.success() {
        bail!("git {} exited with status {status}", args.join(" "));
    }
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn web_url(client: &AdoClient, repo: &str, pr_id: u32) -> String {
    format!(
        "{}/{}/_git/{}/pullrequest/{}",
        client.org,
        project_segment(client),
        repo_segment(repo),
        pr_id
    )
}

fn pr_threads_path(client: &AdoClient, repo: &str, pr_id: u32) -> String {
    format!(
        "{}/_apis/git/repositories/{}/pullRequests/{}/threads?api-version=7.1",
        project_segment(client),
        repo_segment(repo),
        pr_id
    )
}

fn pr_thread_path(client: &AdoClient, repo: &str, pr_id: u32, thread_id: u32) -> String {
    format!(
        "{}/_apis/git/repositories/{}/pullRequests/{}/threads/{}?api-version=7.1",
        project_segment(client),
        repo_segment(repo),
        pr_id,
        thread_id
    )
}

fn pr_thread_comments_path(client: &AdoClient, repo: &str, pr_id: u32, thread_id: u32) -> String {
    format!(
        "{}/_apis/git/repositories/{}/pullRequests/{}/threads/{}/comments?api-version=7.1",
        project_segment(client),
        repo_segment(repo),
        pr_id,
        thread_id
    )
}

fn project_segment(client: &AdoClient) -> String {
    encode_path_segment(&client.project)
}

fn repo_segment(repo: &str) -> String {
    encode_path_segment(repo)
}

fn build_pr_artifact_link_patch(artifact_id: &str) -> serde_json::Value {
    json!([
        {
            "op": "add",
            "path": "/relations/-",
            "value": {
                "rel": "ArtifactLink",
                "url": artifact_id,
                "attributes": { "name": "Pull Request" }
            }
        }
    ])
}

fn strip_refs_heads(s: &str) -> &str {
    s.strip_prefix("refs/heads/").unwrap_or(s)
}

fn print_threads_text(pr_id: u32, threads: &[PrThread]) {
    if threads.is_empty() {
        println!("(no threads on PR #{pr_id})");
        return;
    }

    for thread in threads {
        let status = thread_status_label(&thread.status);
        let location = thread_location(thread);
        let comment_count = visible_comment_count(thread);
        let updated = thread
            .last_updated_date
            .as_deref()
            .or(thread.published_date.as_deref())
            .unwrap_or("");

        println!(
            "#{:<5} [{}] {}  {} comment{}  {}",
            thread.id,
            status,
            location,
            comment_count,
            if comment_count == 1 { "" } else { "s" },
            updated
        );

        if let Some(comment) = first_visible_comment(thread) {
            let author = comment
                .author
                .as_ref()
                .map(|a| a.display_name.as_str())
                .unwrap_or("?");
            let preview = comment_preview(comment.content.as_deref().unwrap_or(""));
            if !preview.is_empty() {
                println!("  {author}: {preview}");
            }
        }
    }
}

fn print_threads_table(pr_id: u32, threads: &[PrThread]) {
    if threads.is_empty() {
        println!("(no threads on PR #{pr_id})");
        return;
    }

    let rows: Vec<Vec<String>> = threads
        .iter()
        .map(|thread| {
            let comment = first_visible_comment(thread);
            let author = comment
                .and_then(|c| c.author.as_ref())
                .map(|a| a.display_name.as_str())
                .unwrap_or("?");
            let preview = comment
                .and_then(|c| c.content.as_deref())
                .map(comment_preview)
                .unwrap_or_default();
            let updated = thread
                .last_updated_date
                .as_deref()
                .or(thread.published_date.as_deref())
                .unwrap_or("");

            vec![
                format!("#{}", thread.id),
                thread_status_label(&thread.status),
                thread_location(thread),
                visible_comment_count(thread).to_string(),
                author.to_string(),
                updated.to_string(),
                preview,
            ]
        })
        .collect();

    output::print_table(
        &[
            "Thread", "Status", "Location", "Comments", "Author", "Updated", "Preview",
        ],
        &rows,
    );
}

fn thread_status_label(status: &serde_json::Value) -> String {
    if let Some(s) = status.as_str() {
        return s.to_string();
    }
    match status.as_u64() {
        Some(0) => "unknown",
        Some(1) => "active",
        Some(2) => "fixed",
        Some(3) => "wontFix",
        Some(4) => "closed",
        Some(5) => "byDesign",
        Some(6) => "pending",
        Some(n) => return n.to_string(),
        None => "?",
    }
    .to_string()
}

fn thread_location(thread: &PrThread) -> String {
    let Some(context) = &thread.thread_context else {
        return "general".to_string();
    };
    let Some(path) = context.file_path.as_deref() else {
        return "general".to_string();
    };

    if let Some(position) = context
        .right_file_start
        .as_ref()
        .or(context.left_file_start.as_ref())
    {
        format!("{path}:{}", position.line)
    } else {
        path.to_string()
    }
}

fn visible_comment_count(thread: &PrThread) -> usize {
    thread.comments.iter().filter(|c| !c.is_deleted).count()
}

fn first_visible_comment(thread: &PrThread) -> Option<&PrThreadComment> {
    thread.comments.iter().find(|c| !c.is_deleted)
}

fn comment_preview(content: &str) -> String {
    const MAX_CHARS: usize = 100;

    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = String::new();
    for c in collapsed.chars().take(MAX_CHARS) {
        preview.push(c);
    }
    if collapsed.chars().count() > MAX_CHARS {
        preview.push_str("...");
    }
    preview
}

/// Apply `--field name=value` entries into a JSON object body. Names are
/// resolved through `resolve_pr_field` (alias map) before insertion.
fn apply_fields(body: &mut serde_json::Value, fields: &[String]) -> Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    let map = body
        .as_object_mut()
        .context("apply_fields: body is not an object")?;
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
        encode_path_segment(name)
    );
    let resp: IdentitiesResponse = client.get_json(&path).await?;
    let mut iter = resp.value.into_iter().filter(|i| !i.id.is_empty());
    let first = iter
        .next()
        .with_context(|| format!("no identity matched '{name}'"))?;
    if iter.next().is_some() {
        eprintln!(
            "warning: multiple identities matched '{name}', using first: {}",
            first.display_name
        );
    }
    Ok(first.id)
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
    let url = String::from_utf8_lossy(&out.stdout);
    repo_name_from_remote_url(url.trim())
}

fn repo_name_from_remote_url(remote_url: &str) -> Option<String> {
    let clean = remote_url
        .split(['?', '#'])
        .next()
        .unwrap_or(remote_url)
        .trim_end_matches(".git");
    clean
        .rsplit(['/', ':'])
        .next()
        .map(String::from)
        .filter(|s| !s.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_pr_artifact_link_patch() {
        let patch = build_pr_artifact_link_patch("vstfs:///Git/PullRequestId/project%2Frepo%2F42");

        assert_eq!(
            patch,
            json!([
                {
                    "op": "add",
                    "path": "/relations/-",
                    "value": {
                        "rel": "ArtifactLink",
                        "url": "vstfs:///Git/PullRequestId/project%2Frepo%2F42",
                        "attributes": { "name": "Pull Request" }
                    }
                }
            ])
        );
    }
}
