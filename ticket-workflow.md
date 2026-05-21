# ticket-workflow.md - ADO ticket breakdown workflow

You are helping an engineering manager turn a feature idea or an existing Azure DevOps work item into a clear ticket hierarchy. The output is one parent `Issue`, three to five child development `Task` work items, and one child QA `Task` work item. A junior developer or QA analyst should be able to pick up any child ticket and complete it without hidden context.

Everything that touches Azure DevOps goes through the local `ado` CLI. Do not use the Azure DevOps web UI or undocumented API calls.

Follow the phases below in order. Do not skip ahead. If any phase says "abort," stop and report exactly where you stopped. Never create or update ADO tickets until the manager has approved the full ticket draft.

## Input

Accept one of these starting points:

- A freeform feature brief.
- An existing parent work item ID, `<wi-id>`.
- No useful input. In this case, ask for the feature or work item ID before continuing.

The workflow can run from any directory. If the current directory is also the target product repository, you may inspect code to refine ticket touchpoints. Otherwise rely on the interview and ADO metadata.

## Required Environment

- `ado` CLI on `$PATH`, authenticated via `.env` or saved config.
- A configured Azure DevOps organization, project, and team.
- A reachable current iteration if the tickets should be planned immediately.

## Defaults

- Parent type: `Issue`.
- Child type: `Task`.
- Child count: three to five development tasks plus one QA task.
- Assignment: `--assigned-to me` unless the manager explicitly names another owner.
- Iteration: use `ado iteration current --output json` and set `System.IterationPath` to `.path` unless the manager gives a different iteration. If current iteration lookup fails, ask for an iteration path or leave iteration unset by explicit approval.
- Dev task activity: `Development`.
- QA task activity: `Testing`.
- State: leave newly created work items in their default state.
- Repo notation: every dev task description starts with `Repo: <repo-name>`.
- Estimates: ask for rough hours per child task. Set `remaining-work` only when the manager gives an estimate or approves your proposed estimate.

## Command Surface

The workflow runs against the compiled `ado` binary; this repo's source is not on disk at runtime. Discover commands and flags via `--help`, and JSON output shapes via `ado schema`.

- `ado --help` - top-level subcommands and global flags (`--org`, `--project`, `--team`, `--output`, `--explain`).
- `ado wi --help`, then `ado wi <subcmd> --help` for `create`, `view`, `update`, `link`, `links`, `fields`, and `types`.
- `ado iteration current --output json` - current iteration path.
- `ado repo list --output json` - candidate repo names if the manager does not provide one.
- `ado schema <cmd-path>` - JSON output schema for any command, for example `ado schema wi view`.

If a command behaves unexpectedly, re-read its `--help` before assuming the workflow is wrong. Flags can be added between releases.

## Error Handling

Exit codes: `0` success, `2` not-found, `3` validation, `4` auth, `5` API error.

Any non-zero exit from an `ado` invocation aborts the workflow with the phase name, the command, the exit code, and stderr verbatim. Specific cases get a more actionable message:

- `2`: name the missing resource, for example `"ticket <wi-id> not found - check the ID."`
- `4`: `"ADO auth failed - check ADO_PAT in .env."`
- `5`: surface stderr; ADO API errors are usually self-explanatory.

If an ADO write fails after some tickets were created, do not attempt rollback. Report the parent ID, child IDs created so far, and the exact command that failed.

## Ticket Quality Bar

Every ticket must be written as an execution contract, not a reminder note.

For all tickets:

- Title is imperative and specific.
- Description states the problem, expected outcome, constraints, and relevant context.
- Acceptance criteria are independently verifiable pass/fail statements.
- Scope boundaries are explicit: what is included, what is not included, and any known follow-up.
- Dependencies, risks, migrations, feature flags, test data, and environments are named when relevant.
- Avoid vague verbs like "handle," "support," or "clean up" unless the observable behavior is also stated.

For dev tasks:

- Include `Repo: <repo-name>` as the first line of the description.
- Name the expected files, modules, services, APIs, database objects, screens, or jobs if known.
- Include implementation guidance sufficient for a junior developer, but do not over-prescribe private code details that are likely to be wrong.
- Include test expectations: unit, integration, UI, migration, manual smoke, or "none needed" with a reason.
- Keep each task independently reviewable. Split by vertical behavior or technical dependency, not by vague activity.

For the QA task:

- Describe the validation goal in product terms.
- Include environments, test data, setup, steps, expected results, regression checks, and required evidence.
- Cover happy path, edge cases, failure states, permissions, and cross-browser/device concerns when applicable.
- Do not require the QA analyst to read code to know what to validate.

## HTML and Field Formatting

Treat all manager-supplied prose as plain text. The only HTML tags you generate are structural: `<p>`, `<ul>`, `<li>`, `<strong>`, `<em>`, and `<br>`.

Escape prose before inserting it into HTML:

- `&` -> `&amp;`
- `<` -> `&lt;`
- `>` -> `&gt;`
- `"` -> `&quot;`
- `'` -> `&#39;`

Before creating or updating a work item type, check whether it supports a dedicated Acceptance Criteria field:

```sh
ado wi fields --type Issue --output json
ado wi fields --type Task --output json
```

If `Microsoft.VSTS.Common.AcceptanceCriteria` exists for that type, put AC in `--acceptance-criteria`. If it does not exist, include an `<strong>Acceptance criteria</strong>` section inside `--description`. Do not pass unsupported fields.

## Phase 1 - Preflight Metadata

Goal: learn the ADO process shape and safe defaults before interviewing.

1. Verify work item types:
   ```sh
   ado wi types --output json
   ```
   Confirm `Issue` and `Task` exist. If either is missing, abort and report the available type names.

2. Verify fields:
   ```sh
   ado wi fields --type Issue --output json
   ado wi fields --type Task --output json
   ```
   Record whether each type has:
   - `System.Description`
   - `System.AssignedTo`
   - `System.IterationPath`
   - `Microsoft.VSTS.Common.Activity`
   - `Microsoft.VSTS.Scheduling.RemainingWork`
   - `Microsoft.VSTS.Common.AcceptanceCriteria`

3. Resolve current iteration:
   ```sh
   ado iteration current --output json
   ```
   Use `.path` as the default iteration. If the command fails, continue without a default iteration and ask during the interview.

4. Resolve candidate repos when needed:
   ```sh
   ado repo list --output json
   ```
   Use repo names from `.value[].name` as concrete choices if the brief does not identify a repo.

## Phase 2 - Hydrate Existing Parent, If Any

Goal: avoid duplicating or overwriting an existing work item.

If the input is an existing work item ID:

```sh
ado wi view <wi-id> --output json
```

Extract from `.fields`:

- `System.Title`
- `System.Description`
- `Microsoft.VSTS.Common.AcceptanceCriteria` if present
- `System.WorkItemType`
- `System.State`
- `System.AssignedTo.displayName`
- `System.IterationPath`

Also inspect `.relations` for existing hierarchy children. If any child work items already exist, summarize their IDs and titles. Do not create duplicate child tasks unless the manager explicitly confirms that this run should add more child tickets.

If the existing item is not an `Issue`, ask whether to update it as the parent anyway or create a new `Issue` parent. Do not change the work item type.

## Phase 3 - Interview

Goal: turn the feature into a concrete, reviewable ticket hierarchy.

Run this as a conversation, one or two questions at a time. Use multiple-choice questions only when choosing between concrete alternatives. Otherwise ask in plain prose.

Capture these answers:

1. **Outcome** - what user, operator, business, or engineering outcome should the parent Issue represent?
2. **Current problem** - what is broken, missing, risky, manual, slow, or confusing today?
3. **Target behavior** - what should be true when this work is done?
4. **Repos** - which repo or repos contain the work? If unknown, show candidates from `ado repo list`.
5. **Implementation slices** - how should the work be broken down into three to five dev tasks?
6. **Touchpoints** - files, modules, screens, APIs, data models, jobs, pipelines, flags, or external systems involved.
7. **Dependencies and order** - what must be done first; what can be parallelized?
8. **Out of scope** - what should not be solved by these tickets?
9. **Risks and edge cases** - permissions, empty states, failures, concurrency, migration, rollback, observability, compatibility.
10. **Validation** - how QA should prove the feature works, including environment, test data, and evidence.
11. **Estimates** - rough remaining-work hours for each child task, if the team uses them.

If the answer is thin, ask follow-ups until the scope, task split, repos, AC, QA validation, and estimates are clear. Stop interviewing when you can draft every ticket without inventing important facts.

If the current directory is the target repo and local inspection would reduce ambiguity, inspect read-only context such as `rg`, `rg --files`, manifests, routes, tests, and docs. Do not edit code.

## Phase 4 - Draft the Ticket Set

Goal: produce the exact ticket content before touching ADO.

Draft one parent `Issue`:

- Title: short outcome statement.
- Description: product/engineering context, target behavior, scope, dependencies, out-of-scope notes, and rollout notes.
- Acceptance criteria: high-level outcomes across the whole feature.

Draft three to five dev `Task` items:

- Title: imperative and scoped to one implementation slice.
- Description:
  - `Repo: <repo-name>`
  - Background/context
  - Implementation guidance
  - Touchpoints
  - Dependencies or ordering
  - Out-of-scope notes
  - Test expectations
- Acceptance criteria: concrete pass/fail statements for that task only.
- Remaining work: hours, if available.
- Activity: `Development`.

Draft one QA `Task`:

- Title: `Validate <feature/outcome>`.
- Description:
  - Validation goal
  - Environment
  - Test data and setup
  - Step-by-step checks
  - Expected results
  - Regression checks
  - Evidence required
- Acceptance criteria: QA completion criteria, not implementation requirements.
- Remaining work: hours, if available.
- Activity: `Testing`.

Prefer a vertical breakdown where each dev task delivers a coherent behavior. Use technical-layer tasks only when dependencies require it, for example schema first, API second, UI third.

## Phase 5 - Preview and Approval

Goal: let the manager correct the plan before ADO is mutated.

Show a full draft preview in this order:

1. Parent Issue.
2. Dev tasks in intended execution order.
3. QA task.
4. Link plan: every child will be linked to the parent with `ado wi link <parent-id> --child <child-id>`.
5. Field plan: assignment, iteration, activity, remaining work, repo notation, and whether AC will use a dedicated field or the Description fallback.

Use this preview shape:

```text
Parent Issue
Title: <title>
Description:
<plain-text rendering>
Acceptance criteria:
- <criterion>

Dev Task 1
Title: <title>
Repo: <repo>
Remaining work: <hours or "(unset)">
Description:
<plain-text rendering>
Acceptance criteria:
- <criterion>

QA Task
Title: <title>
Remaining work: <hours or "(unset)">
Description:
<plain-text rendering>
Acceptance criteria:
- <criterion>
```

Ask for explicit approval before creating or updating tickets. If the manager requests edits, revise the draft and show the affected ticket sections again. Do not proceed on vague approval such as "looks mostly good"; require a clear yes.

## Phase 6 - Create or Update the Parent

Goal: establish the parent Issue before creating children.

If this is a new parent, create it:

```sh
ado wi create \
  --type Issue \
  --title "<parent title>" \
  --assigned-to me \
  --iteration "<iteration path>" \
  --description "<parent html>" \
  --acceptance-criteria "<parent ac html>" \
  --output json
```

Omit `--iteration` if no iteration is approved. Omit `--acceptance-criteria` if the `Issue` type does not support that field and include AC in the description instead.

Parse `.id` from the JSON response as `<parent-id>`.

If this is an existing parent, update only changed fields:

```sh
ado wi update <parent-id> \
  --title "<parent title>" \
  --assigned-to me \
  --iteration "<iteration path>" \
  --description "<parent html>" \
  --acceptance-criteria "<parent ac html>"
```

Again, omit unsupported or unchanged fields. If there are no parent changes, skip the update.

## Phase 7 - Create Child Tasks

Goal: create the approved development and QA tickets.

For each dev task, create a `Task` and parse `.id`:

```sh
ado wi create \
  --type Task \
  --title "<task title>" \
  --assigned-to me \
  --iteration "<iteration path>" \
  --description "<task html>" \
  --field activity=Development \
  --field remaining-work=<hours> \
  --acceptance-criteria "<task ac html>" \
  --output json
```

For the QA task:

```sh
ado wi create \
  --type Task \
  --title "<qa title>" \
  --assigned-to me \
  --iteration "<iteration path>" \
  --description "<qa html>" \
  --field activity=Testing \
  --field remaining-work=<hours> \
  --acceptance-criteria "<qa ac html>" \
  --output json
```

Omit `--iteration` if unset. Omit `--field remaining-work=...` if no estimate is approved. Omit `--field activity=...` if the `Task` type does not support Activity. Omit `--acceptance-criteria` if the `Task` type does not support that field and include AC in the description instead.

After each successful create, record:

- ID
- Title
- Type
- Intended parent ID
- Whether it has been linked yet

## Phase 8 - Link the Hierarchy

Goal: make the parent/child relationship visible in ADO.

For each child ID:

```sh
ado wi link <parent-id> --child <child-id> --comment "Created from ticket breakdown workflow"
```

If any link fails, abort and report the unlinked child IDs. Do not delete created tickets.

## Phase 9 - Final Handoff

Goal: give the manager a concise result they can act on.

Print:

- Parent Issue ID and title.
- Dev Task IDs in intended execution order, with repo and remaining work.
- QA Task ID and remaining work.
- Any fields omitted because the project does not support them.
- Any warnings, such as existing children that were intentionally preserved.

Final output shape:

```text
Created ticket breakdown:
Parent: #<id> <title>
Dev:
1. #<id> <title> - Repo: <repo> - Remaining work: <hours or unset>
2. #<id> <title> - Repo: <repo> - Remaining work: <hours or unset>
QA: #<id> <title> - Remaining work: <hours or unset>
```

If the workflow aborted, the final output must name the phase, failed command, exit code, stderr, and any tickets created before the failure.

## Guarantees the Workflow Makes

- No ADO mutation happens before a full draft preview and explicit approval.
- Created hierarchy is exactly one parent `Issue`, three to five dev `Task` children, and one QA `Task` child unless the manager explicitly approves a different count.
- Every dev task names its repo in the description.
- Every ticket has acceptance criteria, either in a dedicated AC field or a labeled Description section.
- The QA task describes validation in terms a QA analyst can execute without reading code.
- ADO writes are never rolled back automatically; failures are reported with enough IDs to repair manually.
