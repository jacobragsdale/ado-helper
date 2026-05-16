//! All clap-derived argument structs for `ado wi …` subcommands.

use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::flags::FieldFlags;

#[derive(Args)]
#[command(
    after_help = "Examples:\n  ado wi create --title \"Fix login redirect\" --type Bug --assigned-to me\n  ado wi list --assigned-to me --state Active\n  ado wi query --wiql \"SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project\"\n  ado wi view 123\n  ado wi update 123 --state Closed --field priority=2\n  ado wi link 123 --child 456\n  ado wi attach 123 ./screenshot.png\n\nWork item field aliases include title, state, assigned-to, tags, priority, severity, story-points, acceptance-criteria, repro-steps, and remaining-work."
)]
pub struct WorkItemArgs {
    #[command(subcommand)]
    pub command: WorkItemCommand,
}

#[derive(Subcommand)]
pub enum WorkItemCommand {
    /// Create a new work item
    #[command(
        after_help = "Examples:\n  ado wi create --title \"Fix login redirect\"\n  ado wi create --type Bug --title \"Crash on save\" --description \"<p>Steps...</p>\" --assigned-to me --priority 1\n  ado wi create --type User Story --title \"Improve onboarding\" --story-points 3 --acceptance-criteria \"<p>Given...</p>\"\n  ado wi create --title \"Custom field\" --field Microsoft.VSTS.Common.Activity=Development"
    )]
    Create(CreateArgs),

    /// List work items matching filters
    #[command(
        visible_alias = "ls",
        after_help = "Examples:\n  ado wi list --assigned-to me\n  ado wi ls --state Active --type Bug\n  ado wi list --search login --output table"
    )]
    List(ListArgs),

    /// Run a raw WIQL query and show matching work items
    #[command(
        after_help = "Examples:\n  ado wi query --wiql \"SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC\"\n  ado wi query --file bugs.wiql --output table\n\nPass exactly one query source: --wiql for inline WIQL or --file for a .wiql file."
    )]
    Query(QueryArgs),

    /// View details of a work item
    #[command(
        visible_alias = "show",
        after_help = "Examples:\n  ado wi view 123\n  ado wi show 123 --output json"
    )]
    View(ViewArgs),

    /// Update fields on a work item
    #[command(
        after_help = "Examples:\n  ado wi update 123 --state Closed\n  ado wi update 123 124 --assigned-to me --tags \"release;docs\"\n  ado wi update 123 --field priority=2 --field story-points=5\n\nPass multiple IDs to apply the same changes to each work item."
    )]
    Update(UpdateArgs),

    /// Delete a work item (soft-delete to recycle bin by default)
    #[command(
        visible_alias = "rm",
        after_help = "Examples:\n  ado wi delete 123\n  ado wi rm 123\n  ado wi delete 123 --destroy\n\nWithout --destroy, Azure DevOps moves the work item to the recycle bin."
    )]
    Delete(DeleteArgs),

    /// Add a comment to a work item
    Comment(CommentArgs),

    /// List comments on a work item
    Comments(CommentsArgs),

    /// Edit an existing comment
    CommentEdit(CommentEditArgs),

    /// Delete a comment
    CommentDelete(CommentDeleteArgs),

    /// Add a relation (parent/child/related/hyperlink) to a work item
    #[command(
        after_help = "Examples:\n  ado wi link 123 --parent 100\n  ado wi link 123 --child 456 --comment \"Split out implementation\"\n  ado wi link 123 --related 789\n  ado wi link 123 --hyperlink https://example.com/spec"
    )]
    Link(LinkArgs),

    /// List relations on a work item
    Links(LinksArgs),

    /// Remove a relation by index (see `wi links` for indices)
    LinkRm(LinkRmArgs),

    /// Upload a file and attach it to a work item
    #[command(
        after_help = "Examples:\n  ado wi attach 123 ./screenshot.png\n  ado wi attach 123 ./log.txt --comment \"Failure log from staging\""
    )]
    Attach(AttachArgs),

    /// Show revision history of a work item
    History(HistoryArgs),

    /// Open a work item in the browser
    #[command(
        visible_alias = "browse",
        after_help = "Examples:\n  ado wi open 123\n  ado wi browse 123"
    )]
    Open(OpenArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Work item type (Bug, Task, User Story, Issue, Feature, Epic, …)
    #[arg(long, value_name = "TYPE", default_value = "Task")]
    pub r#type: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,

    #[command(flatten)]
    pub fields: FieldFlags,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by assigned user ("me" expands to current user)
    #[arg(long, value_name = "USER")]
    pub assigned_to: Option<String>,

    /// Filter by state (Active, New, Closed, Resolved, etc.)
    #[arg(long, value_name = "STATE")]
    pub state: Option<String>,

    /// Filter by work item type
    #[arg(long, value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Filter by iteration path
    #[arg(long, value_name = "PATH")]
    pub iteration: Option<String>,

    /// Free-text search on title (WIQL CONTAINS)
    #[arg(long, value_name = "TERM")]
    pub search: Option<String>,

    /// Free-text search on description (WIQL CONTAINS on System.Description)
    #[arg(long, value_name = "TERM")]
    pub search_body: Option<String>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
#[command(group(clap::ArgGroup::new("source").required(true).multiple(false).args(["wiql", "file"])))]
pub struct QueryArgs {
    /// Inline WIQL query text
    #[arg(long, value_name = "WIQL")]
    pub wiql: Option<String>,

    /// Read WIQL query text from a file
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub file: Option<PathBuf>,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Work item ID(s) — pass multiple to apply the same field changes to each
    #[arg(required = true, num_args = 1..)]
    pub ids: Vec<u32>,

    #[command(flatten)]
    pub fields: FieldFlags,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Permanently destroy instead of moving to the recycle bin
    #[arg(long)]
    pub destroy: bool,
}

#[derive(Args)]
pub struct CommentArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Comment text (HTML allowed)
    #[arg(long, value_name = "HTML")]
    pub text: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentsArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentEditArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Comment ID (from `wi comments`)
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: u64,

    /// New comment text
    #[arg(long, value_name = "HTML")]
    pub text: String,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentDeleteArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Comment ID (from `wi comments`)
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: u64,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct LinkArgs {
    /// Work item ID (the source of the link)
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Link target work item as a parent
    #[arg(long, group = "link_kind", value_name = "ID")]
    pub parent: Option<u32>,

    /// Link target work item as a child
    #[arg(long, group = "link_kind", value_name = "ID")]
    pub child: Option<u32>,

    /// Link target work item as related
    #[arg(long, group = "link_kind", value_name = "ID")]
    pub related: Option<u32>,

    /// Link target work item as predecessor
    #[arg(long, group = "link_kind", value_name = "ID")]
    pub predecessor: Option<u32>,

    /// Link target work item as successor
    #[arg(long, group = "link_kind", value_name = "ID")]
    pub successor: Option<u32>,

    /// Add an external hyperlink (URL)
    #[arg(long, group = "link_kind", value_name = "URL", value_hint = clap::ValueHint::Url)]
    pub hyperlink: Option<String>,

    /// Optional comment for the link
    #[arg(long, value_name = "TEXT")]
    pub comment: Option<String>,
}

#[derive(Args)]
pub struct LinksArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,
}

#[derive(Args)]
pub struct LinkRmArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Index of the relation (from `wi links`)
    #[arg(long, value_name = "INDEX")]
    pub index: usize,
}

#[derive(Args)]
pub struct AttachArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// File path to upload
    #[arg(value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub file: String,

    /// Optional comment shown with the attachment
    #[arg(long, value_name = "TEXT")]
    pub comment: Option<String>,
}

#[derive(Args)]
pub struct HistoryArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Maximum revisions to show (default 20)
    #[arg(long, value_name = "N", default_value_t = 20)]
    pub limit: u32,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Work item ID
    #[arg(value_name = "ID")]
    pub id: u32,

    /// Project (defaults to configured project)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
}
