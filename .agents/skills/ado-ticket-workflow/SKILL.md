---
name: ado-ticket-workflow
description: Use this skill when turning a feature brief or existing Azure DevOps work item into an approved parent Issue, three to five development Tasks, and one QA Task with safe ADO creation and linking.
disable-model-invocation: true
---

# ADO Ticket Workflow

Turn a feature brief or existing work item into a clear ticket hierarchy: one parent `Issue`, three to five child development `Task` items, and one child QA `Task`. A junior developer or QA analyst should be able to pick up any child ticket and complete it without hidden context. Never create or update ADO tickets until the manager has approved the full ticket draft.

## Inputs

- Start with one of: a feature brief, an existing work item ID, or no useful input.
- If no useful input is provided, ask for either the feature idea or work item ID before continuing.
- Treat the requester as the approving manager for this workflow.

## Preconditions

- `ado` CLI on `$PATH` and configured for the target Azure DevOps org, project, and team. Do not inspect, print, request, log, or manage `ADO_PAT`; treat it as opaque.
- All Azure DevOps access goes through the local `ado` CLI. Do not use the ADO web UI, raw undocumented REST calls, or guessed JSON shapes. Discover commands with `ado --help`, `ado <subcmd> --help`. Discover JSON shapes with `ado schema <command-path>`. Prefer `--output json` for any command this skill parses.
- A reachable current iteration if the tickets should be planned immediately.

## Defaults

- Parent type: `Issue`.
- Child type: `Task`.
- Child count: three to five development tasks plus one QA task.
- Assignment: `--assigned-to me` unless the manager explicitly names another owner.
- Iteration: use `ado iteration current --output json` and set `System.IterationPath` to `.path`. If lookup fails, ask for an iteration path or approval to leave iteration unset.
- Dev task activity: `Development`. QA task activity: `Testing`.
- State: leave newly created work items in their default state.
- Repo notation: every dev task description starts with `Repo: <repo-name>`.
- Estimates: ask for rough hours per child task. Set `remaining-work` only when the manager gives or approves an estimate.

## Ticket Quality Bar

Every ticket must be written as an execution contract, not a reminder note.

**For all tickets:**
- Title is imperative and specific.
- Description states the problem, expected outcome, constraints, and relevant context.
- Acceptance criteria are independently verifiable pass/fail statements.
- Scope boundaries are explicit: what is included, what is not, and any known follow-up.
- Dependencies, risks, migrations, feature flags, test data, and environments are named when relevant.
- Avoid vague verbs like "handle," "support," or "clean up" unless the observable behavior is also stated.

**For dev tasks:**
- Include `Repo: <repo-name>` as the first line of the description.
- Name the expected files, modules, services, APIs, database objects, screens, or jobs if known.
- Include implementation guidance sufficient for a junior developer, but do not over-prescribe private code details.
- Include test expectations: unit, integration, UI, migration, manual smoke, or "none needed" with a reason.
- Keep each task independently reviewable. Split by vertical behavior or technical dependency, not by vague activity.

**For the QA task:**
- Describe the validation goal in product terms.
- Include environments, test data, setup, steps, expected results, regression checks, and required evidence.
- Cover happy path, edge cases, failure states, permissions, and cross-browser/device concerns when applicable.
- Do not require the QA analyst to read code to know what to validate.

## HTML field formatting

ADO Description, Acceptance Criteria, and similar rich-text fields accept HTML. The `ado` CLI passes these flags through unchanged — un-escaped prose will break the rendered field.

Treat all manager-supplied prose as plain text. Escape every prose value before inserting into any tag:

| Char | Escape   |
| ---- | -------- |
| `&`  | `&amp;`  |
| `<`  | `&lt;`   |
| `>`  | `&gt;`   |
| `"`  | `&quot;` |
| `'`  | `&#39;`  |

The only HTML tags this skill may generate are structural: `<p>`, `<ul>`, `<li>`, `<strong>`, `<em>`, `<br>`.

Use `--acceptance-criteria` only when `Microsoft.VSTS.Common.AcceptanceCriteria` is supported for the work item type. Otherwise include a labeled `<strong>Acceptance criteria</strong>` section inside `--description`.

## Command Surface

General discovery (run `ado <subcmd> --help` ad-hoc when a flag is unclear):

```sh
ado --help
```

ADO metadata and schemas:

```sh
ado wi types --output json
ado wi fields --type Issue --output json
ado wi fields --type Task --output json
ado iteration current --output json
ado repo list --output json
ado schema --list --output json
ado schema wi view
ado schema wi create
ado schema wi links
ado schema wi types
ado schema wi fields
ado schema iteration current
ado schema repo list
```

ADO reads:

```sh
ado wi view <wi-id> --output json
ado wi links <wi-id> --output json
```

ADO mutations (all after approval only):

```sh
ado wi create --type Issue --title "<title>" --assigned-to <owner> --iteration "<path>" \
  --description "<html>" --acceptance-criteria "<html>" --output json
ado wi update <parent-id> --title "<title>" --assigned-to <owner> --iteration "<path>" \
  --description "<html>" --acceptance-criteria "<html>"
ado wi create --type Task --title "<title>" --assigned-to <owner> --iteration "<path>" \
  --description "<html>" --field activity=Development --field remaining-work=<hours> \
  --acceptance-criteria "<html>" --output json
ado wi create --type Task --title "<title>" --assigned-to <owner> --iteration "<path>" \
  --description "<html>" --field activity=Testing --field remaining-work=<hours> \
  --acceptance-criteria "<html>" --output json
ado wi link <parent-id> --child <child-id> --comment "Created from ticket breakdown workflow"
```

Local read-only inspection, only when the current directory is the target product repository:

```sh
pwd
git remote get-url origin
rg --files
rg -n "<term>" <path>
sed -n '<start>,<end>p' <file>
```

## Workflow

1. **Preflight ADO shape:**
   - Run `ado schema --list --output json`; before parsing any `--output json` command below, use the matching `ado schema <command-path>` entry when the response shape is not already known.
   - Run `ado wi types --output json`; abort if `Issue` or `Task` is missing and report available type names.
   - Run the Issue and Task `ado wi fields` commands. Record support for `System.Description`, `System.AssignedTo`, `System.IterationPath`, `Microsoft.VSTS.Common.Activity`, `Microsoft.VSTS.Scheduling.RemainingWork`, and `Microsoft.VSTS.Common.AcceptanceCriteria`.
   - Run `ado iteration current --output json`; use `.path` as the default iteration. If it fails, ask for an iteration path or approval to leave iteration unset.
   - Run `ado repo list --output json` only when the repo is not known.

2. **Hydrate an existing parent when given an ID:**
   - Run `ado wi view <wi-id> --output json` and extract title, description, acceptance criteria, type, state, assigned owner, and iteration from `.fields`.
   - Run `ado wi links <wi-id> --output json`; summarize existing child IDs and titles.
   - If children already exist, do not create duplicates unless the manager explicitly approves adding more.
   - If the existing parent is not an `Issue`, ask whether to update it as-is or create a new Issue. Never change its type.

3. **Interview for missing facts:**
   - Ask one or two questions at a time until outcome, current problem, target behavior, repos, three to five implementation slices, touchpoints, ordering, out-of-scope items, risks, QA validation, and rough estimates are clear.
   - Default assignment is `--assigned-to me` unless another owner is named.
   - Set remaining work only when the manager provides estimates or approves proposed estimates.
   - Use local read-only inspection only to remove ambiguity; do not edit product code.

4. **Draft the ticket set** per the Ticket Quality Bar above:
   - Parent `Issue`: outcome title, context, target behavior, scope, dependencies, rollout notes, out-of-scope notes, and high-level acceptance criteria.
   - Three to five development `Task` items: imperative title, first description line `Repo: <repo-name>`, implementation guidance, touchpoints, dependencies, out-of-scope notes, test expectations, `Development` activity, and task-specific acceptance criteria.
   - One QA `Task`: title `Validate <feature/outcome>`, validation goal, environment, test data/setup, steps, expected results, regression checks, evidence, `Testing` activity, and QA completion criteria.
   - Prefer vertical slices. Use technical-layer tasks only when dependency order requires it.

5. **Format fields** per the HTML field formatting section above:
   - Use `--acceptance-criteria` only when supported; otherwise fall back to a labeled section in `--description`.
   - Omit `--iteration`, `--field activity=...`, or `--field remaining-work=...` when unsupported or unapproved.

6. **Preview and approval gate:**
   - Show the full draft in this order: parent Issue, dev tasks in execution order, QA task, link plan, and field plan.
   - The field plan must name assignment, iteration, activity, remaining work, repo notation, and whether acceptance criteria use a dedicated field or description fallback.
   - If the manager asks for edits, revise and re-preview the affected sections.
   - Proceed only after a clear approval such as "approved" or "yes, create these tickets." A "yes" approves the previewed change only; subsequent changes need new approvals.

7. **Create or update the parent:**
   - For a new parent, run the approved `ado wi create --type Issue ... --output json` command and parse `.id`.
   - For an existing parent, run `ado wi update <parent-id> ...` with only changed and supported fields. Skip when no parent fields changed.

8. **Create child tasks:**
   - Create each dev task with the approved Task create command and parse `.id`.
   - Create the QA task with activity `Testing` and parse `.id`.
   - After every successful create, record ID, title, type, intended parent ID, and link status.

9. **Link hierarchy:**
   - For each child ID, run `ado wi link <parent-id> --child <child-id> --comment "Created from ticket breakdown workflow"`.
   - If a link fails, abort and report all unlinked child IDs. Do not delete created tickets.

## Failure Handling

ADO exit codes are stable: `0` success, `1` unclassified, `2` not found, `3` validation, `4` auth, `5` ADO API error. Any non-zero `ado` exit aborts with phase name, command, exit code, stderr, and created IDs so far. Specific cases:

- `2`: name the missing resource (e.g. `"ticket <wi-id> not found - check the ID."`).
- `4`: `"ADO auth failed - check the local ado setup."`
- `5`: surface stderr without reinterpretation.

Do not promise or attempt rollback for any ADO mutation. On partial mutation failure, report created/changed IDs, the phase, the exact failing command, and stderr. Do not continue after a failed write unless the manager explicitly gives a new repair instruction.

## Output

For a successful mutation, return:

```text
Created ticket breakdown:
Parent: #<id> <title>
Dev:
1. #<id> <title> - Repo: <repo> - Remaining work: <hours or unset>
2. #<id> <title> - Repo: <repo> - Remaining work: <hours or unset>
3. #<id> <title> - Repo: <repo> - Remaining work: <hours or unset>
QA: #<id> <title> - Remaining work: <hours or unset>
Omitted fields: <unsupported or unapproved fields, or none>
Warnings: <existing children, unset iteration, partial repair notes, or none>
```

For an abort, return the phase, failed command, exit code, stderr, and any parent or child IDs created before failure.
