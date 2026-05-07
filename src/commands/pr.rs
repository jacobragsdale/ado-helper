use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

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

    /// Approve a pull request (vote = 10)
    Approve(ApproveArgs),

    /// Complete (merge) a pull request
    Complete(CompleteArgs),

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

    /// Repository name (defaults to repo of current directory)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by status
    #[arg(long, default_value = "active")]
    pub status: String,

    /// Repository name (defaults to repo of current directory)
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct ApproveArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name
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

    /// Merge strategy
    #[arg(long, default_value = "squash")]
    pub merge_strategy: String,

    /// Repository name
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Pull request ID
    pub id: u32,

    /// Repository name
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

    #[serde(rename = "isDraft")]
    pub is_draft: bool,

    #[serde(rename = "mergeStatus")]
    pub merge_status: Option<String>,

    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityRef {
    #[serde(rename = "displayName")]
    pub display_name: String,

    #[serde(rename = "uniqueName")]
    pub unique_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrListResponse {
    pub value: Vec<PullRequest>,
    pub count: u32,
}

// ── Command handlers ──────────────────────────────────────────────────────────

pub async fn run(args: PrArgs) -> Result<()> {
    match args.command {
        PrCommand::Create(a) => create(a).await,
        PrCommand::List(a) => list(a).await,
        PrCommand::View(a) => view(a).await,
        PrCommand::Approve(a) => approve(a).await,
        PrCommand::Complete(a) => complete(a).await,
        PrCommand::Open(a) => open(a).await,
    }
}

/*
 * IMPLEMENTATION NOTES — create()
 *
 * Endpoint: POST {org}/{project}/_apis/git/repositories/{repo}/pullrequests?api-version=7.1
 *
 * Request body:
 *   {
 *     "title": "<args.title>",
 *     "description": "<args.description or empty string>",
 *     "sourceRefName": "refs/heads/<args.source>",
 *     "targetRefName": "refs/heads/<args.target>",
 *     "isDraft": <args.draft>,
 *     "reviewers": [{ "id": "<reviewer-id>" }, ...]
 *   }
 *
 * Determining the source branch when --source is not provided:
 *   Run `git rev-parse --abbrev-ref HEAD` and capture its stdout.
 *   If the command fails (not in a git repo), return an error asking the user
 *   to specify --source explicitly.
 *
 * Resolving reviewer IDs:
 *   ADO requires UUIDs for reviewers, not display names. Use the Identities API:
 *   GET {org}/_apis/identities?searchFilter=General&filterValue=<name>&api-version=6.0
 *   Extract the first match's `id` field. If no match, warn and skip that reviewer.
 *
 * Determining the repo when --repo is not provided:
 *   Run `git remote get-url origin` and parse the repo name from the URL.
 *   ADO remote URLs follow the pattern:
 *     https://org@dev.azure.com/org/project/_git/repo-name
 *   The last path segment is the repo name.
 *
 * On success, print: "Created PR #{id}: <title>"
 * Also print the web URL so the user can click to open it.
 */
async fn create(args: CreateArgs) -> Result<()> {
    todo!("POST pullrequests, resolve source branch from git if not provided")
}

/*
 * IMPLEMENTATION NOTES — list()
 *
 * Endpoint: GET {org}/{project}/_apis/git/repositories/{repo}/pullrequests
 *           ?searchCriteria.status={active|completed|abandoned|all}
 *           &api-version=7.1
 *
 * If --repo is not provided, try to infer it from `git remote get-url origin`
 * (same parsing as create()). If not in a git repo, list across all repos using
 * the project-level endpoint:
 *   GET {org}/{project}/_apis/git/pullrequests?searchCriteria.status={status}&api-version=7.1
 *
 * Plain text output format per PR (one line each):
 *   #{id}  [{status}]  <title>  (<sourceRef> -> <targetRef>)  by <createdBy.displayName>
 *
 * With --output json, print the full PrListResponse.
 */
async fn list(args: ListArgs) -> Result<()> {
    todo!("GET pullrequests with status filter, print one line per PR")
}

/*
 * IMPLEMENTATION NOTES — view()
 *
 * Endpoint: GET {org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}?api-version=7.1
 *
 * If --repo is not provided, infer from `git remote get-url origin`.
 *
 * Plain text output (multi-line):
 *   PR #<id>: <title>
 *   Status:   <status>
 *   Author:   <createdBy.displayName> (<createdBy.uniqueName>)
 *   Source:   <sourceRefName stripped of refs/heads/>
 *   Target:   <targetRefName stripped of refs/heads/>
 *   Draft:    yes/no
 *   URL:      <webUrl>
 *
 * With --output json, print the full PullRequest object.
 */
async fn view(args: ViewArgs) -> Result<()> {
    todo!("GET pullrequest by ID and print details")
}

/*
 * IMPLEMENTATION NOTES — approve()
 *
 * Approving requires knowing the current user's identity ID, then casting a vote.
 *
 * Step 1 — Get current user ID:
 *   GET {org}/_apis/connectionData?api-version=5.0
 *   Extract authenticatedUser.id from the response. This is the reviewer UUID
 *   we need to vote as.
 *
 * Step 2 — Submit vote:
 *   PUT {org}/{project}/_apis/git/repositories/{repo}/pullrequests/{pr-id}/reviewers/{user-id}
 *       ?api-version=7.1
 *   Body: { "vote": 10 }
 *   Vote values: 10 = approved, 5 = approved with suggestions,
 *                0 = no vote, -5 = waiting for author, -10 = rejected
 *
 * On success, print: "Approved PR #{id}"
 */
async fn approve(args: ApproveArgs) -> Result<()> {
    todo!("get current user ID then PUT vote=10 on the PR")
}

/*
 * IMPLEMENTATION NOTES — complete()
 *
 * Endpoint: PATCH {org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}?api-version=7.1
 *
 * Request body:
 *   {
 *     "status": "completed",
 *     "completionOptions": {
 *       "deleteSourceBranch": <args.delete_source_branch>,
 *       "mergeStrategy": "<args.merge_strategy>"
 *         // valid values: "noFastForward", "squash", "rebase", "rebaseMerge"
 *     }
 *   }
 *
 * On success, print: "Completed PR #{id}"
 *
 * Note: The PR must be in an "active" state and all required policies must be
 * satisfied; otherwise ADO returns 400. The check_response helper will surface
 * the error message from ADO.
 */
async fn complete(args: CompleteArgs) -> Result<()> {
    todo!("PATCH pullrequest with status=completed and merge options")
}

/*
 * IMPLEMENTATION NOTES — open()
 *
 * Build the browser URL:
 *   https://dev.azure.com/{org-name}/{project}/_git/{repo}/pullrequest/{id}
 *
 * Note: org-name is the last path segment of the org URL
 *   (strip "https://dev.azure.com/" from config.org).
 *
 * If --repo is not provided, infer from `git remote get-url origin`.
 *
 * Then call client::AdoClient::open_in_browser(&url).
 * Print: "Opening PR #{id} in browser..."
 */
async fn open(args: OpenArgs) -> Result<()> {
    todo!("construct PR web URL and open in browser via cmd /c start")
}
