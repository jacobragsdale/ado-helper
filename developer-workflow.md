# developer-workflow.md — single-shot ADO dev cycle

You are executing one developer's end-to-end SDLC loop against an Azure DevOps work item. You are given a single argument — `<wi-id>` — and the current working directory is the product repo the work will land in. Everything that touches ADO goes through the local `ado` CLI; everything that touches code goes through normal git + the repo's own toolchain.

Follow the phases below in order. Do not skip ahead. If any phase says "abort," stop work and report exactly where you stopped — leave ADO state as you found it for any phase you did not reach.

## Input

- `<wi-id>` — the ADO work item ID, integer.

## Required environment

- `ado` CLI on `$PATH`, authenticated via `.env` or saved config.
- CWD is a git repository with an `origin` remote.

## Branch naming

Branches are named `main-jragsd-<wi-id>-<slug>`, where `<slug>` is derived from the (final) work item Title in Phase 6. Including `<wi-id>` guarantees uniqueness across tickets and makes the link visible in `git log` and tab-completion.

## Command surface (refer to these if you need exact flags)

The workflow runs against the compiled `ado` binary; this repo's source is not on disk at runtime. Discover commands and flags via `--help`, and JSON output shapes via `ado schema`.

- `ado --help` — top-level subcommands and global flags (`--org`, `--project`, `--output`, `--explain`).
- `ado wi --help`, then `ado wi <subcmd> --help` for the ones this workflow uses: `view`, `update`, `states`, `attach`. The update subcommand's `--help` lists every field alias (`--title`, `--description`, `--acceptance-criteria`, `--state`, `--field <NAME=VALUE>`, …).
- `ado pr --help`, then `ado pr <subcmd> --help` for `create`, `view`, `list`, `link-work-item`.
- `ado schema <cmd-path>` — JSON output schema for any command (e.g. `ado schema wi view`, `ado schema pr create`). Use this to confirm field paths like `.fields["System.State"]` or `.id` before parsing.

If a command behaves unexpectedly, re-read its `--help` before assuming the workflow is wrong — flags can be added between releases.

## Error handling

Exit codes: `0` success, `2` not-found, `3` validation, `4` auth, `5` API error.

Any non-zero exit from an `ado` invocation aborts the workflow with: the phase name, the command, the exit code, and the stderr verbatim. Specific cases get a more actionable message:

- `2`: name the resource that wasn't found (`"ticket <wi-id> not found — check the ID."`).
- `4`: `"ADO auth failed — check ADO_PAT in .env."`
- `5`: surface the stderr; ADO API errors are usually self-explanatory (state name invalid for type, branch already has a PR, etc.).

ADO state is never partially rolled back. The final line of stdout is always either an abort message naming the phase, or the PR URL — nothing else.

---

## Phase 1 — Hydrate the ticket

Goal: pull the current ticket state so the rest of the workflow has ground truth.

```sh
ado wi view <wi-id> --output json
```

Extract from `.fields`:

- `System.Title`
- `System.Description`  (HTML)
- `Microsoft.VSTS.Common.AcceptanceCriteria`  (HTML)
- `System.State`
- `System.WorkItemType`
- `System.AssignedTo.displayName`

Also extract `.relations` — Phase 3 uses it to detect an already-linked PR.

For display, convert Description and AC to plain text (strip tags, decode entities, collapse whitespace). Truncate the Description preview to 200 chars with a trailing `…` if longer; printing raw HTML to the developer is unreadable.

Print a one-screen summary:

```
Ticket #<id> · <type> · <state> · assigned to <name>
Title:        <title>
Description:  <plain-text, ≤200 chars>
AC:           <bullet list of existing AC items, or "(empty)">
```

Capture the work item type — Phase 6 needs it to pick a valid state transition.

---

## Phase 2 — Preflight (abort on any failure, no ADO mutation)

Run preflight before any ADO write *and* before the interview. An interview discarded because the local repo wasn't ready is wasted developer time; a half-updated ticket because preflight ran late is worse. The first failure aborts; do not attempt to fix anything for the developer.

1. **On main:**
   ```sh
   git rev-parse --abbrev-ref HEAD
   ```
   Must equal `main`. Otherwise abort: "you're on `<branch>` — switch to main and re-run."

2. **Clean working tree:**
   ```sh
   git status --porcelain
   ```
   Must be empty. Otherwise abort with the porcelain output and: "commit, stash, or discard these changes, then re-run."

3. **In sync with origin (not behind, not ahead):**
   ```sh
   git fetch origin main
   git rev-list --left-right --count origin/main...HEAD
   ```
   Output is `<behind>\t<ahead>`. Both must be `0`.
   - If behind > 0: "main is behind origin/main by N commits — `git pull` and re-run."
   - If ahead > 0: "main is *ahead* of origin/main by N commits — those shouldn't be on main. Investigate (`git log origin/main..HEAD`) before re-running."

---

## Phase 3 — Resumability check

Goal: detect that the ticket is already in flight and avoid re-interviewing or stomping on existing work.

1. **Existing PR.** Scan `.relations` (from Phase 1) for any entry pointing at a pull request — typically `rel == "ArtifactLink"` with the URL embedding `vstfs:///Git/PullRequestId/…`. For any such PR, run `ado pr view <pr-id> --repo <repo> --output json` and check its status. If any are open or active, abort: "ticket already has an open PR — `<web-url>`. Update or abandon that PR rather than running this workflow again."

2. **Existing branch.** Look for branches matching `main-jragsd-<wi-id>-*`:
   ```sh
   git branch --list 'main-jragsd-<wi-id>-*'
   git ls-remote --heads origin 'main-jragsd-<wi-id>-*'
   ```
   - **Exactly one (local or remote).** Print: "ticket has an existing branch `<name>` from a prior run — resume from implementation? [y/n]". On `y`: check it out (fetching from origin if only the remote has it), then if `.ado/<wi-id>.design.md` is missing locally, pull the design attachment from the work item with `ado wi view`'s relations into that path. Skip directly to Phase 7. On `n`: abort.
   - **Multiple.** List them and abort: "more than one in-flight branch for this ticket — resolve manually."

3. **In-progress state without a branch.** If `System.State` is one of {`Doing`, `Active`, `In Progress`, `Committed`, `Started`} and step 2 found no branch, warn: "ticket is in-progress but no matching branch was found. Either someone else is working on it, or a prior run aborted before branch creation. Proceed with a fresh setup? [y/n]". Continue on `y`, abort on `n`.

If none of the above triggers, continue.

---

## Phase 4 — Interview (medium depth)

Goal: get from the developer the implementation details that turn the ticket into a concrete plan and a testable AC.

Show the existing AC verbatim first so they can react to it rather than start from blank. Then walk through these five questions in a normal conversation — one at a time, not as a wall of text. Use `AskUserQuestion` only when you're offering a choice between concrete alternatives; otherwise just ask in prose.

1. **Approach** — what's the chosen implementation strategy at a high level?
2. **Locations** — which files / modules / layers will be touched?
3. **Success criteria** — what observable behavior defines done? (this drives the new AC)
4. **Edge cases / risks** — what could go wrong; anything to deliberately not handle?
5. **Test strategy** — unit / integration / manual; concrete test names if known.

Add 1–2 follow-ups only when an answer is unclear or the ticket Description was unusually thin. Don't pad — five sharp questions beats nine soft ones.

---

## Phase 5 — Persist and write back

Goal: capture the refined understanding in three places — a local design note for resumability, the ADO fields as the contract for the rest of the workflow, and an attachment on the ticket so the design survives a wiped working tree.

1. **Compose updated values.** Treat all developer-supplied prose as plain text content; the only HTML tags you generate are structural (`<p>`, `<ul>`, `<li>`, `<strong>`, `<em>`). Escape `&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`, `"` → `&quot;`, `'` → `&#39;` in prose before inserting into tags. The `ado` CLI passes `--description` and `--acceptance-criteria` through to ADO unchanged — if you don't escape, a stray `&` or `<` will break the rendered field.

   - **Title:** plain text. Keep existing unless the interview produced a clearly better one.
   - **Description:** HTML. Restate problem + chosen approach in one or two short `<p>…</p>` paragraphs.
   - **Acceptance Criteria:** HTML. One `<li>` per criterion inside a `<ul>`. Prefer Given/When/Then phrasing. Each item must be independently verifiable.

2. **Write a local design note** at `.ado/<wi-id>.design.md` (create `.ado/` if it doesn't exist, and add `.ado/` to `.gitignore` if not already ignored — it must not be committed). Markdown, not HTML. Sections:
   - `# <wi-id> — <title>`
   - `## Approach` — Q1
   - `## Touchpoints` — Q2
   - `## Acceptance Criteria` — Q3 as a markdown bullet list (mirrors what's going into the AC field)
   - `## Risks` — Q4
   - `## Test strategy` — Q5

3. **Show the developer a diff-style preview** for each field that changed:

   ```
   Title:
     - <old>
     + <new>

   AC:
     - <old bullets>
     + <new bullets>
   ```

   Wait for an explicit "yes" before writing. If they want edits, revise the design note and the field values, then re-show.

4. **Apply the field updates in one call.** Omit any flag for a field that didn't change. If nothing changed, skip this step entirely.

   ```sh
   ado wi update <wi-id> \
     --title "<new title>" \
     --description "<new html description>" \
     --acceptance-criteria "<new html ac>"
   ```

5. **Attach the design note** so a future resume can pull it even without the local file:

   ```sh
   ado wi attach <wi-id> .ado/<wi-id>.design.md --comment "design (auto-generated)"
   ```

Do not post any comment on the work item.

---

## Phase 6 — Branch and state transition

Goal: create the feature branch and mark the ticket in-progress. Local action first (cheap, reversible), then ADO write (visible, semantic). If the branch step fails, the ticket state is untouched.

1. **Derive the branch slug from the (final) Title:**
   - Lowercase.
   - Replace any run of non-`[a-z0-9]` characters with a single `-`.
   - Trim leading and trailing `-`.
   - Truncate to 40 characters (keeps tab-completion and `git log --oneline` readable); if the cut lands mid-word, back up to the previous `-`.

2. **Branch name:** `main-jragsd-<wi-id>-<slug>`. Create and check out:
   ```sh
   git checkout -b main-jragsd-<wi-id>-<slug>
   ```
   If this fails with "branch already exists", Phase 3's resume check missed it (race condition or stale state). Abort with the conflict; do not move to step 3.

3. **Pick a valid in-progress state for the work item type.** State names are template-specific (Agile/Scrum/CMMI all differ, and Bug-type items often diverge from Story-type within the same template). Query the allowed states:
   ```sh
   ado wi states "<type>" --output json
   ```
   From the returned list, pick the first match (case-insensitive) in this preference order: `Doing`, `Active`, `In Progress`, `Committed`, `Started`. If none match, abort with the full state list and: "no in-progress state recognised for type `<type>` — extend the preference order in developer-workflow.md."

4. **Transition state:**
   ```sh
   ado wi update <wi-id> --state "<picked state>"
   ```

---

## Phase 7 — Implementation (you drive end-to-end)

Goal: land the code that satisfies every AC item, with a reproducible protocol.

First, register one task per AC criterion via `TaskCreate`. All tasks at the start. The AC list is your work breakdown.

Then loop, one criterion at a time:

1. Mark the next pending task `in_progress`.
2. State your intended approach in ≤3 sentences before writing code. If the approach has materially shifted from Phase 4's interview, surface that to the developer and wait for a yes before proceeding.
3. Make the change. Stay inside the scope captured in Phase 4 — anything that grows beyond it must be surfaced to the developer, not absorbed silently.
4. Run the directly-relevant tests if they exist (the AC may have named some in Q5). If none exist yet, write the test alongside the change.
5. Commit using conventional commits with the work item embedded: `<type>(AB#<wi-id>): <criterion summary>`. Default cadence is one commit per AC item; combine only when changes are too entangled to split cleanly.
6. Mark the task `completed`.

Do not move to validation until every AC task is completed, or the developer has explicitly deferred one with a written reason recorded in `.ado/<wi-id>.design.md`.

---

## Phase 8 — Validation (auto-detect, auto-fix once, then bail)

Goal: prove the change builds, lints, and tests cleanly in this repo's own toolchain before pushing.

Detect the project type by looking at the repo root:

| Marker file        | Build / type-check                              | Lint / format (check, not rewrite)                                  | Tests             |
| ------------------ | ----------------------------------------------- | ------------------------------------------------------------------- | ----------------- |
| `Cargo.toml`       | `cargo build`                                   | `cargo fmt --check && cargo clippy --all-targets -- -D warnings`    | `cargo test`      |
| `package.json`     | `npm run build` (skip if no `build` script)     | `npm run lint` (skip if absent), else `prettier --check .`          | `npm test`        |
| `pyproject.toml`   | `python -m compileall .`                        | `ruff check && ruff format --check`                                 | `pytest`          |
| `go.mod`           | `go build ./...`                                | `gofmt -l . && go vet ./...`                                        | `go test ./...`   |

Note: `cargo fmt` and `gofmt -w` *rewrite* files and exit 0; they are not check commands. The table uses the check variants on purpose.

Rules:

- If a marker file isn't present, skip that family entirely. If multiple are present (polyglot repo), run all of them.
- If a specific stage doesn't apply (e.g., no lint script), say so explicitly in your output and move on — don't silently skip.
- On any failure, attempt **one** auto-fix pass with the obvious tool (`cargo fmt`, `ruff check --fix && ruff format`, `eslint --fix`, `gofmt -w .`) and re-run the failing stage.
- If the auto-fix changed files and the re-run passed, **commit the fix** before continuing:
  ```sh
  git add -A
  git commit -m "chore(AB#<wi-id>): lint autofix"
  ```
  A passing local check with uncommitted fixes is a lie — pushing the unfixed tree will fail CI.
- If the re-run still fails, **bail to the developer** with the full failure output. Do not push, do not open a PR. The ticket stays in its in-progress state; the branch stays local.

After all stages pass, print the AC items from `.ado/<wi-id>.design.md` and ask explicitly:

> Validation passed. Verify manually:
> 1. <ac item 1>
> 2. <ac item 2>
> …
> Confirm before I push.

Wait for an explicit yes. If they want changes, loop back to Phase 7.

---

## Phase 9 — Push, open the PR, return the URL

Goal: get the change up for review and link it to the ticket.

1. **Check the remote branch state.**
   ```sh
   git ls-remote --exit-code origin refs/heads/main-jragsd-<wi-id>-<slug>
   ```
   - Exit 0 (branch exists on origin): ask the developer whether to (a) push and let it fast-forward, (b) abort, or (c) force-push. Only allow (c) after verifying the local branch contains the remote's commits with `git merge-base --is-ancestor origin/<branch> HEAD`; refuse otherwise.
   - Exit 2 (does not exist): proceed.

2. **Push:**
   ```sh
   git push -u origin main-jragsd-<wi-id>-<slug>
   ```

3. **Determine the ADO repo name for `--repo`:**
   - Prefer `$ADO_REPO` if set.
   - Otherwise parse `git remote get-url origin` and take the final path segment, stripping `.git`. (The `ado` CLI uses the same rsplit logic internally when `--repo` is omitted, so passing it explicitly just makes failure messages easier to read.)

4. **Create the PR.** Title is the (final) ticket Title. Body is a 1–3 sentence summary of the change *plus* a textual back-reference to the work item — `link-work-item` handles ADO-side cross-linking, but the text reference is what shows up in search tools, audit exports, and mirror repos. Pass `--output json` so the PR ID can be parsed reliably:

   ```sh
   ado pr create --repo <repo> --title "<ticket title>" --target main \
     --description "<short summary>\n\nWork item: AB#<wi-id>" \
     --output json
   ```

   Parse `.id` from the JSON response — that's the PR ID for the next two steps. (`ado pr create` returns the API URL in `.url`, not the human-facing web URL, which is why step 6 calls `pr view`.)

5. **Link the work item natively:**
   ```sh
   ado pr link-work-item <pr-id> --repo <repo> --work-item <wi-id>
   ```

6. **Resolve the human-facing PR URL:**
   ```sh
   ado pr view <pr-id> --repo <repo> --output json
   ```
   Pull the web URL field and print it as the final line of your output, on its own, with no other formatting. That's the handoff.

---

## Guarantees the workflow makes

- State transitions performed by the workflow: `To Do → <in-progress state>` only (Phase 6 picks the state name from the work item type). Closing to Done is the developer's call after merge.
- No comments are ever posted to the work item.
- ADO is mutated only after the local repo has passed preflight (Phase 2) and the developer has explicitly confirmed the field diff (Phase 5).
- On abort, ADO state is left exactly as it was at the point of failure — never partially rolled back. The local `.ado/<wi-id>.design.md` (and its attachment on the ticket) is the recovery artifact for resuming the interview.
- The final line of stdout is always either an aborted-phase message or the PR URL.
