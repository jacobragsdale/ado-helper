use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

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

    /// Open a work item in the browser
    Open(OpenArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Work item title
    #[arg(long)]
    pub title: String,

    /// Work item type (Bug, Task, User Story, Feature, Epic)
    #[arg(long, value_name = "TYPE")]
    pub r#type: String,

    /// Assign to this user (use "me" for yourself)
    #[arg(long)]
    pub assigned_to: Option<String>,

    /// Iteration path (e.g. "MyProject\\Sprint 1")
    #[arg(long)]
    pub iteration: Option<String>,

    /// Area path
    #[arg(long)]
    pub area: Option<String>,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by assigned user ("me" expands to current user's email)
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
    /// Work item ID
    pub id: u32,

    /// New state (Active, Closed, Resolved, etc.)
    #[arg(long)]
    pub state: Option<String>,

    /// Reassign to this user
    #[arg(long)]
    pub assigned_to: Option<String>,

    /// Update the title
    #[arg(long)]
    pub title: Option<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Work item ID
    pub id: u32,

    /// Project (defaults to configured project)
    #[arg(long)]
    pub project: Option<String>,
}

// ── ADO API response shapes ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: u32,
    pub url: String,
    pub fields: WorkItemFields,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkItemFields {
    #[serde(rename = "System.Title")]
    pub title: String,

    #[serde(rename = "System.State")]
    pub state: String,

    #[serde(rename = "System.WorkItemType")]
    pub work_item_type: String,

    #[serde(rename = "System.AssignedTo")]
    pub assigned_to: Option<serde_json::Value>,

    #[serde(rename = "System.IterationPath")]
    pub iteration_path: Option<String>,

    #[serde(rename = "System.AreaPath")]
    pub area_path: Option<String>,

    #[serde(rename = "System.TeamProject")]
    pub team_project: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WiqlResult {
    #[serde(rename = "workItems")]
    pub work_items: Vec<WiqlWorkItemRef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WiqlWorkItemRef {
    pub id: u32,
    pub url: String,
}

// A single JSON Patch operation — used for create and update
#[derive(Serialize)]
pub struct PatchOp {
    pub op: String,
    pub path: String,
    pub value: serde_json::Value,
}

// ── Command handlers ──────────────────────────────────────────────────────────

pub async fn run(args: WorkItemArgs) -> Result<()> {
    match args.command {
        WorkItemCommand::Create(a) => create(a).await,
        WorkItemCommand::List(a) => list(a).await,
        WorkItemCommand::View(a) => view(a).await,
        WorkItemCommand::Update(a) => update(a).await,
        WorkItemCommand::Open(a) => open(a).await,
    }
}

/*
 * IMPLEMENTATION NOTES — create()
 *
 * Endpoint: POST {org}/{project}/_apis/wit/workitems/${type}?api-version=7.1
 *   Note: The type appears URL-encoded in the path after a $ sign, e.g. $Bug, $Task,
 *   $User%20Story. Use percent-encoding for types with spaces.
 *
 * Content-Type: application/json-patch+json  (NOT application/json)
 *
 * Request body is a JSON Patch array of operations. Build it by pushing PatchOp
 * structs for each field. Required fields:
 *   [
 *     { "op": "add", "path": "/fields/System.Title", "value": "<title>" },
 *     { "op": "add", "path": "/fields/System.AssignedTo", "value": "<email>" },  // optional
 *     { "op": "add", "path": "/fields/System.IterationPath", "value": "<path>" }, // optional
 *     { "op": "add", "path": "/fields/System.AreaPath", "value": "<path>" }       // optional
 *   ]
 *
 * The "me" shorthand for --assigned-to:
 *   Resolve to the current user's email using:
 *   GET {org}/_apis/connectionData?api-version=5.0
 *   → authenticatedUser.providerDisplayName (or subjectDescriptor for the email)
 *   A simpler approach: set System.AssignedTo to the string "me" — ADO resolves it.
 *
 * On success, print: "Created #{id}: <title>  [{type}]"
 */
async fn create(args: CreateArgs) -> Result<()> {
    todo!("POST wit/workitems/$Type with JSON Patch body, Content-Type: application/json-patch+json")
}

/*
 * IMPLEMENTATION NOTES — list()
 *
 * Use the WIQL (Work Item Query Language) endpoint to filter work items:
 * POST {org}/{project}/_apis/wit/wiql?api-version=7.1
 * Body: { "query": "<WIQL string>" }
 *
 * Build the WIQL dynamically based on args:
 *   SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType],
 *          [System.AssignedTo]
 *   FROM WorkItems
 *   WHERE [System.TeamProject] = @project
 *   AND [System.State] <> 'Removed'
 *   [AND [System.AssignedTo] = @me]         // when --assigned-to me
 *   [AND [System.State] = '<state>']        // when --state is set
 *   [AND [System.WorkItemType] = '<type>']  // when --type is set
 *   [AND [System.IterationPath] UNDER '<iteration>']  // when --iteration is set
 *   ORDER BY [System.ChangedDate] DESC
 *
 * The WIQL response only returns IDs. Fetch full details in a second request:
 *   GET {org}/_apis/wit/workitems
 *     ?ids=1,2,3,...
 *     &fields=System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo
 *     &api-version=7.1
 * Batch IDs in groups of 200 (ADO limit per request).
 *
 * Plain text output per work item (one line):
 *   #{id}  [{type}]  [{state}]  <title>  (assigned: <displayName or "unassigned">)
 */
async fn list(args: ListArgs) -> Result<()> {
    todo!("POST WIQL query then GET batch work item details, print one line per item")
}

/*
 * IMPLEMENTATION NOTES — view()
 *
 * Endpoint: GET {org}/_apis/wit/workitems/{id}?$expand=all&api-version=7.1
 * The $expand=all flag includes relations and comments in the response.
 *
 * Plain text output (multi-line):
 *   #{id}: <title>
 *   Type:      <work_item_type>
 *   State:     <state>
 *   Assigned:  <assignedTo.displayName or "unassigned">
 *   Iteration: <iterationPath>
 *   Area:      <areaPath>
 *   URL:       https://dev.azure.com/{org}/{project}/_workitems/edit/{id}
 *
 * With --output json, print the full WorkItem object.
 */
async fn view(args: ViewArgs) -> Result<()> {
    todo!("GET work item by ID with $expand=all, print details")
}

/*
 * IMPLEMENTATION NOTES — update()
 *
 * Endpoint: PATCH {org}/_apis/wit/workitems/{id}?api-version=7.1
 * Content-Type: application/json-patch+json
 *
 * Build a Vec<PatchOp> from whichever flags were provided:
 *   --state      → { "op": "add", "path": "/fields/System.State", "value": "<state>" }
 *   --assigned-to → { "op": "add", "path": "/fields/System.AssignedTo", "value": "<user>" }
 *   --title      → { "op": "add", "path": "/fields/System.Title", "value": "<title>" }
 *
 * If no flags were provided, print "Nothing to update." and return Ok(()).
 *
 * Note: Use "op": "add" even for updates — ADO's JSON Patch treats "add" as upsert.
 *
 * On success, print: "Updated #{id}"
 */
async fn update(args: UpdateArgs) -> Result<()> {
    todo!("PATCH work item with JSON Patch ops for each provided flag")
}

/*
 * IMPLEMENTATION NOTES — open()
 *
 * Build the URL:
 *   https://dev.azure.com/{org-name}/{project}/_workitems/edit/{id}
 *
 * Then call client::AdoClient::open_in_browser(&url).
 * Print: "Opening work item #{id} in browser..."
 */
async fn open(args: OpenArgs) -> Result<()> {
    todo!("construct work item URL and open in browser via cmd /c start")
}
