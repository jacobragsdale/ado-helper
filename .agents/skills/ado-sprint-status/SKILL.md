---
name: ado-sprint-status
description: Use this skill when you need a read-only overview of an Azure DevOps sprint — what tickets are in the iteration, what is completed / in progress / not started, what was added mid-sprint, and a per-assignee rollup.
disable-model-invocation: true
---

# ADO Sprint Status

Give a manager a fast, read-only picture of one sprint: every ticket in the iteration, grouped into
**completed / in progress / not started**, plus a per-assignee rollup and callouts for mid-sprint
additions and carryover. This skill never mutates ADO — it only reads.

The only input is an optional iteration reference; it defaults to the current sprint.

## Inputs

- `<iteration-ref>` (optional): an iteration id or shorthand. Defaults to `@current`. Also accepts
  `@next`, `@previous` (aliases `@now`, `@prev`), or a concrete iteration id.

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
ado schema sprint summary
ado schema wi states
```

ADO reads (this skill performs no mutations):

```sh
ado iteration view <ref> --output json
ado sprint backlog --iteration <ref> --output json
ado sprint summary --iteration <ref> --output json
ado wi states "<work-item-type>" --output json
```

## Workflow

1. **Resolve the iteration.** Use the supplied `<iteration-ref>`, else default to `@current`.
   Optionally confirm it with `ado iteration view <ref> --output json` to capture `name`,
   `attributes.startDate`, `attributes.finishDate`, and `attributes.timeFrame` for the header.
   If the iteration or team cannot resolve, abort with the CLI's message (exit `2`/`3`).

2. **List the iteration.** Run `ado sprint backlog --iteration <ref> --output json`.

   IMPORTANT: `sprint backlog` defaults to `@next`, not the current sprint — always pass
   `--iteration <ref>` explicitly.

   Each entry in `.value` already carries: `id`, `work_item_type`, `title`, `state`, `assigned_to`
   (`{display_name, unique_name, id}` or absent), `story_points`, `effort`, `original_estimate`,
   `remaining_work`, `completed_work`, `tags`, `area_path`, `url`. `Removed` items are already
   excluded. This is the answer to "what tickets are part of this iteration?".

3. **Bucket by state category.** Collect the distinct `work_item_type` values from `.value`. For
   each, run `ado wi states "<type>" --output json` once and build a `(type, state) -> category`
   map from `.value[].name` and `.value[].category`. Map ADO state categories to three buckets:

   - **Not started** = category `Proposed`
   - **In progress** = category `InProgress` or `Resolved`
   - **Completed** = category `Completed`

   If a state has an empty `category`, fall back to a name heuristic: `closed | done | completed |
   resolved` → Completed; otherwise In progress. This answers "what is completed, in progress, and
   not started?".

4. **Enrich with the summary.** Run `ado sprint summary --iteration <ref> --output json` and read:
   - `additions_mid_sprint` (work item ids injected after the sprint started) and
     `additions_mid_sprint_count`,
   - `carryover_count`,
   - `per_member` (`member`, `total_count`, `completed_count`, `carryover_count`).

5. **Print the terminal report.** Do not write any file. Do not dump full ticket bodies.

   - **Header:** `Sprint <name> · <start>→<finish> · <timeFrame> · <count> items`.
   - **Three state sections** (Not started / In progress / Completed). Each is a compact table:
     `#<id> · <type> · <assignee or "unassigned"> · <points or hours> · <title>`. Show story points
     for stories/issues/bugs and hours (remaining/original) for tasks, whichever the item carries.
   - **By-assignee rollup:** one row per person — completed / in progress / not-started counts.
   - **Callouts:** mid-sprint additions (ids + titles, resolved from the backlog list) and the
     carryover count.

## Failure Handling

ADO exit codes are stable: `0` success, `1` unclassified, `2` not found, `3` validation, `4` auth,
`5` ADO API error. Any non-zero `ado` exit aborts with the phase name, the command, the exit code,
and stderr verbatim. Specific cases:

- `2`: name the missing resource (e.g. `"iteration <ref> not found - check the reference."`).
- `3`: surface the validation message (commonly a missing team — `"no team configured - pass --team or run ado team set <name>."`).
- `4`: `"ADO auth failed - check the local ado setup."`
- `5`: surface stderr without reinterpretation.

This skill only reads, so there is nothing to roll back. If `wi states` fails for one type, fall back
to the name heuristic for that type's items rather than aborting the whole report; note the
degradation in the output.

## Output

The final line is a one-line digest, with no formatting:

```
Sprint <name>: <N> items — <done> done / <wip> in progress / <todo> not started; <A> added mid-sprint, <C> carryover.
```

On abort:

```
ABORT Phase <n> - <name>: <short reason>
```
