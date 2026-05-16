//! Command handlers for `ado wi …`. The dispatch entry is `dispatch()`.

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::client::AdoClient;
use crate::output::{self, OutputFormat};

const WI_LIST_HEADERS: &[&str] = &["ID", "Type", "State", "Title", "Assignee"];

use super::args::{
    AttachArgs, CommentArgs, CommentDeleteArgs, CommentEditArgs, CommentsArgs, CreateArgs,
    DeleteArgs, HistoryArgs, LinkArgs, LinkRmArgs, LinksArgs, ListArgs, OpenArgs, UpdateArgs,
    ViewArgs, WorkItemArgs, WorkItemCommand,
};
use super::flags::build_field_ops;
use super::helpers::{encode_path, escape_wiql, field_str, workitem_url};
use super::types::{AttachmentRef, PatchOp, WiqlResult, WorkItem, WorkItemBatch};

pub async fn dispatch(args: WorkItemArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    match args.command {
        WorkItemCommand::Create(a) => create(a, client, output).await,
        WorkItemCommand::List(a) => list(a, client, output).await,
        WorkItemCommand::View(a) => view(a, client, output).await,
        WorkItemCommand::Update(a) => update(a, client, output).await,
        WorkItemCommand::Delete(a) => delete(a, client).await,
        WorkItemCommand::Comment(a) => comment(a, client, output).await,
        WorkItemCommand::Comments(a) => comments(a, client, output).await,
        WorkItemCommand::CommentEdit(a) => comment_edit(a, client, output).await,
        WorkItemCommand::CommentDelete(a) => comment_delete(a, client).await,
        WorkItemCommand::Link(a) => link(a, client, output).await,
        WorkItemCommand::Links(a) => links(a, client, output).await,
        WorkItemCommand::LinkRm(a) => link_rm(a, client, output).await,
        WorkItemCommand::Attach(a) => attach(a, client, output).await,
        WorkItemCommand::History(a) => history(a, client, output).await,
        WorkItemCommand::Open(a) => open(a, client).await,
    }
}

// ── create ──────────────────────────────────────────────────────────────────

async fn create(args: CreateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let type_encoded = encode_path(&args.r#type);
    let path = format!(
        "{project}/_apis/wit/workitems/${type_encoded}?api-version=7.1",
        project = encode_path(project)
    );

    let title = args
        .fields
        .title
        .as_deref()
        .context("--title is required when creating a work item")?
        .to_string();

    let mut fields = args.fields;
    fields.title = Some(title);

    let ops = build_field_ops(&fields, client).await?;
    let wi: WorkItem = client.patch_json_patch(&path, &ops).await?;

    match output {
        OutputFormat::Json => output::print_json(&wi)?,
        OutputFormat::Text | OutputFormat::Table => {
            let title = field_str(&wi.fields, "System.Title").unwrap_or_default();
            println!("Created #{}: {title}  [{}]", wi.id, args.r#type);
        }
    }
    Ok(())
}

// ── list ────────────────────────────────────────────────────────────────────

async fn list(args: ListArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let wiql = build_list_wiql(&args);

    let wiql_path = format!(
        "{project}/_apis/wit/wiql?api-version=7.1",
        project = encode_path(project)
    );
    let result: WiqlResult = client
        .post_json(&wiql_path, &json!({ "query": wiql }))
        .await?;

    if result.work_items.is_empty() {
        if matches!(output, OutputFormat::Json) {
            output::print_json(&serde_json::json!([]))?;
        } else {
            println!("(no work items match)");
        }
        return Ok(());
    }

    let mut items: Vec<WorkItem> = Vec::with_capacity(result.work_items.len());
    let fields = "System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo";
    for chunk in result.work_items.chunks(200) {
        let ids = chunk
            .iter()
            .map(|w| w.id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let path = format!("_apis/wit/workitems?ids={ids}&fields={fields}&api-version=7.1");
        let batch: WorkItemBatch = client.get_json(&path).await?;
        items.extend(batch.value);
    }

    match output {
        OutputFormat::Json => output::print_json(&items)?,
        OutputFormat::Text => {
            let lines: Vec<String> = items
                .iter()
                .map(|w| {
                    let ty = field_str(&w.fields, "System.WorkItemType").unwrap_or("?");
                    let st = field_str(&w.fields, "System.State").unwrap_or("?");
                    let title = field_str(&w.fields, "System.Title").unwrap_or("");
                    let assignee = w
                        .fields
                        .get("System.AssignedTo")
                        .and_then(|v| v.get("displayName"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unassigned");
                    format!("#{:<5} [{ty}] [{st}] {title}  (assigned: {assignee})", w.id)
                })
                .collect();
            output::print_text(&lines);
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = items
                .iter()
                .map(|w| {
                    let ty = field_str(&w.fields, "System.WorkItemType").unwrap_or("?");
                    let st = field_str(&w.fields, "System.State").unwrap_or("?");
                    let title = field_str(&w.fields, "System.Title").unwrap_or("");
                    let assignee = w
                        .fields
                        .get("System.AssignedTo")
                        .and_then(|v| v.get("displayName"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unassigned");
                    vec![
                        format!("#{}", w.id),
                        ty.to_string(),
                        st.to_string(),
                        title.to_string(),
                        assignee.to_string(),
                    ]
                })
                .collect();
            output::print_table(WI_LIST_HEADERS, &rows);
        }
    }
    Ok(())
}

fn build_list_wiql(args: &ListArgs) -> String {
    let mut clauses: Vec<String> = vec![
        "[System.TeamProject] = @project".to_string(),
        "[System.State] <> 'Removed'".to_string(),
    ];

    if let Some(a) = args.assigned_to.as_deref() {
        if a.eq_ignore_ascii_case("me") {
            clauses.push("[System.AssignedTo] = @me".to_string());
        } else {
            clauses.push(format!("[System.AssignedTo] = '{}'", escape_wiql(a)));
        }
    }
    if let Some(s) = args.state.as_deref() {
        clauses.push(format!("[System.State] = '{}'", escape_wiql(s)));
    }
    if let Some(t) = args.r#type.as_deref() {
        clauses.push(format!("[System.WorkItemType] = '{}'", escape_wiql(t)));
    }
    if let Some(i) = args.iteration.as_deref() {
        clauses.push(format!("[System.IterationPath] UNDER '{}'", escape_wiql(i)));
    }
    if let Some(s) = args.search.as_deref() {
        clauses.push(format!("[System.Title] CONTAINS '{}'", escape_wiql(s)));
    }
    if let Some(s) = args.search_body.as_deref() {
        clauses.push(format!(
            "[System.Description] CONTAINS '{}'",
            escape_wiql(s)
        ));
    }

    format!(
        "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], \
                [System.AssignedTo] \
         FROM WorkItems \
         WHERE {} \
         ORDER BY [System.ChangedDate] DESC",
        clauses.join(" AND ")
    )
}

// ── view ────────────────────────────────────────────────────────────────────

async fn view(args: ViewArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let path = format!(
        "_apis/wit/workitems/{}?$expand=all&api-version=7.1",
        args.id
    );
    let wi: WorkItem = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&wi)?,
        OutputFormat::Text | OutputFormat::Table => {
            let title = field_str(&wi.fields, "System.Title").unwrap_or("");
            let ty = field_str(&wi.fields, "System.WorkItemType").unwrap_or("?");
            let state = field_str(&wi.fields, "System.State").unwrap_or("?");
            let assignee = wi
                .fields
                .get("System.AssignedTo")
                .and_then(|v| v.get("displayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("unassigned");
            let project = field_str(&wi.fields, "System.TeamProject").unwrap_or(&client.project);

            println!("#{}: {title}", wi.id);
            println!("Type:      {ty}");
            println!("State:     {state}");
            println!("Assigned:  {assignee}");
            print_optional(&wi.fields, "Iteration", "System.IterationPath");
            print_optional(&wi.fields, "Area", "System.AreaPath");
            print_optional(&wi.fields, "Tags", "System.Tags");
            print_optional(&wi.fields, "Priority", "Microsoft.VSTS.Common.Priority");
            print_optional(&wi.fields, "Severity", "Microsoft.VSTS.Common.Severity");
            print_optional(
                &wi.fields,
                "StoryPts",
                "Microsoft.VSTS.Scheduling.StoryPoints",
            );
            print_optional(&wi.fields, "Effort", "Microsoft.VSTS.Scheduling.Effort");
            print_optional(
                &wi.fields,
                "OrigEst",
                "Microsoft.VSTS.Scheduling.OriginalEstimate",
            );
            print_optional(
                &wi.fields,
                "Remaining",
                "Microsoft.VSTS.Scheduling.RemainingWork",
            );
            print_optional(
                &wi.fields,
                "Completed",
                "Microsoft.VSTS.Scheduling.CompletedWork",
            );
            print_optional(&wi.fields, "Activity", "Microsoft.VSTS.Common.Activity");
            print_optional(&wi.fields, "ValueArea", "Microsoft.VSTS.Common.ValueArea");
            print_optional(&wi.fields, "Risk", "Microsoft.VSTS.Common.Risk");
            print_optional(
                &wi.fields,
                "StartDate",
                "Microsoft.VSTS.Scheduling.StartDate",
            );
            print_optional(
                &wi.fields,
                "TargetDate",
                "Microsoft.VSTS.Scheduling.TargetDate",
            );
            print_optional(&wi.fields, "Reason", "System.Reason");
            if !wi.relations.is_empty() {
                println!(
                    "Relations: {} (use `wi links {}`)",
                    wi.relations.len(),
                    wi.id
                );
            }
            println!(
                "URL:       {}/{}/_workitems/edit/{}",
                client.org,
                encode_path(project),
                wi.id
            );
        }
    }
    Ok(())
}

fn print_optional(fields: &serde_json::Value, label: &str, key: &str) {
    if let Some(v) = fields.get(key) {
        let s = match v {
            serde_json::Value::String(s) if !s.is_empty() => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => return,
        };
        println!("{label:<10} {s}");
    }
}

// ── update ──────────────────────────────────────────────────────────────────

async fn update(args: UpdateArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let ops = build_field_ops(&args.fields, client).await?;

    if ops.is_empty() {
        println!("Nothing to update.");
        return Ok(());
    }

    // Per-ID continue-on-failure: one bad ID shouldn't block the rest. We collect
    // successes/failures and exit non-zero at the end if anything failed.
    let mut updated: Vec<WorkItem> = Vec::with_capacity(args.ids.len());
    let mut failures: Vec<(u32, anyhow::Error)> = Vec::new();
    for id in &args.ids {
        let path = format!("_apis/wit/workitems/{id}?api-version=7.1");
        match client.patch_json_patch::<_, WorkItem>(&path, &ops).await {
            Ok(wi) => updated.push(wi),
            Err(e) => failures.push((*id, e)),
        }
    }

    match output {
        OutputFormat::Json => output::print_json(&updated)?,
        OutputFormat::Text | OutputFormat::Table => {
            for wi in &updated {
                println!("Updated #{}", wi.id);
            }
            for (id, err) in &failures {
                eprintln!("Failed #{id}: {err}");
            }
        }
    }

    if !failures.is_empty() {
        bail!("{}/{} updates failed", failures.len(), args.ids.len());
    }
    Ok(())
}

// ── delete ──────────────────────────────────────────────────────────────────

async fn delete(args: DeleteArgs, client: &AdoClient) -> Result<()> {
    let path = if args.destroy {
        format!(
            "_apis/wit/workitems/{}?destroy=true&api-version=7.1",
            args.id
        )
    } else {
        format!("_apis/wit/workitems/{}?api-version=7.1", args.id)
    };
    client.delete_no_body(&path).await?;
    if args.destroy {
        println!("Permanently destroyed #{}", args.id);
    } else {
        println!("Deleted #{} (recycle bin)", args.id);
    }
    Ok(())
}

// ── comments ────────────────────────────────────────────────────────────────

async fn comment(args: CommentArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workItems/{}/comments?api-version=7.1-preview.4",
        args.id,
        project = encode_path(project)
    );
    let resp: serde_json::Value = client
        .post_json(&path, &json!({ "text": args.text }))
        .await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            let cid = resp.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("Added comment {cid} on #{}", args.id);
        }
    }
    Ok(())
}

async fn comments(args: CommentsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workItems/{}/comments?api-version=7.1-preview.4",
        args.id,
        project = encode_path(project)
    );
    let resp: serde_json::Value = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            let empty: Vec<serde_json::Value> = Vec::new();
            let list = resp
                .get("comments")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);
            if list.is_empty() {
                println!("(no comments on #{})", args.id);
                return Ok(());
            }
            for c in list {
                let cid = c.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let author = c
                    .get("createdBy")
                    .and_then(|v| v.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let date = c.get("createdDate").and_then(|v| v.as_str()).unwrap_or("");
                let text = c.get("text").and_then(|v| v.as_str()).unwrap_or("");
                println!("─ #{cid}  {author}  {date}");
                println!("  {text}");
            }
        }
    }
    Ok(())
}

async fn comment_edit(
    args: CommentEditArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workItems/{}/comments/{}?api-version=7.1-preview.4",
        args.id,
        args.comment_id,
        project = encode_path(project)
    );
    let v: serde_json::Value = client
        .patch_json(&path, &json!({ "text": args.text }))
        .await?;

    match output {
        OutputFormat::Json => output::print_json(&v)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Edited comment {} on #{}", args.comment_id, args.id)
        }
    }
    Ok(())
}

async fn comment_delete(args: CommentDeleteArgs, client: &AdoClient) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workItems/{}/comments/{}?api-version=7.1-preview.4",
        args.id,
        args.comment_id,
        project = encode_path(project)
    );
    client.delete_no_body(&path).await?;
    println!("Deleted comment {} on #{}", args.comment_id, args.id);
    Ok(())
}

// ── links ───────────────────────────────────────────────────────────────────

async fn link(args: LinkArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let (rel, url) = relation_target(&args, client)?;

    let mut value = json!({ "rel": rel, "url": url });
    if let Some(c) = args.comment {
        value["attributes"] = json!({ "comment": c });
    }

    let ops = vec![PatchOp {
        op: "add".into(),
        path: "/relations/-".into(),
        value,
    }];
    let path = format!("_apis/wit/workitems/{}?api-version=7.1", args.id);
    let wi: WorkItem = client.patch_json_patch(&path, &ops).await?;
    match output {
        OutputFormat::Json => output::print_json(&wi)?,
        OutputFormat::Text | OutputFormat::Table => println!("Added {rel} link to #{}", args.id),
    }
    Ok(())
}

fn relation_target(args: &LinkArgs, client: &AdoClient) -> Result<(&'static str, String)> {
    if let Some(target) = args.parent {
        Ok((
            "System.LinkTypes.Hierarchy-Reverse",
            workitem_url(client, target),
        ))
    } else if let Some(target) = args.child {
        Ok((
            "System.LinkTypes.Hierarchy-Forward",
            workitem_url(client, target),
        ))
    } else if let Some(target) = args.related {
        Ok(("System.LinkTypes.Related", workitem_url(client, target)))
    } else if let Some(target) = args.predecessor {
        Ok((
            "System.LinkTypes.Dependency-Reverse",
            workitem_url(client, target),
        ))
    } else if let Some(target) = args.successor {
        Ok((
            "System.LinkTypes.Dependency-Forward",
            workitem_url(client, target),
        ))
    } else if let Some(href) = args.hyperlink.as_ref() {
        Ok(("Hyperlink", href.clone()))
    } else {
        bail!(
            "link requires one of: --parent, --child, --related, --predecessor, --successor, --hyperlink"
        )
    }
}

async fn links(args: LinksArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let path = format!(
        "_apis/wit/workitems/{}?$expand=relations&api-version=7.1",
        args.id
    );
    let wi: WorkItem = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&wi.relations)?,
        OutputFormat::Text | OutputFormat::Table => {
            if wi.relations.is_empty() {
                println!("(no relations on #{})", args.id);
                return Ok(());
            }
            for (i, r) in wi.relations.iter().enumerate() {
                let comment = r
                    .attributes
                    .get("comment")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let name = r
                    .attributes
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let extra = if !comment.is_empty() {
                    format!("  // {comment}")
                } else if !name.is_empty() {
                    format!("  ({name})")
                } else {
                    String::new()
                };
                println!("[{i:>2}] {}  {}{}", r.rel, r.url, extra);
            }
        }
    }
    Ok(())
}

async fn link_rm(args: LinkRmArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let path = format!("_apis/wit/workitems/{}?api-version=7.1", args.id);
    let ops = vec![PatchOp {
        op: "remove".into(),
        path: format!("/relations/{}", args.index),
        value: serde_json::Value::Null,
    }];
    let wi: WorkItem = client.patch_json_patch(&path, &ops).await?;
    match output {
        OutputFormat::Json => output::print_json(&wi)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Removed relation [{}] from #{}", args.index, args.id);
        }
    }
    Ok(())
}

// ── attachments ─────────────────────────────────────────────────────────────

async fn attach(args: AttachArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let bytes = std::fs::read(&args.file).with_context(|| format!("reading {}", args.file))?;
    let filename = std::path::Path::new(&args.file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment");

    // 1. Upload the file body to get an attachment URL.
    let upload_path = format!(
        "_apis/wit/attachments?fileName={}&api-version=7.1",
        encode_path(filename)
    );
    let resp = client
        .post(&upload_path)
        .header("Content-Type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    let resp = AdoClient::check_response(resp).await?;
    let attachment: AttachmentRef = resp.json().await?;

    // 2. PATCH the work item to add an AttachedFile relation pointing at it.
    let mut value = json!({
        "rel": "AttachedFile",
        "url": attachment.url,
        "attributes": { "name": filename }
    });
    if let Some(c) = args.comment {
        value["attributes"]["comment"] = json!(c);
    }
    let ops = vec![PatchOp {
        op: "add".into(),
        path: "/relations/-".into(),
        value,
    }];
    let path = format!("_apis/wit/workitems/{}?api-version=7.1", args.id);
    let wi: WorkItem = client.patch_json_patch(&path, &ops).await?;
    match output {
        OutputFormat::Json => output::print_json(&json!({
            "attachment": attachment,
            "workItem": wi,
        }))?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Attached {filename} to #{} (id {})", args.id, attachment.id);
        }
    }
    Ok(())
}

// ── history ─────────────────────────────────────────────────────────────────

async fn history(args: HistoryArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let path = format!(
        "_apis/wit/workItems/{}/updates?$top={}&api-version=7.1",
        args.id, args.limit
    );
    let resp: serde_json::Value = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            let empty: Vec<serde_json::Value> = Vec::new();
            let updates = resp
                .get("value")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);
            if updates.is_empty() {
                println!("(no history)");
                return Ok(());
            }
            for u in updates {
                let rev = u.get("rev").and_then(|v| v.as_u64()).unwrap_or(0);
                let by = u
                    .get("revisedBy")
                    .and_then(|v| v.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let date = u.get("revisedDate").and_then(|v| v.as_str()).unwrap_or("");
                println!("rev {rev}  {by}  {date}");
                if let Some(fields) = u.get("fields").and_then(|v| v.as_object()) {
                    for (name, change) in fields {
                        let old = change.get("oldValue").map(short_val).unwrap_or_default();
                        let new = change.get("newValue").map(short_val).unwrap_or_default();
                        println!("  {name}: {old} → {new}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn short_val(v: &serde_json::Value) -> String {
    let s = match v {
        serde_json::Value::Null => return "(none)".to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    truncate_chars(&s, 80)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

// ── open ────────────────────────────────────────────────────────────────────

async fn open(args: OpenArgs, client: &AdoClient) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let url = format!(
        "{}/{}/_workitems/edit/{}",
        client.org,
        encode_path(project),
        args.id
    );
    println!("Opening work item #{} in browser...", args.id);
    AdoClient::open_in_browser(&url)
}
