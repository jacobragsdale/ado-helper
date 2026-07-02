---
name: ado-sprint-hydration
description: Use this skill when you need a read-only quality audit of an Azure DevOps sprint — flag tickets that are not ready to be picked up because they lack a description, acceptance criteria, an assignee, or an estimate.
disable-model-invocation: true
---

# ADO Sprint Hydration

Audit the tickets in one sprint and flag the ones that are not ready for a developer or QA analyst to
pick up. A ticket is under-hydrated when it lacks a usable description, acceptance criteria, an
assignee, or an estimate. This skill never mutates ADO — it only reads and reports.

The only input is an optional iteration reference plus optional scoping filters; it defaults to the
current sprint.

## Inputs

- `<iteration-ref>` (optional): iteration id or shorthand. Defaults to `@current`. Also accepts
  `@next`, `@previous`, or a concrete id.
- Optional scoping filters, passed straight through to `ado sprint backlog`, to narrow a large
  sprint: `--type`, `--state`, `--tag`, `--area`, `--unassigned`, `--top`.

## Preconditions

- `ado` CLI on `$PATH` and configured for the target Azure DevOps org, project, and team. Do not
  inspect, print, request, log, or manage `ADO_PAT`; treat it as opaque.
- All Azure DevOps access goes through the local `ado` CLI. Do not use the ADO web UI, raw
  undocumented REST calls, or guessed JSON shapes. Discover commands with `ado --help`,
  `ado <subcmd> --help`. Discover JSON shapes with `ado schema <command-path>`. Prefer
  `--output json` for any command this skill parses.
- Sprint commands are team-scoped. A team must resolve from `--team`, `ADO_TEAM`, or saved config
  (`ado team set <name>`). If no team resolves, the CLI errors — surface that message and stop.

## Command Surface

General discovery (run `ado <subcmd> --help` ad-hoc when a flag is unclear):

```sh
ado --help
```

JSON shapes (the workflow parses these commands' output):

```sh
ado schema sprint backlog
ado schema wi view
```

ADO reads (this skill performs no mutations):

```sh
ado sprint backlog --iteration <ref> [filters] --output json
ado wi fields --type "<work-item-type>" --output json
ado wi view <id> --output json
```

## Hydration bar

Flag a ticket if **any** of these is true:

- **Empty / thin description.** `System.Description` is missing, or — after stripping HTML tags,
  decoding entities, and collapsing whitespace — is shorter than the threshold (default **40
  characters**). State the threshold in the output so it is tunable.
- **Missing acceptance criteria.** `Microsoft.VSTS.Common.AcceptanceCriteria` is empty. If the work
  item type does not support that field (check with `ado wi fields --type "<type>"` for a
  `Microsoft.VSTS.Common.AcceptanceCriteria` entry), look for a labeled acceptance-criteria section
  inside the description instead; flag only if neither is present. If the concept does not apply to
  the type at all, mark `n/a` rather than flagging.
- **Unassigned.** `System.AssignedTo` is empty. Visible directly in the backlog data — no `wi view`
  needed.
- **No estimate.** Type-aware: Tasks need `RemainingWork` or `OriginalEstimate`; stories, issues,
  and bugs need `StoryPoints` or `Effort`. All of these are present in the backlog response, so this
  is decided before any `wi view` call.

## Workflow

1. **Resolve the iteration and enumerate.** Run
   `ado sprint backlog --iteration <ref> [filters] --output json`.

   IMPORTANT: `sprint backlog` defaults to `@next`, not the current sprint — always pass
   `--iteration <ref>` explicitly. Pass through any scoping filters the user supplied.

   Each entry in `.value` carries `id`, `work_item_type`, `title`, `state`, `assigned_to`,
   `story_points`, `effort`, `original_estimate`, `remaining_work`, `completed_work`, `tags`, `url`.

2. **Cheap pass — no extra calls.** From the backlog data alone, evaluate **Unassigned** and **No
   estimate** for every item using the type-aware rule above.

3. **Deep pass — one read per item.** The **Description** and **AC** checks require full fields, so
   run `ado wi view <id> --output json` per item and read `.fields["System.Description"]` and
   `.fields["Microsoft.VSTS.Common.AcceptanceCriteria"]`. Cache `ado wi fields --type "<type>"` per
   distinct type to avoid repeat lookups.

   This is N reads. Report the count before starting. If it is large (more than ~40 after filters),
   tell the user and suggest a scoping filter (`--type`, `--top`, `--unassigned`, …) instead of
   silently fanning out — proceed only if they confirm or the count is small.

4. **Print the terminal report.** Do not reprint full descriptions. Show a table of flagged tickets,
   sorted worst-first (most flags first):

   ```
   #<id> · <type> · <assignee or "unassigned"> · missing: <desc, AC, assignee, estimate>
   ```

   Follow with a one-line count of clean vs. flagged tickets.

## Failure Handling

ADO exit codes are stable: `0` success, `1` unclassified, `2` not found, `3` validation, `4` auth,
`5` ADO API error. Any non-zero `ado` exit aborts with the phase name, the command, the exit code,
and stderr verbatim. Specific cases:

- `2`: name the missing resource (e.g. `"iteration <ref> not found - check the reference."`).
- `3`: surface the validation message (commonly a missing team — `"no team configured - pass --team or run ado team set <name>."`).
- `4`: `"ADO auth failed - check the local ado setup."`
- `5`: surface stderr without reinterpretation.

This skill only reads, so there is nothing to roll back. If a single `ado wi view` fails mid-audit,
report the ids audited so far, the failing id and command, and stderr; do not retry blindly.

## Output

The final line is a one-line digest, with no formatting:

```
Hydration: <F>/<N> tickets need work (<desc> thin desc, <ac> missing AC, <un> unassigned, <est> no estimate).
```

On abort:

```
ABORT Phase <n> - <name>: <short reason>
```
