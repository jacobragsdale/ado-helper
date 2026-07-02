Note: Mark checklist items as completed (`[x]`) when the feature is implemented, documented, and covered by appropriate tests or smoke checks.

# Vision

`ado` is the one-shot CLI an LLM agent uses to drive my Azure DevOps work as an engineering manager. Every feature is a discrete subcommand with stable flags, clean `--output json`, and predictable exit codes so an agent can chain them into higher-level workflows (sprint planning, standup digests, release notes). Default scope is "just me, current iteration"; flags widen to team or arbitrary iteration. No external integrations — ADO REST APIs only.

# Foundation

Primitives every higher-level command depends on. Build these first so the rest can compose them.

- [x] Add `ado me`
  - Resolve the caller's identity via the Profile/Identity API (descriptor, display name, unique name, id).
  - Used as the default `--assigned-to` and as the implicit subject of "my" commands.
  - Cache to config so other commands can read it without an extra round-trip.

- [x] Add `ado team` commands
  - `ado team list`, `ado team current`, `ado team members`, `ado team set <name>`.
  - Use Core Teams API; persist `--team` default via `ado config set --team`.
  - Required by every iteration, capacity, and board command below.

- [x] Add `ado iteration` commands
  - `ado iteration list`, `ado iteration current`, `ado iteration next`, `ado iteration view <id|@current|@next>`.
  - Use Work/TeamSettings/Iterations API.
  - Show id, name, path, start/finish date, time-frame (past/current/future).
  - Accept `@current` / `@next` / `@previous` shorthands everywhere an iteration is taken.

- [x] Add `ado area` commands
  - `ado area list`, `ado area tree` (use Classification Nodes API with depth).
  - Output should be paste-ready into `--area` / `--field area=...`.

- [x] Add `ado wi` metadata discovery
  - `ado wi types`, `ado wi states <type>`, `ado wi fields [--type ...]`.
  - Use Work Item Tracking metadata APIs; output paste-ready values for `--type`, `--state`, `--field`.

- [x] Standardize agent-friendly output across all commands
  - Guarantee `--output json` returns a stable, documented schema for every command (not just the raw API response).
  - Add `--quiet` (no banners/colors) and consistent exit codes: `0` success, `2` not-found, `3` validation, `4` auth, `5` API error.
  - Document the schema contract in `README.md` so agents can rely on it.

# Sprint & Iteration Planning (priority)

The top-priority area. Goal: an agent can drive a full sprint planning session — pull capacity, see the backlog, assign work, set the goal, roll over carryover — without touching the web UI.

- [x] Add `ado sprint backlog`
  - List candidate work items for an iteration: `--iteration @next` by default.
  - Filter by `--type`, `--state`, `--tag`, `--area`, `--unassigned`, `--top N`.
  - Show id, type, title, state, assigned-to, story points/effort, tags.

- [x] Add `ado sprint board`
  - Render the team board for an iteration: columns × work items.
  - Text mode: column headers with item counts; table mode: items grouped by column; json: full structure.
  - Use Work Boards API.

- [x] Add `ado sprint plan-into`
  - `ado sprint plan-into <wi-id...> --iteration @next` sets `System.IterationPath` on one or more items in a single call.
  - Accept ids via args or stdin (so an agent can pipe results from `ado wi query`).
  - Support `--assigned-to` and `--state` to set in the same operation (one round-trip per item).

- [x] Add `ado sprint capacity`
  - `ado sprint capacity --iteration @current` shows per-member capacity, days off, activity buckets.
  - `ado sprint capacity set --member <id> --hours-per-day 6 --activity Development` writes capacity for the configured team.
  - Use Work/TeamSettings/Iterations/Capacities API.

- [ ] Add `ado sprint goal`
  - **Blocked on stock ADO API:** no verified first-party Azure DevOps Work/WIT REST endpoint was found for sprint goal text. Keep this unchecked unless a stock endpoint is confirmed, or intentionally choose a different storage model such as a goal work item or extension API.

- [x] Add `ado sprint burndown`
  - Pull remaining-work totals across the iteration's items, grouped by day from start to today.
  - Text: ASCII sparkline + numbers; json: array of `{date, remaining_hours, completed_hours, scope_hours}`.
  - Optional `--by member` to break out per-engineer.

- [x] Add `ado sprint rollover`
  - Move unfinished items from `@current` (or any iteration) to `@next`.
  - `--dry-run` lists what would move; `--state-filter "Active,New"` controls which items; `--reset-remaining` optional.
  - Post a comment on each moved item linking to the rollover summary.

- [x] Add `ado sprint summary`
  - End-of-sprint snapshot: planned vs. completed points/hours, carryover count, additions mid-sprint (items whose iteration was changed during the sprint), per-member breakdown.
  - Pair with `ado sprint burndown` to feed retro prep.

# My Work & Time Tracking

Default scope is me. These should be the lowest-friction commands in the tool.

- [ ] Add `ado my queue`
  - One-shot "what's on my plate now": active items assigned to me in `@current`, sorted by state (Doing → To Do), then priority/stack-rank.
  - Flags: `--iteration`, `--state`, `--include-blocked`.

- [ ] Add `ado my prs`
  - PRs I authored (active) + PRs awaiting my review. Show id, repo, title, status, vote summary, age.
  - One-shot version of "should I review something today?".

- [ ] Add `ado my time log`
  - **Blocked on info:** time tracking is handled by a custom ADO plugin, not the stock `CompletedWork`/`RemainingWork` fields. Need details from the user before designing this: plugin name/vendor, REST endpoints (or extension API surface), auth model (same PAT scopes?), the data shape it stores (custom fields on the WI? separate entity?), and whether it exposes read + write or write-only.
  - Tentative shape: `ado my time log <wi-id> --hours 1.5 [--note "..."]` writes a time entry via the plugin's API.
  - `--date YYYY-MM-DD` to backfill; defaults to today.

- [ ] Add `ado my time today` / `ado my time week`
  - **Blocked on the same plugin info above** — read-side queries depend on what the plugin exposes (per-user entries, date filters, aggregation).
  - Tentative shape: roll up entries authored by me across a date range, grouped by work item; show hours per item and total.

- [ ] Add `ado my focus`
  - "What should I pick up next" helper: highest stack-rank Doing item in `@current` assigned to me; falls back to To Do. Returns one work item id in text mode for easy chaining.

# Team Progress & Standup

Same scope flag (`--team` or default team) on everything; "just me" is a `--mine` opt-in for the team commands.

- [ ] Add `ado team status`
  - One row per member: active item, blocked count, PRs open, PRs awaiting their review.
  - Pulls from `wi list` + PR list filtered by author/reviewer.

- [ ] Add `ado team standup`
  - Per-member digest of changes since a cutoff (`--since 24h` default): state transitions, comments added, PRs opened/merged, items closed.
  - Reads work item revisions + PR list; output is a markdown digest by default (paste-ready into chat manually — no external posting).

- [ ] Add `ado team blocked`
  - Items tagged `blocked` or in a Blocked state, or items with no activity for N days, across the team's `@current` iteration.

- [ ] Add `ado team aging`
  - Active items grouped by age in current state. Highlights items that have been Doing for > N days.
  - `--threshold 3d` to tune; output sorted descending by age.

- [ ] Add `ado team throughput`
  - Closed items per member over the last N sprints (or a date range). Counts and points if available.
  - Useful for forecasting and retro inputs.

- [ ] Add `ado team review-queue`
  - Active PRs across the team's repos, with each PR's age, vote rollup, and reviewer list.
  - `--awaiting <member>` filters to PRs blocked on a specific person.

# Release Notes & Reporting

Generate markdown from ADO state. No external posting — write to stdout or `--out file.md`.

- [ ] Add `ado notes iteration`
  - `ado notes iteration @current --out release-notes.md` generates a markdown release-notes draft from completed work items in the iteration.
  - Group by work item type (Feature → User Story → Bug); include id, title, and a link to the work item.
  - `--include-prs` cross-references merged PRs linked to those items.

- [ ] Add `ado notes range`
  - Same idea but bounded by `--from <date> --to <date>` or `--query <wiql>` for ad-hoc reports.
  - Used for cross-sprint releases or hotfix batches.

- [ ] Add `ado notes prs`
  - Markdown summary of merged PRs in a date range across one or more repos.
  - Useful when work items don't tell the whole story (infra, refactors).

# Polish for Agent Use

Quality-of-life that makes the CLI safer to drive from an LLM.

- [x] Stdin batching for mutation commands
  - `ado wi update`, `ado sprint plan-into`, `ado pr link-work-item` should accept ids on stdin (one per line or JSON array) so agents can pipe `ado wi query` output in.
  - Done for `wi update` and `pr link-work-item`; `sprint plan-into` will reuse the same `stdin_ids::read_ids` helper when that command lands.

- [x] `--explain` flag on mutation commands
  - Prints the exact REST call(s) that would be made and exits non-zero without performing the mutation. Lets an agent (or me) dry-run anything destructive.
  - Implemented as a global flag; `--explain` dry-runs exit `0` (success) so agents can verify the planned call without branching on a failure code.

- [x] Schema docs command
  - `ado schema <command>` prints the JSON output schema for that command. Lets agents introspect without scraping the README.

# Deferred Skills (revisit after foundation is stable)

These skills were removed on 2026-05-24 to focus the starting set on the ticket → refine → developer loop. Original specs are recoverable from git history.

- [ ] **Re-introduce `ado-qa-test-plan-workflow` skill.** Sprint-level QA test plan generator from QA Testing tasks (read-only ADO; outputs local markdown). Spec previously at `.agents/skills/ado-qa-test-plan-workflow/SKILL.md` and `qa-test-plan-workflow.md`.
- [ ] **Re-introduce `ado-product-update-workflow` skill.** Read-only sprint → stakeholder markdown generator. Spec previously at `.agents/skills/ado-product-update-workflow/SKILL.md` and `product-update-workflow.md`.
- [ ] **Sprint-management skill family** — planning (capacity-aware), standup digest, monitoring (aging/blocked), retrospective. Build after the foundation skills (`ado-ticket-workflow`, `ado-refine-ticket`, `ado-developer-workflow`) are settled. See `ideas.md` for the full menu.
