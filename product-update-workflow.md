# product-update-workflow.md - sprint product stakeholder update

You are creating a product-facing stakeholder update from one Azure DevOps sprint. The output is a local markdown file only. This is a read-only reporting workflow: do not create, update, comment on, or send anything through ADO, Slack, Teams, email, or any external system.

The audience is product/client stakeholders. Write in product language: what changed, why it matters, what is still in motion, what is blocked, and what is next. Do not mention ticket IDs. Do not mention QA status.

## Input

- `<iteration-ref>` - optional. Accepts an ADO iteration id or shorthand such as `@current`, `@next`, or `@previous`. Default to `@current`.
- `<out-file>` - optional. Default to `product-update-<iteration-slug>.md`.

## Required Environment

- `ado` CLI on `$PATH`, authenticated via `.env` or saved config.
- A configured Azure DevOps organization, project, and team.

## Command Surface

Use only read-only `ado` commands:

- `ado iteration view <iteration-ref> --output json`
- `ado sprint backlog --iteration <iteration-ref> --output json`
- `ado sprint summary --iteration <iteration-ref> --output json`
- `ado wi view <id> --output json`
- `ado wi comments <id> --output json` for recent blocker context when needed
- `ado schema <cmd-path>` if output shape is unclear

Do not use any mutating command: no `ado wi create`, `ado wi update`, `ado wi comment`, `ado wi attach`, `ado sprint plan-into`, or PR mutation commands.

## Error Handling

Exit codes: `0` success, `2` not-found, `3` validation, `4` auth, `5` API error.

Any non-zero exit from an `ado` command aborts with the phase name, command, exit code, and stderr. If one optional enrichment command fails, such as comments for a single item, skip that enrichment and record a local warning; do not fail the whole report unless the core sprint data cannot be read.

## Phase 1 - Resolve the Sprint

Goal: normalize the sprint reference and output filename.

```sh
ado iteration view <iteration-ref> --output json
```

Extract:

- `.name`
- `.path`
- `.attributes.startDate`
- `.attributes.finishDate`
- `.attributes.timeFrame`

If `<iteration-ref>` was omitted, use `@current`.

Derive `<iteration-slug>` from `.name`:

- Lowercase.
- Replace non-alphanumeric runs with `-`.
- Trim leading/trailing `-`.

Default output path: `product-update-<iteration-slug>.md`.

## Phase 2 - Read Sprint Inventory

Goal: gather the sprint-level source data without relying on the web UI.

Run:

```sh
ado sprint backlog --iteration <iteration-ref> --output json
```

Extract from `.iteration`:

- `name`
- `path`
- `start_date`
- `finish_date`
- `time_frame`

Extract from `.value[]`:

- `id` (internal only; never print in the final stakeholder body)
- `title`
- `work_item_type`
- `state`
- `tags`
- `area_path`
- `assigned_to.display_name`
- `remaining_work`
- `story_points`
- `effort`

Also run:

```sh
ado sprint summary --iteration <iteration-ref> --output json
```

Use this only for internal context about completed count, planned count, carryover, and additions. Do not turn raw counts into stakeholder filler unless they clarify the update.

If the backlog is empty, write a short markdown update that says there were no product-facing changes captured for the sprint, then stop.

## Phase 3 - Hydrate Relevant Work Items

Goal: get enough detail to summarize product impact accurately.

Classify sprint work items from the backlog:

- Completed: `System.State` / `state` is `Done` or another completed state discovered from the sprint data.
- In progress: `state` is `Doing`, `Active`, `In Progress`, `Committed`, or similar.
- Blocked: tags include `blocked` or a blocker-like tag, or title/description/comments clearly indicate blocked status.

Prioritize parent-level items, especially `Issue` work items. Use child `Task` items only to understand what changed or why something is blocked; the final update should not read like a task list.

For each relevant parent item, and for child tasks when needed for context:

```sh
ado wi view <id> --output json
```

Extract from `.fields`:

- `System.Title`
- `System.Description`
- `Microsoft.VSTS.Common.AcceptanceCriteria` if present
- `System.WorkItemType`
- `System.State`
- `System.Tags`
- `System.AreaPath`
- `System.IterationPath`
- `System.Parent` if present

Also inspect `.relations` for hierarchy links:

- `System.LinkTypes.Hierarchy-Forward` points from parent to child.
- `System.LinkTypes.Hierarchy-Reverse` points from child to parent.

For blocked or notable in-progress items where the reason is unclear, optionally read comments:

```sh
ado wi comments <id> --output json
```

Use comments only to understand stakeholder-relevant blocker context. Do not quote private/internal comments verbatim unless they are already written for stakeholders.

## Phase 4 - Decide What Is Notable

Goal: keep the update concise and useful.

Include:

- Completed product-facing work.
- Completed internal work only when it affects reliability, performance, delivery confidence, data quality, security, or customer operations.
- Notable in-progress work that stakeholders expect or that materially affects near-term plans.
- Blocked work when the blocker changes timing, scope, dependency expectations, or stakeholder coordination.
- Meaningful known limitations or follow-up work.

Exclude:

- Ticket IDs.
- QA status or QA progress.
- Individual developer names unless ownership is stakeholder-relevant.
- Branch names, PR numbers, commit hashes, internal file paths, or implementation chatter.
- Low-level tasks that do not change product behavior or delivery expectations.
- Raw ADO counts unless they clarify scope.

If a work item title is technical, translate it into product language. For example, "Refactor auth middleware" becomes "Improved sign-in reliability" only if the hydrated ticket supports that impact.

## Phase 5 - Write the Markdown Update

Goal: produce a markdown file the manager can review and edit before sharing.

Use this structure:

```md
# Product Update: <Sprint Name>

## Summary
<2-4 sentences summarizing the sprint in product language.>

## Completed
### <Product area or workflow>
- <What changed and why it matters.>
- <User/customer impact when clear.>

## In Progress
- <Notable item still moving, with plain-language expected outcome or next step.>

## Blocked
- <Blocked item, blocker context, and what is needed next.>

## Known Limitations
- <Important limitation or deferred scope. Omit the section if empty.>

## Next
- <Likely next product-facing step or follow-up.>
```

Rules:

- Omit empty sections except `Summary`.
- Do not mention ticket IDs anywhere in the stakeholder-facing output.
- Do not mention QA status.
- Use product area or user workflow headings, not repo names, unless the repo name is itself stakeholder-facing.
- Keep bullets short and outcome-oriented.
- Use cautious language when inferring impact from technical work.
- Do not claim a feature shipped unless sprint state and ticket content support that claim.

## Phase 6 - Local Review Checks

Goal: catch unsafe or noisy output before handing it to the manager.

Before finishing, scan the markdown for:

- Ticket IDs like `#123`, `AB#123`, or raw ADO URLs.
- QA status language such as `QA`, `tested`, `validated`, `pending validation`, or `test pass`.
- Internal implementation noise: branch names, PR IDs, file paths, commit hashes.
- Unsupported claims such as "released" or "available to all users" when the source only says work is Done.

Revise the file until the update is safe to review.

## Final Output

Print:

```text
Wrote product stakeholder update: <out-file>
Source sprint: <iteration path>
Items reviewed: <count>
Warnings: <none or short list>
```

Do not print the full markdown body unless the manager asks for it.

## Guarantees the Workflow Makes

- Read-only ADO access only.
- Writes exactly one local markdown file.
- Does not auto-send AI-generated content.
- Uses one sprint as the source scope.
- Includes completed, notable in-progress, and blocked work.
- Does not mention ticket IDs or QA status in the stakeholder-facing update.
