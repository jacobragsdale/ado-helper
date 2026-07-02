//! Command handlers for `ado wi …`. The dispatch entry is `dispatch()`.

use anyhow::{Context, Result, bail};
use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::client::AdoClient;
use crate::context::CmdCtx;
use crate::error::CliError;
use crate::output::{self, OutputFormat};

const WI_LIST_HEADERS: &[&str] = &["ID", "Type", "State", "Title", "Assignee"];
const WI_ATTACHMENT_HEADERS: &[&str] = &["Index", "Name", "Size", "ID", "Comment", "Relation"];

use super::api;
use super::args::{
    AttachArgs, AttachmentDownloadArgs, AttachmentsArgs, CommentArgs, CommentDeleteArgs,
    CommentEditArgs, CommentsArgs, CreateArgs, DeleteArgs, FieldsArgs, HistoryArgs, LinkArgs,
    LinkRmArgs, LinksArgs, ListArgs, OpenArgs, QueryArgs, StatesArgs, TypesArgs, UpdateArgs,
    ViewArgs, WorkItemArgs, WorkItemCommand,
};
use super::flags::build_field_ops;
use super::helpers::{encode_path, escape_wiql, field_str, workitem_url};
use super::types::{
    AttachResult, AttachmentRef, FieldListResponse, PatchOp, Relation, StateListResponse,
    WiAttachment, WiAttachmentDownloadResult, WiAttachmentDownloadedFile, WiAttachmentList,
    WiComment, WiCommentList, WorkItem, WorkItemTypeListResponse,
};

pub async fn dispatch(args: WorkItemArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    let client = ctx.client;
    let output = &ctx.output;
    match args.command {
        WorkItemCommand::Create(a) => create(a, client, output).await,
        WorkItemCommand::List(a) => list(a, client, output).await,
        WorkItemCommand::Query(a) => query(a, client, output).await,
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
        WorkItemCommand::Attachments(a) => attachments(a, client, output).await,
        WorkItemCommand::AttachmentDownload(a) => attachment_download(a, client, output).await,
        WorkItemCommand::History(a) => history(a, client, output).await,
        WorkItemCommand::Open(a) => open(a, client, ctx.quiet).await,
        WorkItemCommand::Types(a) => types(a, client, output).await,
        WorkItemCommand::States(a) => states(a, client, output).await,
        WorkItemCommand::Fields(a) => fields(a, client, output).await,
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
    run_wiql_query(client, project, &wiql, output).await
}

// ── query ───────────────────────────────────────────────────────────────────

async fn query(args: QueryArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let wiql = read_wiql(&args)?;
    run_wiql_query(client, project, &wiql, output).await
}

fn read_wiql(args: &QueryArgs) -> Result<String> {
    if let Some(wiql) = args.wiql.as_deref() {
        return Ok(wiql.to_string());
    }
    let file = args
        .file
        .as_ref()
        .context("expected --wiql or --file for WIQL query")?;
    std::fs::read_to_string(file).with_context(|| format!("reading WIQL file {}", file.display()))
}

async fn run_wiql_query(
    client: &AdoClient,
    project: &str,
    wiql: &str,
    output: &OutputFormat,
) -> Result<()> {
    let result = api::run_wiql(client, project, wiql, None).await?;

    if result.work_items.is_empty() {
        if matches!(output, OutputFormat::Json) {
            output::print_json(&serde_json::json!([]))?;
        } else {
            println!("(no work items match)");
        }
        return Ok(());
    }

    let fields = [
        "System.Id",
        "System.Title",
        "System.State",
        "System.WorkItemType",
        "System.AssignedTo",
    ];
    let items = api::fetch_work_items(client, &result.work_items, &fields).await?;
    print_work_items(&items, output)
}

fn print_work_items(items: &[WorkItem], output: &OutputFormat) -> Result<()> {
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

    let ids = crate::stdin_ids::read_ids(&args.ids)?;

    // Per-ID continue-on-failure: one bad ID shouldn't block the rest. We collect
    // successes/failures and exit non-zero at the end if anything failed.
    let mut updated: Vec<WorkItem> = Vec::with_capacity(ids.len());
    let mut failures: Vec<(u32, anyhow::Error)> = Vec::new();
    for id in &ids {
        match api::patch_work_item(client, *id, &ops).await {
            Ok(wi) => updated.push(wi),
            Err(e) => failures.push((*id, e)),
        }
    }

    // Under --explain, every per-ID error is the dry-run sentinel. Skip the
    // "Failed #X" noise and propagate Explain so main exits 0.
    let explain = client.explain_enabled();

    match output {
        OutputFormat::Json => output::print_json(&updated)?,
        OutputFormat::Text | OutputFormat::Table => {
            for wi in &updated {
                println!("Updated #{}", wi.id);
            }
            if !explain {
                for (id, err) in &failures {
                    eprintln!("Failed #{id}: {err}");
                }
            }
        }
    }

    if !failures.is_empty() {
        if explain {
            return Err(crate::error::CliError::Explain.into());
        }
        bail!("{}/{} updates failed", failures.len(), ids.len());
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
    let resp: WiComment = api::add_comment(client, project, args.id, &args.text).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!("Added comment {} on #{}", resp.id, args.id);
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
    let resp: WiCommentList = client.get_json(&path).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            if resp.comments.is_empty() {
                println!("(no comments on #{})", args.id);
                return Ok(());
            }
            for c in &resp.comments {
                let author = c
                    .created_by
                    .as_ref()
                    .map(|i| i.display_name.as_str())
                    .unwrap_or("?");
                let date = c.created_date.as_deref().unwrap_or("");
                println!("─ #{}  {author}  {date}", c.id);
                println!("  {}", c.text);
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
    let resp: WiComment = client
        .patch_json(&path, &json!({ "text": args.text }))
        .await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
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
    let filename_owned = std::path::Path::new(&args.file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let filename = filename_owned.as_str();

    let upload_path = format!(
        "_apis/wit/attachments?fileName={}&api-version=7.1",
        encode_path(filename)
    );

    // Raw octet-stream POST bypasses the JSON helpers' explain hook, so guard
    // it explicitly. Done before reading the file to keep --explain side-effect-free.
    if let Some(e) = client.explain_skip(
        "POST (octet-stream)",
        &upload_path,
        Some(&format!("[binary upload of {}]", args.file)),
    ) {
        return Err(e);
    }

    let bytes = std::fs::read(&args.file).with_context(|| format!("reading {}", args.file))?;

    // 1. Upload the file body to get an attachment URL.
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
    let result = AttachResult {
        attachment,
        work_item: wi,
    };
    match output {
        OutputFormat::Json => output::print_json(&result)?,
        OutputFormat::Text | OutputFormat::Table => {
            println!(
                "Attached {filename} to #{} (id {})",
                args.id, result.attachment.id
            );
        }
    }
    Ok(())
}

async fn attachments(
    args: AttachmentsArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let list = fetch_attachment_list(client, project, args.id).await?;
    print_attachments(&list, output)
}

async fn attachment_download(
    args: AttachmentDownloadArgs,
    client: &AdoClient,
    output: &OutputFormat,
) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let list = fetch_attachment_list(client, project, args.id).await?;
    let selected = selected_attachments(&list, &args)?;
    let targets = plan_download_targets(&args, &selected)?;
    let mut files = Vec::with_capacity(targets.len());

    for (attachment, path, overwritten) in targets {
        if attachment.id.is_empty() {
            return Err(validation_error(format!(
                "attachment [{}] {} does not include an attachment id",
                attachment.index, attachment.name
            )));
        }
        let api_path = attachment_download_path(project, &attachment);
        let bytes = client
            .get_bytes(&api_path)
            .await
            .with_context(|| format!("downloading attachment {}", attachment.name))?;
        std::fs::write(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;
        files.push(WiAttachmentDownloadedFile {
            attachment,
            path: path.display().to_string(),
            bytes: bytes.len() as u64,
            overwritten,
        });
    }

    let result = WiAttachmentDownloadResult {
        work_item_id: list.work_item_id,
        files,
    };
    print_attachment_download_result(&result, output)
}

async fn fetch_attachment_list(
    client: &AdoClient,
    project: &str,
    id: u32,
) -> Result<WiAttachmentList> {
    let path = format!(
        "{project}/_apis/wit/workitems/{id}?$expand=relations&api-version=7.1",
        project = encode_path(project)
    );
    let wi: WorkItem = client.get_json(&path).await?;
    let attachments = attachments_from_relations(&wi.relations);
    Ok(WiAttachmentList {
        work_item_id: id,
        count: attachments.len(),
        attachments,
    })
}

fn attachments_from_relations(relations: &[Relation]) -> Vec<WiAttachment> {
    let mut attachments = Vec::new();
    for (relation_index, relation) in relations.iter().enumerate() {
        if relation.rel != "AttachedFile" {
            continue;
        }
        let index = attachments.len();
        let name = relation_attr_string(&relation.attributes, "name")
            .or_else(|| attachment_file_name_from_url(&relation.url))
            .unwrap_or_else(|| format!("attachment-{index}"));
        attachments.push(WiAttachment {
            index,
            relation_index,
            id: attachment_id_from_url(&relation.url).unwrap_or_default(),
            name,
            url: relation.url.clone(),
            size: relation
                .attributes
                .get("resourceSize")
                .and_then(|value| value.as_u64()),
            comment: relation_attr_string(&relation.attributes, "comment"),
            created_date: relation_attr_string(&relation.attributes, "resourceCreatedDate"),
            modified_date: relation_attr_string(&relation.attributes, "resourceModifiedDate"),
        });
    }
    attachments
}

fn relation_attr_string(attributes: &serde_json::Value, key: &str) -> Option<String> {
    attributes
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn attachment_id_from_url(url: &str) -> Option<String> {
    let marker = "/attachments/";
    let start = url.find(marker)? + marker.len();
    let id = url[start..]
        .split(['?', '/', '#'])
        .next()
        .filter(|id| !id.is_empty())?;
    Some(id.to_string())
}

fn attachment_file_name_from_url(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for part in query.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.eq_ignore_ascii_case("fileName") && !value.is_empty() {
            return Some(percent_decode_query_component(value));
        }
    }
    None
}

fn percent_decode_query_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                decoded.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn selected_attachments(
    list: &WiAttachmentList,
    args: &AttachmentDownloadArgs,
) -> Result<Vec<WiAttachment>> {
    if args.all {
        return Ok(list.attachments.clone());
    }
    let selector = args
        .selector
        .as_deref()
        .ok_or_else(|| validation_error("expected SELECTOR or --all"))?;
    let attachment = resolve_attachment_selector(&list.attachments, selector, list.work_item_id)?;
    Ok(vec![attachment.clone()])
}

fn resolve_attachment_selector<'a>(
    attachments: &'a [WiAttachment],
    selector: &str,
    work_item_id: u32,
) -> Result<&'a WiAttachment> {
    if let Ok(index) = selector.parse::<usize>() {
        if let Some(attachment) = attachments
            .iter()
            .find(|attachment| attachment.index == index)
        {
            return Ok(attachment);
        }
    }

    if let Some(attachment) = attachments
        .iter()
        .find(|attachment| attachment.id.eq_ignore_ascii_case(selector))
    {
        return Ok(attachment);
    }

    let matches: Vec<&WiAttachment> = attachments
        .iter()
        .filter(|attachment| attachment.name == selector)
        .collect();
    match matches.as_slice() {
        [attachment] => Ok(*attachment),
        [] => Err(validation_error(format!(
            "attachment selector `{selector}` not found on #{work_item_id}"
        ))),
        _ => Err(validation_error(format!(
            "multiple attachments named `{selector}` on #{work_item_id}; use attachment index or id"
        ))),
    }
}

fn plan_download_targets(
    args: &AttachmentDownloadArgs,
    attachments: &[WiAttachment],
) -> Result<Vec<(WiAttachment, PathBuf, bool)>> {
    let mut targets: Vec<(WiAttachment, PathBuf)> = attachments
        .iter()
        .cloned()
        .map(|attachment| {
            let path = target_path_for_attachment(args, &attachment);
            (attachment, path)
        })
        .collect();

    ensure_unique_targets(&targets)?;
    create_target_directories(args, &targets)?;

    let mut planned = Vec::with_capacity(targets.len());
    for (attachment, path) in targets.drain(..) {
        let overwritten = path.exists();
        if overwritten && !args.force {
            return Err(validation_error(format!(
                "{} already exists; pass --force to overwrite",
                path.display()
            )));
        }
        planned.push((attachment, path, overwritten));
    }
    Ok(planned)
}

fn target_path_for_attachment(args: &AttachmentDownloadArgs, attachment: &WiAttachment) -> PathBuf {
    if let Some(file) = args.file.as_ref() {
        return file.clone();
    }
    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
    dir.join(output_file_name(attachment))
}

fn output_file_name(attachment: &WiAttachment) -> String {
    let name = attachment.name.rsplit(['/', '\\']).next().unwrap_or("");
    if name.is_empty() {
        format!("attachment-{}", attachment.index)
    } else {
        name.to_string()
    }
}

fn ensure_unique_targets(targets: &[(WiAttachment, PathBuf)]) -> Result<()> {
    let mut seen = HashSet::new();
    for (_, path) in targets {
        if !seen.insert(path.clone()) {
            return Err(validation_error(format!(
                "multiple attachments would write to {}; download individually with --file",
                path.display()
            )));
        }
    }
    Ok(())
}

fn create_target_directories(
    args: &AttachmentDownloadArgs,
    targets: &[(WiAttachment, PathBuf)],
) -> Result<()> {
    if let Some(dir) = args.dir.as_ref() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        return Ok(());
    }
    if args.file.is_some() {
        for (_, path) in targets {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
        }
    }
    Ok(())
}

fn attachment_download_path(project: &str, attachment: &WiAttachment) -> String {
    format!(
        "{project}/_apis/wit/attachments/{id}?fileName={name}&download=true&api-version=7.1",
        project = encode_path(project),
        id = encode_path(&attachment.id),
        name = encode_path(&attachment.name)
    )
}

fn print_attachments(list: &WiAttachmentList, output: &OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => output::print_json(list)?,
        OutputFormat::Text => {
            if list.attachments.is_empty() {
                println!("(no attachments on #{})", list.work_item_id);
                return Ok(());
            }
            for attachment in &list.attachments {
                let size = attachment
                    .size
                    .map(|size| format!("{size} bytes"))
                    .unwrap_or_else(|| "? bytes".to_string());
                let comment = attachment
                    .comment
                    .as_deref()
                    .map(|comment| format!("  // {comment}"))
                    .unwrap_or_default();
                println!(
                    "[{}] {}  {}  id {}  relation [{}]{}",
                    attachment.index,
                    attachment.name,
                    size,
                    attachment.id,
                    attachment.relation_index,
                    comment
                );
            }
        }
        OutputFormat::Table => {
            if list.attachments.is_empty() {
                println!("(no attachments on #{})", list.work_item_id);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = list
                .attachments
                .iter()
                .map(|attachment| {
                    vec![
                        attachment.index.to_string(),
                        attachment.name.clone(),
                        attachment
                            .size
                            .map(|size| size.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                        attachment.id.clone(),
                        attachment.comment.clone().unwrap_or_default(),
                        attachment.relation_index.to_string(),
                    ]
                })
                .collect();
            output::print_table(WI_ATTACHMENT_HEADERS, &rows);
        }
    }
    Ok(())
}

fn print_attachment_download_result(
    result: &WiAttachmentDownloadResult,
    output: &OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Json => output::print_json(result)?,
        OutputFormat::Text => {
            if result.files.is_empty() {
                println!("(no attachments on #{})", result.work_item_id);
                return Ok(());
            }
            for file in &result.files {
                let action = if file.overwritten {
                    "Overwrote"
                } else {
                    "Downloaded"
                };
                println!(
                    "{action} {} to {} ({} bytes)",
                    file.attachment.name, file.path, file.bytes
                );
            }
        }
        OutputFormat::Table => {
            if result.files.is_empty() {
                println!("(no attachments on #{})", result.work_item_id);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = result
                .files
                .iter()
                .map(|file| {
                    vec![
                        file.attachment.name.clone(),
                        file.path.clone(),
                        file.bytes.to_string(),
                        if file.overwritten {
                            "yes".into()
                        } else {
                            "".into()
                        },
                    ]
                })
                .collect();
            output::print_table(&["Name", "Path", "Bytes", "Overwritten"], &rows);
        }
    }
    Ok(())
}

fn validation_error(message: impl Into<String>) -> anyhow::Error {
    CliError::Validation(message.into()).into()
}

// ── history ─────────────────────────────────────────────────────────────────

async fn history(args: HistoryArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let resp = api::list_updates(client, &client.project, args.id, Some(args.limit)).await?;

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text | OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no history)");
                return Ok(());
            }
            for u in &resp.value {
                let by = u
                    .revised_by
                    .as_ref()
                    .map(|i| i.display_name.as_str())
                    .unwrap_or("?");
                let date = u.revised_date.as_deref().unwrap_or("");
                println!("rev {}  {by}  {date}", u.rev);
                if let Some(fields) = u.fields.as_object() {
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

// ── metadata: types / states / fields ───────────────────────────────────────

async fn types(args: TypesArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workitemtypes?api-version=7.1",
        project = encode_path(project)
    );
    let mut resp: WorkItemTypeListResponse = client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no work item types in {project})");
                return Ok(());
            }
            let width = resp.value.iter().map(|t| t.name.len()).max().unwrap_or(0);
            for t in &resp.value {
                let dis = if t.is_disabled { " (disabled)" } else { "" };
                if t.description.is_empty() {
                    println!("{:<width$}{dis}", t.name, width = width);
                } else {
                    println!("{:<width$}  {}{dis}", t.name, t.description, width = width);
                }
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no work item types in {project})");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|t| {
                    vec![
                        t.name.clone(),
                        t.reference_name.clone(),
                        if t.is_disabled {
                            "yes".into()
                        } else {
                            "".into()
                        },
                        t.description.clone(),
                    ]
                })
                .collect();
            output::print_table(
                &["Name", "Reference Name", "Disabled", "Description"],
                &rows,
            );
        }
    }
    Ok(())
}

async fn states(args: StatesArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = format!(
        "{project}/_apis/wit/workitemtypes/{type}/states?api-version=7.1",
        project = encode_path(project),
        r#type = encode_path(&args.r#type)
    );
    let mut resp: StateListResponse = client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.order.cmp(&b.order));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no states for {})", args.r#type);
                return Ok(());
            }
            for s in &resp.value {
                if s.category.is_empty() {
                    println!("{}", s.name);
                } else {
                    println!("{:<20}  [{}]", s.name, s.category);
                }
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no states for {})", args.r#type);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|s| vec![s.name.clone(), s.category.clone(), s.color.clone()])
                .collect();
            output::print_table(&["Name", "Category", "Color"], &rows);
        }
    }
    Ok(())
}

async fn fields(args: FieldsArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let path = match args.r#type.as_deref() {
        Some(t) => format!(
            "{project}/_apis/wit/workitemtypes/{type}/fields?api-version=7.1",
            project = encode_path(project),
            r#type = encode_path(t)
        ),
        None => format!(
            "{project}/_apis/wit/fields?api-version=7.1",
            project = encode_path(project)
        ),
    };
    let mut resp: FieldListResponse = client.get_json(&path).await?;
    resp.value.sort_by(|a, b| a.name.cmp(&b.name));

    match output {
        OutputFormat::Json => output::print_json(&resp)?,
        OutputFormat::Text => {
            if resp.value.is_empty() {
                println!("(no fields)");
                return Ok(());
            }
            let name_w = resp.value.iter().map(|f| f.name.len()).max().unwrap_or(0);
            let ref_w = resp
                .value
                .iter()
                .map(|f| f.reference_name.len())
                .max()
                .unwrap_or(0);
            // Per-type endpoint omits `type`; print two columns there.
            let any_type = resp.value.iter().any(|f| !f.field_type.is_empty());
            for f in &resp.value {
                if any_type {
                    println!(
                        "{:<name_w$}  {:<ref_w$}  {}",
                        f.name,
                        f.reference_name,
                        f.field_type,
                        name_w = name_w,
                        ref_w = ref_w
                    );
                } else {
                    println!("{:<name_w$}  {}", f.name, f.reference_name, name_w = name_w);
                }
            }
        }
        OutputFormat::Table => {
            if resp.value.is_empty() {
                println!("(no fields)");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = resp
                .value
                .iter()
                .map(|f| {
                    vec![
                        f.name.clone(),
                        f.reference_name.clone(),
                        f.field_type.clone(),
                        if f.read_only { "yes".into() } else { "".into() },
                    ]
                })
                .collect();
            output::print_table(&["Name", "Reference Name", "Type", "Read-Only"], &rows);
        }
    }
    Ok(())
}

// ── open ────────────────────────────────────────────────────────────────────

async fn open(args: OpenArgs, client: &AdoClient, quiet: bool) -> Result<()> {
    let project = args.project.as_deref().unwrap_or(&client.project);
    let url = format!(
        "{}/{}/_workitems/edit/{}",
        client.org,
        encode_path(project),
        args.id
    );
    output::banner(
        quiet,
        &format!("Opening work item #{} in browser...", args.id),
    );
    AdoClient::open_in_browser(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(index: usize, id: &str, name: &str) -> WiAttachment {
        WiAttachment {
            index,
            relation_index: index + 10,
            id: id.to_string(),
            name: name.to_string(),
            url: format!("https://dev.azure.com/org/_apis/wit/attachments/{id}"),
            size: None,
            comment: None,
            created_date: None,
            modified_date: None,
        }
    }

    fn download_args(dir: Option<PathBuf>, force: bool) -> AttachmentDownloadArgs {
        AttachmentDownloadArgs {
            id: 123,
            selector: Some("0".into()),
            all: false,
            dir,
            file: None,
            force,
            project: None,
        }
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("ado-helper-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn extracts_attached_file_relations() {
        let relations = vec![
            Relation {
                rel: "System.LinkTypes.Related".into(),
                url: "https://dev.azure.com/org/_apis/wit/workItems/456".into(),
                attributes: json!({}),
            },
            Relation {
                rel: "AttachedFile".into(),
                url: "https://dev.azure.com/org/_apis/wit/attachments/55942c02-4fe2-41e9-8bfd-559e2e244c36?fileName=ignored.txt".into(),
                attributes: json!({
                    "name": "report.xlsx",
                    "resourceSize": 42,
                    "comment": "source workbook",
                    "resourceCreatedDate": "2026-05-24T12:00:00Z",
                    "resourceModifiedDate": "2026-05-24T12:01:00Z"
                }),
            },
        ];

        let attachments = attachments_from_relations(&relations);

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].index, 0);
        assert_eq!(attachments[0].relation_index, 1);
        assert_eq!(attachments[0].id, "55942c02-4fe2-41e9-8bfd-559e2e244c36");
        assert_eq!(attachments[0].name, "report.xlsx");
        assert_eq!(attachments[0].size, Some(42));
        assert_eq!(attachments[0].comment.as_deref(), Some("source workbook"));
        assert_eq!(
            attachments[0].created_date.as_deref(),
            Some("2026-05-24T12:00:00Z")
        );
    }

    #[test]
    fn selector_resolves_by_index_id_or_exact_name() {
        let attachments = vec![
            attachment(0, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "first.xlsx"),
            attachment(1, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "second.xlsx"),
        ];

        assert_eq!(
            resolve_attachment_selector(&attachments, "1", 123)
                .unwrap()
                .name,
            "second.xlsx"
        );
        assert_eq!(
            resolve_attachment_selector(&attachments, "BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB", 123)
                .unwrap()
                .name,
            "second.xlsx"
        );
        assert_eq!(
            resolve_attachment_selector(&attachments, "first.xlsx", 123)
                .unwrap()
                .id,
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
        );
    }

    #[test]
    fn duplicate_filename_selector_is_validation_error() {
        let attachments = vec![
            attachment(0, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "report.xlsx"),
            attachment(1, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "report.xlsx"),
        ];

        let err = resolve_attachment_selector(&attachments, "report.xlsx", 123).unwrap_err();

        assert!(err.downcast_ref::<CliError>().is_some());
    }

    #[test]
    fn existing_target_requires_force() {
        let dir = unique_test_dir("existing-target");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("report.xlsx");
        std::fs::write(&path, b"old").expect("seed existing file");
        let attachments = vec![attachment(
            0,
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "report.xlsx",
        )];

        let err = plan_download_targets(&download_args(Some(dir.clone()), false), &attachments)
            .unwrap_err();
        assert!(err.downcast_ref::<CliError>().is_some());

        let planned =
            plan_download_targets(&download_args(Some(dir.clone()), true), &attachments).unwrap();
        assert!(planned[0].2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_targets_are_rejected_for_all_downloads() {
        let dir = unique_test_dir("duplicate-targets");
        let mut args = download_args(Some(dir.clone()), true);
        args.selector = None;
        args.all = true;
        let attachments = vec![
            attachment(0, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", "report.xlsx"),
            attachment(1, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", "report.xlsx"),
        ];

        let err = plan_download_targets(&args, &attachments).unwrap_err();

        assert!(err.downcast_ref::<CliError>().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
