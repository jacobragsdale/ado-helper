//! All clap-derived argument structs for `ado wi …` subcommands.

use clap::{Args, Subcommand};

use super::flags::FieldFlags;

#[derive(Args)]
pub struct WorkItemArgs {
    #[command(subcommand)]
    pub command: WorkItemCommand,
}

#[derive(Subcommand)]
pub enum WorkItemCommand {
    /// Create a new work item
    Create(CreateArgs),

    /// List work items matching filters
    List(ListArgs),

    /// View details of a work item
    View(ViewArgs),

    /// Update fields on a work item
    Update(UpdateArgs),

    /// Delete a work item (soft-delete to recycle bin by default)
    #[command(alias = "rm")]
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
    Link(LinkArgs),

    /// List relations on a work item
    Links(LinksArgs),

    /// Remove a relation by index (see `wi links` for indices)
    LinkRm(LinkRmArgs),

    /// Upload a file and attach it to a work item
    Attach(AttachArgs),

    /// Show revision history of a work item
    History(HistoryArgs),

    /// Open a work item in the browser
    Open(OpenArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Work item type (Bug, Task, User Story, Issue, Feature, Epic, …)
    #[arg(long, value_name = "TYPE", default_value = "Task")]
    pub r#type: String,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,

    #[command(flatten)]
    pub fields: FieldFlags,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by assigned user ("me" expands to current user)
    #[arg(long)]
    pub assigned_to: Option<String>,

    /// Filter by state (Active, New, Closed, Resolved, etc.)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by work item type
    #[arg(long, value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Filter by iteration path
    #[arg(long)]
    pub iteration: Option<String>,

    /// Free-text search on title (WIQL CONTAINS)
    #[arg(long, value_name = "TERM")]
    pub search: Option<String>,

    /// Free-text search on description (WIQL CONTAINS on System.Description)
    #[arg(long, value_name = "TERM")]
    pub search_body: Option<String>,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Work item ID
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
    pub id: u32,

    /// Permanently destroy instead of moving to the recycle bin
    #[arg(long)]
    pub destroy: bool,
}

#[derive(Args)]
pub struct CommentArgs {
    /// Work item ID
    pub id: u32,

    /// Comment text (HTML allowed)
    #[arg(long)]
    pub text: String,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentsArgs {
    /// Work item ID
    pub id: u32,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentEditArgs {
    /// Work item ID
    pub id: u32,

    /// Comment ID (from `wi comments`)
    pub comment_id: u64,

    /// New comment text
    #[arg(long)]
    pub text: String,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct CommentDeleteArgs {
    /// Work item ID
    pub id: u32,

    /// Comment ID (from `wi comments`)
    pub comment_id: u64,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct LinkArgs {
    /// Work item ID (the source of the link)
    pub id: u32,

    /// Link target work item as a parent
    #[arg(long, group = "link_kind")]
    pub parent: Option<u32>,

    /// Link target work item as a child
    #[arg(long, group = "link_kind")]
    pub child: Option<u32>,

    /// Link target work item as related
    #[arg(long, group = "link_kind")]
    pub related: Option<u32>,

    /// Link target work item as predecessor
    #[arg(long, group = "link_kind")]
    pub predecessor: Option<u32>,

    /// Link target work item as successor
    #[arg(long, group = "link_kind")]
    pub successor: Option<u32>,

    /// Add an external hyperlink (URL)
    #[arg(long, group = "link_kind")]
    pub hyperlink: Option<String>,

    /// Optional comment for the link
    #[arg(long)]
    pub comment: Option<String>,
}

#[derive(Args)]
pub struct LinksArgs {
    /// Work item ID
    pub id: u32,
}

#[derive(Args)]
pub struct LinkRmArgs {
    /// Work item ID
    pub id: u32,

    /// Index of the relation (from `wi links`)
    #[arg(long)]
    pub index: usize,
}

#[derive(Args)]
pub struct AttachArgs {
    /// Work item ID
    pub id: u32,

    /// File path to upload
    pub file: String,

    /// Optional comment shown with the attachment
    #[arg(long)]
    pub comment: Option<String>,
}

#[derive(Args)]
pub struct HistoryArgs {
    /// Work item ID
    pub id: u32,

    /// Maximum revisions to show (default 20)
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Work item ID
    pub id: u32,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}
