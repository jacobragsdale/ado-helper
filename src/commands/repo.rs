use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use crate::client::{AdoClient, encode_path_segment};
use crate::output::{self, OutputFormat};

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

    /// Clone a repository to the current directory (uses ADO_PAT for auth)
    Clone(CloneArgs),

    /// Delete a repository (permanent — there is no recycle bin)
    #[command(alias = "rm")]
    Delete(DeleteArgs),
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

    /// Destination directory (defaults to ./<name>)
    pub dest: Option<String>,

    /// Project the repo belongs to (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,

    /// Leave the PAT baked into the cloned remote URL (useful for CI). By
    /// default, the remote is rewritten to the credential-free URL after clone.
    #[arg(long)]
    pub keep_pat_in_remote: bool,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Name (or ID) of the repository to delete
    pub name: String,

    /// Required confirmation — this is permanent
    #[arg(long)]
    pub yes: bool,

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

// ── Command dispatch ────────────────────────────────────────────────────────

pub async fn run(args: RepoArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        RepoCommand::Create(a) => create(a, client, output).await,
        RepoCommand::List(a) => list(a, client, output).await,
        RepoCommand::Clone(a) => clone(a, client).await,
        RepoCommand::Delete(a) => delete(a, client).await,
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

// ── helpers ─────────────────────────────────────────────────────────────────

async fn lookup_repo(client: &AdoClient, project: &str, name_or_id: &str) -> Result<Repository> {
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

/// Take an ADO HTTPS clone URL and inject the PAT as `anything:<pat>@`.
/// Strips any existing `user@` prefix so we don't end up with two userinfo blocks.
fn inject_pat(remote_url: &str, pat: &str) -> Result<String> {
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
}
