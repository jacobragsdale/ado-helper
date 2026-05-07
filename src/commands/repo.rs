use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Args)]
pub struct RepoArgs {
    #[command(subcommand)]
    pub command: RepoCommand,
}

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Create a new Git repository in the project
    Create(CreateArgs),

    /// List all repositories in the project
    List(ListArgs),

    /// Clone a repository to the current directory
    Clone(CloneArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Name of the new repository
    #[arg(long)]
    pub name: String,

    /// Project to create the repo in (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,

    /// Default branch name
    #[arg(long, default_value = "main")]
    pub default_branch: String,
}

#[derive(Args)]
pub struct ListArgs {
    /// Project to list repos in (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CloneArgs {
    /// Name of the repository to clone
    pub name: String,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name: String,

    #[serde(rename = "remoteUrl")]
    pub remote_url: String,

    #[serde(rename = "defaultBranch")]
    pub default_branch: Option<String>,

    #[serde(rename = "webUrl")]
    pub web_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoListResponse {
    pub value: Vec<Repository>,
    pub count: u32,
}

// ── Command handlers ──────────────────────────────────────────────────────────

pub async fn run(args: RepoArgs) -> Result<()> {
    match args.command {
        RepoCommand::Create(a) => create(a).await,
        RepoCommand::List(a) => list(a).await,
        RepoCommand::Clone(a) => clone(a).await,
    }
}

/*
 * IMPLEMENTATION NOTES — create()
 *
 * Endpoint: POST {org}/{project}/_apis/git/repositories?api-version=7.1
 *
 * Request body:
 *   {
 *     "name": "<args.name>",
 *     "project": { "name": "<project>" },
 *     "defaultBranch": "refs/heads/<args.default_branch>"
 *   }
 *
 * On success, ADO returns the full Repository object.
 * Print: "<repo-name>  <remoteUrl>"
 *
 * Notes:
 *   - The defaultBranch field must be in refs/heads/ format.
 *   - ADO returns 400 if a repo with the same name already exists in the project;
 *     the check_response helper will surface the ADO error message.
 */
async fn create(args: CreateArgs) -> Result<()> {
    todo!("POST to git/repositories, print created repo name and clone URL")
}

/*
 * IMPLEMENTATION NOTES — list()
 *
 * Endpoint: GET {org}/{project}/_apis/git/repositories?api-version=7.1
 *
 * Deserialize the response as RepoListResponse.
 * For each repository in response.value, print one line:
 *   "<name>  <remoteUrl>"
 *
 * Sort alphabetically by name before printing so output is stable.
 *
 * With --output json, print the full RepoListResponse as pretty JSON.
 */
async fn list(args: ListArgs) -> Result<()> {
    todo!("GET git/repositories, print one line per repo")
}

/*
 * IMPLEMENTATION NOTES — clone()
 *
 * 1. Look up the repository by name:
 *    GET {org}/{project}/_apis/git/repositories/{args.name}?api-version=7.1
 *    This returns a single Repository object.
 *
 * 2. Extract the remoteUrl field — this is the HTTPS clone URL.
 *
 * 3. Shell out to git:
 *      std::process::Command::new("git")
 *          .args(["clone", &repo.remote_url])
 *          .status()?;
 *
 *    Inherit stdout/stderr so the user sees git's progress output live.
 *
 * 4. If git exits non-zero, return an error.
 *
 * Note: The user must have their PAT embedded in a credential helper or the
 * Windows Credential Manager for git to authenticate. Alternatively, print a
 * hint: "If prompted for a password, use your PAT token."
 */
async fn clone(args: CloneArgs) -> Result<()> {
    todo!("look up repo by name, then shell out to git clone <remoteUrl>")
}
