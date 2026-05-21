# qa-test-plan-workflow.md - sprint QA test plan aggregator

You are creating one sprint-level QA test plan from the sprint's Azure DevOps QA tasks. The output is a local markdown file only. This is a read-only planning workflow: do not create, update, comment on, or attach anything in ADO.

The source QA tasks are already expected to be well defined by the idea-to-ticket workflow. Your job is to aggregate them, remove repeated checks, group related testing by product area and user workflow, and produce an efficient execution order. The final plan should contain only test steps and expected outcomes. Do not include environment setup sections by default; QA runs against deployed instances.

## Input

- `<iteration-ref>` - optional. Accepts an ADO iteration id or shorthand such as `@current`, `@next`, or `@previous`. Default to `@current`.
- `<out-file>` - optional. Default to `qa-test-plan-<iteration-slug>.md`.

## Required Environment

- `ado` CLI on `$PATH`, authenticated via `.env` or saved config.
- A configured Azure DevOps organization, project, and team.

## Command Surface

Use only read-only `ado` commands:

- `ado iteration view <iteration-ref> --output json`
- `ado wi query --wiql "<query>" --output json`
- `ado wi view <id> --output json`
- `ado wi fields --type Task --output json` if Activity field support needs confirmation
- `ado schema <cmd-path>` if output shape is unclear

Do not use any mutating command: no `ado wi create`, `ado wi update`, `ado wi comment`, `ado wi attach`, `ado wi link`, or sprint planning mutations.

Important CLI behavior: `ado wi query --output json` filters correctly but hydrates only basic fields (`System.Id`, `System.Title`, `System.State`, `System.WorkItemType`, `System.AssignedTo`). After discovering QA task IDs, run `ado wi view <id> --output json` for each task to get descriptions, acceptance criteria, tags, relations, and Activity.

## Error Handling

Exit codes: `0` success, `2` not-found, `3` validation, `4` auth, `5` API error.

Any non-zero exit from a required `ado` command aborts with the phase name, command, exit code, and stderr. If a single task cannot be hydrated, report it as a warning and continue only if enough QA tasks remain to produce a useful plan.

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

Default output path: `qa-test-plan-<iteration-slug>.md`.

## Phase 2 - Find QA Tasks

Goal: find all sprint tasks whose Activity is Testing.

Use the resolved iteration path from Phase 1. Query by `Microsoft.VSTS.Common.Activity = 'Testing'`:

```sh
ado wi query --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.IterationPath] UNDER '<iteration path>' AND [System.WorkItemType] = 'Task' AND [Microsoft.VSTS.Common.Activity] = 'Testing' ORDER BY [System.AreaPath], [System.Title]" --output json
```

Notes:

- Keep the single quotes around `<iteration path>`, `Task`, and `Testing`.
- Escape any single quote in the iteration path by doubling it for WIQL.
- If the query returns an empty JSON array, write a short markdown file saying no QA testing tasks were found for the sprint, then stop.

For each returned item, record `.id` internally. The final test plan should not need ticket IDs unless the manager explicitly asks for a traceability appendix.

## Phase 3 - Hydrate QA Task Details

Goal: collect steps, expected outcomes, and product context.

For each QA task ID:

```sh
ado wi view <id> --output json
```

Extract from `.fields`:

- `System.Title`
- `System.Description`
- `Microsoft.VSTS.Common.AcceptanceCriteria` if present
- `System.State`
- `System.Tags`
- `System.AreaPath`
- `System.Parent` if present
- `Microsoft.VSTS.Common.Activity`

Inspect `.relations` for parent links:

- `System.LinkTypes.Hierarchy-Reverse` points from child task to parent.

When a parent ID can be discovered from `System.Parent` or relations, hydrate it for product context:

```sh
ado wi view <parent-id> --output json
```

Use parent details to understand product area and workflow, not to expand scope beyond the QA task.

## Phase 4 - Extract Candidate Checks

Goal: turn QA task content into normalized checks.

From each QA task, extract:

- Product area or user workflow.
- Test objective.
- Preconditions that are truly part of the steps. Do not create a separate environment setup section.
- Steps.
- Expected outcome.
- Edge cases.
- Regression checks.

Convert vague descriptions into executable checks only when the expected behavior is clearly implied by the task. If a task is too vague, include a warning in the final handoff and preserve the clearest available check rather than inventing product behavior.

## Phase 5 - Deduplicate and Group

Goal: make the plan efficient for QA to execute.

Deduplicate checks when they cover the same behavior, same user workflow, and same expected result. Merge duplicate steps by keeping the clearest version and preserving any unique edge case from the duplicates.

Do not deduplicate checks merely because they mention the same screen or product area. Keep separate checks when:

- Expected outcomes differ.
- User role or permissions differ.
- Data state differs materially.
- One is happy path and the other is failure/edge behavior.
- They validate different parent outcomes.

Group remaining checks by:

1. Product area.
2. User workflow inside the product area.
3. Risk or execution order within the workflow.

Avoid repo-based grouping unless the repo name is also a product-facing area. The team works across many repos, but QA should execute by product behavior.

## Phase 6 - Choose Execution Order

Goal: help QA move quickly through deployed instances.

Use this order unless the task content suggests a better one:

1. **Smoke checks** - fast checks that confirm the deployed instance is basically usable.
2. **High-risk workflows** - payment, auth, permissions, data loss, migration, critical customer flows, or blocked/high-priority work.
3. **Core workflow validation** - main feature behavior grouped by product area.
4. **Edge cases and failure states** - empty states, invalid input, permissions, retries, concurrency, offline/error behavior.
5. **Regression checks** - nearby workflows that could have been affected.

Within each group, order checks to minimize context switching: same product area, same role, same data setup, same screen, or same workflow should stay together.

## Phase 7 - Write the Markdown Test Plan

Goal: produce a concise QA plan with only steps and expected outcomes.

Use this structure:

```md
# QA Test Plan: <Sprint Name>

## Recommended Order
1. Smoke checks
2. High-risk workflows
3. Core workflow validation
4. Edge cases and failure states
5. Regression checks

## Smoke Checks
### <Product area / workflow>
Steps:
1. <step>
2. <step>

Expected outcome:
<expected result>

## High-Risk Workflows
### <Product area / workflow>
Steps:
1. <step>
2. <step>

Expected outcome:
<expected result>
```

Rules:

- Include only sections that have checks.
- Do not include environment setup sections by default.
- Do not include sign-off checkboxes.
- Do not include evidence placeholders.
- Do not include ticket IDs in the main test plan.
- Keep steps imperative and observable.
- Keep expected outcomes specific enough to pass/fail.
- Prefer one expected outcome block per check instead of scattered expected notes.

If the manager explicitly requests traceability, add a final `## Source Mapping` appendix with QA task IDs. Otherwise omit it.

## Phase 8 - Local Review Checks

Goal: catch output that would slow QA down.

Before finishing, scan the markdown for:

- Repeated or near-duplicate checks.
- Repo-based grouping that should be product/workflow grouping.
- Environment setup content.
- Sign-off checkboxes or evidence placeholders.
- Ticket IDs in the main body.
- Steps that are not actionable.
- Expected outcomes that are vague, such as "works correctly."

Revise until the plan is execution-ready.

## Final Output

Print:

```text
Wrote sprint QA test plan: <out-file>
Source sprint: <iteration path>
QA tasks reviewed: <count>
Deduplicated checks: <count removed or merged>
Warnings: <none or short list>
```

Do not print the full markdown body unless the manager asks for it.

## Guarantees the Workflow Makes

- Read-only ADO access only.
- Writes exactly one local markdown file.
- Finds QA tasks by `Microsoft.VSTS.Common.Activity = Testing`.
- Uses one sprint as the source scope.
- Groups tests by product area and user workflow.
- Includes a recommended execution order.
- Omits environment setup, sign-off checkboxes, evidence placeholders, and ticket IDs from the main plan.
