use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use crate::client::{AdoClient, encode_path_segment};
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado repo list --output table\n  ado repo branches --repo my-service\n  ado repo tags --repo my-service\n  ado repo commits --repo my-service --branch main --max 10\n  ado repo create --name my-service\n  ado repo clone my-service ./my-service\n  ado repo delete old-service --yes\n\nClone uses ADO_PAT for authentication and rewrites origin to the credential-free URL unless --keep-pat-in-remote is passed."
)]
pub struct RepoArgs {
    #[command(subcommand)]
    pub command: RepoCommand,
}

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Create a new Git repository in the project
    #[command(
        after_help = "Examples:\n  ado repo create --name my-service\n  ado repo create --name my-service --project OtherProject"
    )]
    Create(CreateArgs),

    /// List all repositories in the project
    #[command(
        visible_alias = "ls",
        after_help = "Examples:\n  ado repo list\n  ado repo ls --output table\n  ado repo list --project OtherProject --output json"
    )]
    List(ListArgs),

    /// Clone a repository to the current directory (uses ADO_PAT for auth)
    #[command(
        after_help = "Examples:\n  ado repo clone my-service\n  ado repo clone my-service ../work/my-service\n  ado repo clone my-service --keep-pat-in-remote\n\nBy default, origin is rewritten after clone so the PAT is not left in .git/config."
    )]
    Clone(CloneArgs),

    /// Delete a repository (permanent — there is no recycle bin)
    #[command(
        visible_alias = "rm",
        after_help = "Examples:\n  ado repo delete old-service --yes\n  ado repo rm old-service --yes\n\nThis permanently deletes the repository in Azure DevOps."
    )]
    Delete(DeleteArgs),

    /// List branches in a repository
    #[command(
        after_help = "Examples:\n  ado repo branches --repo my-service\n  ado repo branches --filter feature/ --max 25 --output table\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
    )]
    Branches(RefsArgs),

    /// List tags in a repository
    #[command(
        after_help = "Examples:\n  ado repo tags --repo my-service\n  ado repo tags --filter v1. --max 25 --output table\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
    )]
    Tags(RefsArgs),

    /// List recent commits in a repository
    #[command(
        after_help = "Examples:\n  ado repo commits --repo my-service\n  ado repo commits --branch main --max 10 --output table\n  ado repo commits --author alice@example.com --from 2026-05-01 --to 2026-05-15\n\nWhen --repo is omitted, ado uses ADO_REPO or the current git origin remote."
    )]
    Commits(CommitsArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Name of the new repository
    #[arg(long, value_name = "NAME")]
    pub name: String,

    /// Project to create the repo in (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Default branch name
    #[arg(long, value_name = "BRANCH", default_value = "main")]
    pub default_branch: String,
}

#[derive(Args)]
pub struct ListArgs {
    /// Project to list repos in (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CloneArgs {
    /// Name of the repository to clone
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Destination directory (defaults to ./<name>)
    #[arg(value_name = "DEST", value_hint = clap::ValueHint::DirPath)]
    pub dest: Option<String>,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Leave the PAT baked into the cloned remote URL (useful for CI). By
    /// default, the remote is rewritten to the credential-free URL after clone.
    #[arg(long)]
    pub keep_pat_in_remote: bool,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Name (or ID) of the repository to delete
    #[arg(value_name = "NAME_OR_ID")]
    pub name: String,

    /// Required confirmation — this is permanent
    #[arg(long)]
    pub yes: bool,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct RefsArgs {
    /// Repository name or ID (defaults to ADO_REPO or current git origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Ref-name prefix to filter by, relative to branches or tags
    #[arg(long, value_name = "PREFIX")]
    pub filter: Option<String>,

    /// Maximum number of refs to fetch
    #[arg(long, value_name = "N", default_value_t = 50)]
    pub max: usize,
}

#[derive(Args)]
pub struct CommitsArgs {
    /// Repository name or ID (defaults to ADO_REPO or current git origin remote)
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Branch or ref to list commits from
    #[arg(long, value_name = "BRANCH_OR_REF")]
    pub branch: Option<String>,

    /// Filter by author display name or email
    #[arg(long, value_name = "AUTHOR")]
    pub author: Option<String>,

    /// Start date for commit search (ISO date or date-time)
    #[arg(long, value_name = "DATE")]
    pub from: Option<String>,

    /// End date for commit search (ISO date or date-time)
    #[arg(long, value_name = "DATE")]
    pub to: Option<String>,

    /// Maximum number of commits to fetch
    #[arg(long, value_name = "N", default_value_t = 20)]
    pub max: usize,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name: String,

    #[serde(rename = "remoteUrl")]
    pub remote_url: String,

    #[serde(default, rename = "defaultBranch")]
    pub default_branch: Option<String>,

    #[serde(default, rename = "webUrl")]
    pub web_url: String,

    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoListResponse {
    pub value: Vec<Repository>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitRefListResponse {
    pub value: Vec<GitRef>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitRef {
    pub name: String,

    #[serde(rename = "objectId")]
    pub object_id: String,

    #[serde(default, rename = "peeledObjectId")]
    pub peeled_object_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitCommitListResponse {
    pub value: Vec<GitCommit>,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitCommit {
    #[serde(rename = "commitId")]
    pub commit_id: String,

    #[serde(default)]
    pub author: Option<GitUserDate>,

    #[serde(default)]
    pub committer: Option<GitUserDate>,

    #[serde(default)]
    pub comment: String,

    #[serde(default, rename = "remoteUrl")]
    pub remote_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitUserDate {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub email: String,

    #[serde(default)]
    pub date: String,
}

// ── Command dispatch ────────────────────────────────────────────────────────

pub async fn run(args: RepoArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        RepoCommand::Create(a) => create(a, client, output).await,
        RepoCommand::List(a) => list(a, client, output).await,
        RepoCommand::Clone(a) => clone(a, client).await,
        RepoCommand::Delete(a) => delete(a, client).await,
        RepoCommand::Branches(a) => branches(a, client, output).await,
        RepoCommand::Tags(a) => tags(a, client, output).await,
        RepoCommand::Commits(a) => commits(a, client, output).await,
    }
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/git/repositories?api-version=7.1",
        project = encode_path_segment(project)
    );
    let mut resp: RepoListResponse = client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no repos in {project})");
                return Ok(());
            }
            for line in repo_list_lines(&resp.value) {
                println!("{line}");
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no repos in {project})");
                return Ok(());
            }
            let rows = repo_table_rows(&resp.value);
            output::print_table(&["Name", "Remote"], &rows);
        }
    }
    Ok(())
}

// ── create ─────────────────────────────────────────────────────────────────

async fn create(args: CreateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/git/repositories?api-version=7.1",
        project = encode_path_segment(project)
    );
    // The project is already scoped by the URI; including it in the body with
    // a name (rather than UUID) makes ADO reject the request. defaultBranch is
    // also omitted — an empty repo has no refs to set, and `git init` on the
    // first push will record HEAD.
    let _ = &args.default_branch; // kept on the CLI for future use
    let body = json!({ "name": args.name });
    let repo: Repository = client.post_json(&path, &body).await?;
    match output {
        OutputFormat::Json => output::print_json(&repo)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Created repo {}\n  clone: {}", repo.name, repo.remote_url)
        }
    }
    Ok(())
}

// ── clone ───────────────────────────────────────────────────────────────────

async fn clone(args: CloneArgs, client: &AdoClient) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let repo = lookup_repo(client, project, &args.name).await?;

    let dest = args.dest.unwrap_or_else(|| repo.name.clone());
    let auth_url = inject_pat(&repo.remote_url, client.pat())?;

    println!("Cloning {} into {dest}...", repo.name);
    let status = Command::new("git")
        .args(["clone", &auth_url, &dest])
        .status()
        .context("failed to invoke `git clone` — is git installed and on PATH?")?;
    if !status.success() {
        bail!("git clone exited with status {status}");
    }

    if !args.keep_pat_in_remote {
        // Rewrite origin to the credential-free URL so the PAT doesn't leak
        // through `git remote -v` or `.git/config`.
        let st = Command::new("git")
            .args(["-C", &dest, "remote", "set-url", "origin", &repo.remote_url])
            .status()
            .context("failed to rewrite origin URL after clone")?;
        if !st.success() {
            eprintln!(
                "warning: could not rewrite origin URL — PAT may remain in {dest}/.git/config"
            );
        }
    }
    println!("Done.");
    Ok(())
}

// ── delete ──────────────────────────────────────────────────────────────────

async fn delete(args: DeleteArgs, client: &AdoClient) -> Result<()> {
    if !args.yes {
        bail!("repo deletion is permanent — pass --yes to confirm");
    }
    let project = args.project.as_deref().unwrap_or(&client.project);
    let repo = lookup_repo(client, project, &args.name).await?;
    let path = format!(
        "{project}/_apis/git/repositories/{}?api-version=7.1",
        encode_path_segment(&repo.id),
        project = encode_path_segment(project)
    );
    client.delete_no_body(&path).await?;
    println!("Deleted repo {} ({})", repo.name, repo.id);
    Ok(())
}

// ── branches / tags ──────────────────────────────────────────────────────────

async fn branches(args: RefsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let filter = ref_filter("heads", args.filter.as_deref());
    let mut resp = fetch_refs(client, project, &repo, &filter, false, args.max).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no branches in {repo})");
                return Ok(());
            }
            output::print_text(&ref_list_lines(&resp.value, false));
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no branches in {repo})");
                return Ok(());
            }
            output::print_table(&["Name", "Object ID"], &ref_table_rows(&resp.value, false));
        }
    }
    Ok(())
}

async fn tags(args: RefsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let filter = ref_filter("tags", args.filter.as_deref());
    let mut resp = fetch_refs(client, project, &repo, &filter, true, args.max).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no tags in {repo})");
                return Ok(());
            }
            output::print_text(&ref_list_lines(&resp.value, true));
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no tags in {repo})");
                return Ok(());
            }
            output::print_table(
                &["Name", "Object ID", "Peeled Object ID"],
                &ref_table_rows(&resp.value, true),
            );
        }
    }
    Ok(())
}

// ── commits ──────────────────────────────────────────────────────────────────

async fn commits(args: CommitsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let repo = resolve_repo_required(args.repo.as_deref())?;
    let mut resp = fetch_commits(client, project, &repo, &args).await?;

    if resp.value.len() > args.max {
        resp.value.truncate(args.max);
        resp.count = resp.value.len() as u32;
    }

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no commits in {repo})");
                return Ok(());
            }
            output::print_text(&commit_list_lines(&resp.value));
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no commits in {repo})");
                return Ok(());
            }
            output::print_table(
                &["Commit", "Author", "Date", "Comment"],
                &commit_table_rows(&resp.value),
            );
        }
    }
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────────────

pub(crate) async fn lookup_repo(
    client: &AdoClient,
    project: &str,
    name_or_id: &str,
) -> Result<Repository> {
    let path = format!(
        "{project}/_apis/git/repositories/{repo}?api-version=7.1",
        project = encode_path_segment(project),
        repo = encode_path_segment(name_or_id)
    );
    client
        .get_json(&path)
        .await
        .with_context(|| format!("could not find repo '{name_or_id}' in project '{project}'"))
}

async fn fetch_refs(
    client: &AdoClient,
    project: &str,
    repo: &str,
    filter: &str,
    peel_tags: bool,
    max: usize,
) -> Result<GitRefListResponse> {
    if max == 0 {
        bail!("--max must be greater than zero");
    }
    let path = format!(
        "{project}/_apis/git/repositories/{repo}/refs?filter={filter}&peelTags={peel_tags}&$top={max}&api-version=7.1",
        project = encode_path_segment(project),
        repo = encode_path_segment(repo),
        filter = encode_path_segment(filter)
    );
    client.get_json(&path).await
}

async fn fetch_commits(
    client: &AdoClient,
    project: &str,
    repo: &str,
    args: &CommitsArgs,
) -> Result<GitCommitListResponse> {
    if args.max == 0 {
        bail!("--max must be greater than zero");
    }

    let mut query = vec![format!("searchCriteria.$top={}", args.max)];
    if let Some(branch) = args.branch.as_deref() {
        append_version_query(&mut query, branch);
    }
    if let Some(author) = args.author.as_deref() {
        query.push(format!(
            "searchCriteria.author={}",
            encode_path_segment(author)
        ));
    }
    if let Some(from) = args.from.as_deref() {
        query.push(format!(
            "searchCriteria.fromDate={}",
            encode_path_segment(from)
        ));
    }
    if let Some(to) = args.to.as_deref() {
        query.push(format!("searchCriteria.toDate={}", encode_path_segment(to)));
    }
    query.push("api-version=7.1".to_string());

    let path = format!(
        "{project}/_apis/git/repositories/{repo}/commits?{}",
        query.join("&"),
        project = encode_path_segment(project),
        repo = encode_path_segment(repo)
    );
    client.get_json(&path).await
}

fn ref_filter(kind: &str, filter: Option<&str>) -> String {
    let prefix = filter.unwrap_or("").trim().trim_start_matches('/');
    if prefix.is_empty() {
        format!("{kind}/")
    } else {
        let prefix = prefix
            .strip_prefix("refs/")
            .and_then(|s| s.strip_prefix(&format!("{kind}/")))
            .unwrap_or(prefix)
            .trim_start_matches('/');
        format!("{kind}/{prefix}")
    }
}

fn append_version_query(query: &mut Vec<String>, input: &str) {
    let (version_type, version) = commit_version(input);
    query.push(format!(
        "searchCriteria.itemVersion.versionType={}",
        encode_path_segment(version_type)
    ));
    query.push(format!(
        "searchCriteria.itemVersion.version={}",
        encode_path_segment(&version)
    ));
}

fn commit_version(input: &str) -> (&'static str, String) {
    let trimmed = input.trim();
    if let Some(branch) = trimmed.strip_prefix("refs/heads/") {
        ("branch", branch.to_string())
    } else if let Some(branch) = trimmed.strip_prefix("heads/") {
        ("branch", branch.to_string())
    } else if let Some(tag) = trimmed.strip_prefix("refs/tags/") {
        ("tag", tag.to_string())
    } else if let Some(tag) = trimmed.strip_prefix("tags/") {
        ("tag", tag.to_string())
    } else if is_full_sha(trimmed) {
        ("commit", trimmed.to_string())
    } else {
        ("branch", trimmed.to_string())
    }
}

fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn repo_list_lines(repos: &[Repository]) -> Vec<String> {
    let width = repos.iter().map(|r| r.name.len()).max().unwrap_or(0);
    repos
        .iter()
        .map(|r| format!("{:width$}  {}", r.name, r.remote_url, width = width))
        .collect()
}

fn repo_table_rows(repos: &[Repository]) -> Vec<Vec<String>> {
    repos
        .iter()
        .map(|r| vec![r.name.clone(), r.remote_url.clone()])
        .collect()
}

fn ref_list_lines(refs: &[GitRef], include_peeled: bool) -> Vec<String> {
    refs.iter()
        .map(|r| {
            if include_peeled {
                let peeled = r.peeled_object_id.as_deref().unwrap_or("-");
                format!(
                    "{}  {}  peeled: {}",
                    short_ref_name(&r.name),
                    r.object_id,
                    peeled
                )
            } else {
                format!("{}  {}", short_ref_name(&r.name), r.object_id)
            }
        })
        .collect()
}

fn ref_table_rows(refs: &[GitRef], include_peeled: bool) -> Vec<Vec<String>> {
    refs.iter()
        .map(|r| {
            let mut row = vec![short_ref_name(&r.name), r.object_id.clone()];
            if include_peeled {
                row.push(
                    r.peeled_object_id
                        .clone()
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
            row
        })
        .collect()
}

fn commit_list_lines(commits: &[GitCommit]) -> Vec<String> {
    commits
        .iter()
        .map(|c| {
            format!(
                "{}  {}  {}  {}",
                short_sha(&c.commit_id),
                commit_date(c).unwrap_or("?"),
                commit_author(c).unwrap_or("?"),
                first_comment_line(&c.comment)
            )
        })
        .collect()
}

fn commit_table_rows(commits: &[GitCommit]) -> Vec<Vec<String>> {
    commits
        .iter()
        .map(|c| {
            vec![
                c.commit_id.clone(),
                commit_author(c).unwrap_or("?").to_string(),
                commit_date(c).unwrap_or("?").to_string(),
                first_comment_line(&c.comment),
            ]
        })
        .collect()
}

fn short_ref_name(name: &str) -> String {
    name.strip_prefix("refs/heads/")
        .or_else(|| name.strip_prefix("refs/tags/"))
        .unwrap_or(name)
        .to_string()
}

fn short_sha(commit_id: &str) -> &str {
    commit_id.get(..12).unwrap_or(commit_id)
}

fn commit_author(commit: &GitCommit) -> Option<&str> {
    commit
        .author
        .as_ref()
        .or(commit.committer.as_ref())
        .map(|a| a.name.as_str())
        .filter(|s| !s.is_empty())
}

fn commit_date(commit: &GitCommit) -> Option<&str> {
    commit
        .author
        .as_ref()
        .or(commit.committer.as_ref())
        .map(|a| a.date.as_str())
        .filter(|s| !s.is_empty())
}

fn first_comment_line(comment: &str) -> String {
    comment.lines().next().unwrap_or("").trim().to_string()
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

/// Take an ADO HTTPS clone URL and inject the PAT as `anything:<pat>@`.
/// Strips any existing `user@` prefix so we don't end up with two userinfo blocks.
pub(crate) fn inject_pat(remote_url: &str, pat: &str) -> Result<String> {
    let after_scheme = remote_url
        .strip_prefix("https://")
        .with_context(|| format!("repo clone URL is not HTTPS: {remote_url}"))?;
    let host_path = after_scheme
        .split_once('@')
        .map(|(_, hp)| hp)
        .unwrap_or(after_scheme);
    Ok(format!("https://anything:{pat}@{host_path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_pat_replaces_existing_userinfo() {
        let url = "https://jacobragsdale@dev.azure.com/jacobragsdale/development/_git/development";
        let injected = inject_pat(url, "TOKEN").unwrap();
        assert_eq!(
            injected,
            "https://anything:TOKEN@dev.azure.com/jacobragsdale/development/_git/development"
        );
    }

    #[test]
    fn inject_pat_handles_no_userinfo() {
        let url = "https://dev.azure.com/jacobragsdale/development/_git/development";
        let injected = inject_pat(url, "TOKEN").unwrap();
        assert_eq!(
            injected,
            "https://anything:TOKEN@dev.azure.com/jacobragsdale/development/_git/development"
        );
    }

    #[test]
    fn ref_filter_accepts_plain_or_full_ref_prefix() {
        assert_eq!(ref_filter("heads", None), "heads/");
        assert_eq!(ref_filter("heads", Some("feature/")), "heads/feature/");
        assert_eq!(
            ref_filter("heads", Some("refs/heads/feature/login")),
            "heads/feature/login"
        );
        assert_eq!(ref_filter("tags", Some("refs/tags/v1.")), "tags/v1.");
    }

    #[test]
    fn commit_version_detects_branches_tags_and_commits() {
        assert_eq!(commit_version("main"), ("branch", "main".to_string()));
        assert_eq!(
            commit_version("refs/heads/feature/login"),
            ("branch", "feature/login".to_string())
        );
        assert_eq!(
            commit_version("refs/tags/v1.0.0"),
            ("tag", "v1.0.0".to_string())
        );
        assert_eq!(
            commit_version("0123456789abcdef0123456789abcdef01234567"),
            (
                "commit",
                "0123456789abcdef0123456789abcdef01234567".to_string()
            )
        );
    }

    #[test]
    fn repo_name_from_remote_url_handles_ado_and_ssh_urls() {
        assert_eq!(
            repo_name_from_remote_url("https://dev.azure.com/org/project/_git/service"),
            Some("service".to_string())
        );
        assert_eq!(
            repo_name_from_remote_url("git@ssh.dev.azure.com:v3/org/project/service.git"),
            Some("service".to_string())
        );
    }
}
