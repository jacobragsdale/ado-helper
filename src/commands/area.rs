//! `ado area` — list or tree-print the project's area paths.
//!
//! Output strings are paste-ready into `ado wi create --area ...` /
//! `--field area=...` etc. Area path syntax in ADO uses `\` separators
//! (e.g. `MyProject\Backend\API`); we preserve that on the wire and in
//! `--output json`, and use the same form for `--output text`.

use anyhow::Result;
use clap::{Args, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::client::{AdoClient, encode_path_segment};
use crate::context::CmdCtx;
use crate::output::{self, OutputFormat};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado area tree\n  ado area tree --depth 3 --output json\n  ado area list\n  ado area list --output table\n\nOutput uses ADO's backslash-separated paths so they paste directly into --area / --field area=..."
)]
pub struct AreaArgs {
    #[command(subcommand)]
    pub command: AreaCommand,
}

#[derive(Subcommand)]
pub enum AreaCommand {
    /// Flatten the hierarchy to one path per line
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Render the hierarchy as an indented tree
    Tree(TreeArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Maximum tree depth to descend (default 5)
    #[arg(long, value_name = "N", default_value_t = 5)]
    pub depth: u32,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct TreeArgs {
    /// Maximum tree depth to descend (default 5)
    #[arg(long, value_name = "N", default_value_t = 5)]
    pub depth: u32,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

// ── ADO API response shape ──────────────────────────────────────────────────

/// One node in the area-path hierarchy. The shape mirrors ADO's
/// `ClassificationNode` response; `path` is the backslash-joined path
/// from project root.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AreaNode {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    /// Backslash-separated path from project root, e.g. `MyProject\Backend\API`.
    /// Comes from ADO's `path` field, which is prefixed with `\<project>\Area\…`;
    /// we normalize that here to drop the `\Area` segment.
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "hasChildren")]
    pub has_children: bool,
    #[serde(default)]
    pub children: Vec<AreaNode>,
    #[serde(default, rename = "structureType")]
    pub structure_type: Option<String>,
}

// ── Dispatch ────────────────────────────────────────────────────────────────

pub async fn run(args: AreaArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    match args.command {
        AreaCommand::List(a) => list(a, ctx).await,
        AreaCommand::Tree(a) => tree(a, ctx).await,
    }
}

async fn fetch(client: &AdoClient, project: &str, depth: u32) -> Result<AreaNode> {
    let path = format!(
        "{project}/_apis/wit/classificationnodes/areas?$depth={depth}&api-version=7.1",
        project = encode_path_segment(project)
    );
    let mut root: AreaNode = client.get_json(&path).await?;
    normalize_paths(&mut root);
    Ok(root)
}

/// Rewrite ADO's `\<project>\Area\…` paths to `<project>\…` so the strings
/// drop straight into `--area` and `--field area=...` without surprises.
fn normalize_paths(node: &mut AreaNode) {
    node.path = canonical_path(&node.path);
    for child in &mut node.children {
        normalize_paths(child);
    }
}

fn canonical_path(raw: &str) -> String {
    let trimmed = raw.trim_start_matches('\\');
    // ADO returns "<project>\Area\<sub...>"; drop the literal "Area" segment.
    let mut parts: Vec<&str> = trimmed.split('\\').collect();
    if parts.len() >= 2 && parts[1].eq_ignore_ascii_case("Area") {
        parts.remove(1);
    }
    parts.join("\\")
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let root = fetch(ctx.client, project, args.depth).await?;

    let mut paths: Vec<String> = Vec::new();
    collect_paths(&root, &mut paths);
    paths.sort();

    match ctx.output {
        OutputFormat::Json => output::print_json(&paths),
        OutputFormat::Text => {
            for p in &paths {
                println!("{p}");
            }
            Ok(())
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = paths.iter().map(|p| vec![p.clone()]).collect();
            output::print_table(&["Area Path"], &rows);
            Ok(())
        }
    }
}

fn collect_paths(node: &AreaNode, out: &mut Vec<String>) {
    if !node.path.is_empty() {
        out.push(node.path.clone());
    }
    for child in &node.children {
        collect_paths(child, out);
    }
}

// ── tree ────────────────────────────────────────────────────────────────────

async fn tree(args: TreeArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&ctx.client.project);
    let root = fetch(ctx.client, project, args.depth).await?;

    match ctx.output {
        OutputFormat::Json => output::print_json(&root),
        OutputFormat::Text | OutputFormat::Table => {
            render_tree(&root, 0);
            Ok(())
        }
    }
}

fn render_tree(node: &AreaNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let suffix = if !node.children.is_empty() {
        format!("  ({} children)", node.children.len())
    } else {
        String::new()
    };
    println!("{indent}{}{suffix}", node.name);
    for child in &node.children {
        render_tree(child, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_path_drops_area_segment() {
        assert_eq!(canonical_path("\\MyProject\\Area"), "MyProject");
        assert_eq!(
            canonical_path("\\MyProject\\Area\\Backend"),
            "MyProject\\Backend"
        );
        assert_eq!(
            canonical_path("\\MyProject\\Area\\Backend\\API"),
            "MyProject\\Backend\\API"
        );
    }

    #[test]
    fn canonical_path_leaves_non_area_alone() {
        assert_eq!(canonical_path("\\MyProject"), "MyProject");
        assert_eq!(canonical_path("MyProject\\Backend"), "MyProject\\Backend");
    }
}
