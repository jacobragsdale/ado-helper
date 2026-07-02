---
name: ado-developer-workflow
description: Use this skill when running the end-to-end implementation loop for a single Azure DevOps work item — preflight, branch + state transition, code, validate, push, PR, link. Assumes the ticket is already refined via /ado-refine-ticket.
disable-model-invocation: true
---

# ADO Developer Workflow

Run one end-to-end implementation loop for a single Azure DevOps work item. The only input is `<wi-id>`, an integer work item ID. The current working directory must be the product repo where the change will land.

This skill assumes the ticket has already been refined and a local design note exists. For refinement, run `/ado-refine-ticket <wi-id>` first.

## Inputs

- `<wi-id>`: ADO work item ID (integer).

## Preconditions

- `ado` CLI on `$PATH` and configured for the target Azure DevOps org, project, and team. Do not inspect, print, request, log, or manage `ADO_PAT`; treat it as opaque.
- All Azure DevOps access goes through the local `ado` CLI. Do not use the ADO web UI, raw undocumented REST calls, or guessed JSON shapes. Discover commands with `ado --help`, `ado <subcmd> --help`. Discover JSON shapes with `ado schema <command-path>`. Prefer `--output json` for any command this skill parses.
- Current working directory is a git repository with an `origin` remote and target branch `main`.
- **Ticket is already refined.** Title, Description, and Acceptance Criteria are populated, and `.ado/<wi-id>.design.md` exists locally. If the design note is missing, abort with: `Run /ado-refine-ticket <wi-id> first.`

## Command Surface

General discovery (run `ado <subcmd> --help` ad-hoc when a flag is unclear):

```sh
ado --help
```

JSON shapes (the workflow parses these commands' output):

```sh
ado schema wi view
ado schema wi states
ado schema pr create
```

ADO reads:

```sh
ado wi view <wi-id> --output json
ado wi states "<work-item-type>" --output json
ado pr view <pr-id> --repo <repo> --output json
```

ADO mutations (run only at the gates named in the workflow):

```sh
ado wi update <wi-id> --state "<picked-state>"
ado pr create --repo <repo> --title "<ticket-title>" --target main \
  --description "<summary>\n\nWork item: AB#<wi-id>" --output json
ado pr link-work-item <pr-id> --repo <repo> --work-item <wi-id>
```

Git commands:

```sh
git rev-parse --abbrev-ref HEAD
git status --porcelain
git fetch origin main
git rev-list --left-right --count origin/main...HEAD
git log origin/main..HEAD
git remote get-url origin
git branch --list 'main-jragsd-<wi-id>-*'
git ls-remote --heads origin 'main-jragsd-<wi-id>-*'
git fetch origin refs/heads/<branch>:refs/remotes/origin/<branch>
git checkout <branch>
git checkout -b <branch> --track origin/<branch>
git checkout -b main-jragsd-<wi-id>-<slug>
git add -A
git commit -m "<type>(AB#<wi-id>): <summary>"
git ls-remote --exit-code origin refs/heads/<branch>
git merge-base --is-ancestor origin/<branch> HEAD
git push -u origin <branch>
git push --force-with-lease -u origin <branch>
```

## Branch and commit conventions

- **Branch name:** `main-jragsd-<wi-id>-<slug>`.
- **Slug derivation** from the final work item title:
  - Lowercase.
  - Replace any run of non-`[a-z0-9]` characters with a single `-`.
  - Trim leading/trailing `-`.
  - Truncate to 40 characters; if the cut lands mid-word, back up to the previous `-`.
- **Commit format:** `<type>(AB#<wi-id>): <summary>` using conventional commit types (`feat`, `fix`, `chore`, `refactor`, `docs`, `test`).
- **Lint autofix commits:** `chore(AB#<wi-id>): lint autofix`.
- Default cadence is one commit per acceptance criterion. Combine only when changes cannot be cleanly split.

## Validation toolchain

Detect the project type from marker files at the repo root. Run every family whose marker is present. Skip silently absent families. If a family is present but a stage doesn't apply (e.g. no `lint` script), say so explicitly in output — don't silently skip.

| Marker file      | Build / type-check                          | Lint / format (check, not rewrite)                               | Tests           | Autofix                                          |
| ---------------- | ------------------------------------------- | ---------------------------------------------------------------- | --------------- | ------------------------------------------------ |
| `Cargo.toml`     | `cargo build`                               | `cargo fmt --check && cargo clippy --all-targets -- -D warnings` | `cargo test`    | `cargo fmt`                                      |
| `package.json`   | `npm run build` (skip if no `build` script) | `npm run lint` (skip if absent), else `prettier --check .`       | `npm test`      | `npx eslint --fix . && npx prettier --write .`   |
| `pyproject.toml` | `python -m compileall .`                    | `ruff check && ruff format --check`                              | `pytest`        | `ruff check --fix && ruff format`                |
| `go.mod`         | `go build ./...`                            | `gofmt -l . && go vet ./...`                                     | `go test ./...` | `gofmt -w .`                                     |

`cargo fmt` and `gofmt -w` rewrite files and exit `0`; they are autofix commands, not check commands. The Lint/format column uses the check variants on purpose.

**Failure protocol.** On a stage failure, attempt **one** autofix pass for that family and re-run the failing stage. If the rerun passes and the autofix changed files, commit them with `chore(AB#<wi-id>): lint autofix` before continuing — a passing local check with uncommitted fixes will fail CI. If the rerun still fails, bail without pushing or opening a PR.

## Local artifact rules

- The `.ado/` directory at the repo root holds local design notes and is git-ignored. It must never be committed.
- The per-ticket design note lives at `.ado/<wi-id>.design.md` and is produced by `/ado-refine-ticket`. Sections: `## Approach`, `## Touchpoints`, `## Acceptance Criteria`, `## Risks`, `## Test strategy`.

## Workflow

1. **Hydrate the ticket.** Run `ado wi view <wi-id> --output json`. Extract from `.fields`: `System.Title`, `System.Description`, `Microsoft.VSTS.Common.AcceptanceCriteria`, `System.State`, `System.WorkItemType`, `System.AssignedTo.displayName`. Also extract `.relations` (step 3 uses it for PR detection). Print a one-screen summary with HTML converted to plain text. Capture the work item type — step 4 needs it for state transitions.

2. **Preflight (abort on first failure, no ADO mutation).**
   - `git rev-parse --abbrev-ref HEAD` must equal `main`. Otherwise abort with `"you're on <branch> — switch to main and re-run."`
   - `git status --porcelain` must be empty. Otherwise abort with the porcelain output and `"commit, stash, or discard these changes, then re-run."`
   - Run `git fetch origin main`, then `git rev-list --left-right --count origin/main...HEAD`. Both behind and ahead must be `0`. If behind: `"main is behind origin/main by N commits — git pull and re-run."` If ahead: `"main is ahead of origin/main by N — investigate (git log origin/main..HEAD)."`

3. **Resumability check.** Resolve `<repo>` from `$ADO_REPO`, else parse the final path segment of `git remote get-url origin` and strip `.git`.
   - **Existing PR.** Scan ticket `.relations` for pull request artifact links. For each PR ID, run `ado pr view <pr-id> --repo <repo> --output json`. If any PR is open or active, abort with its web URL: `"ticket already has an open PR — <web-url>. Update or abandon that PR rather than running this workflow again."`
   - **Existing branch.** Check `git branch --list 'main-jragsd-<wi-id>-*'` and `git ls-remote --heads origin 'main-jragsd-<wi-id>-*'`.
     - **Exactly one** (local or remote): ask `"ticket has an existing branch <name> from a prior run — resume from implementation? [y/n]"`. On yes, check it out locally (fetching and tracking `origin/<branch>` if needed), then verify `.ado/<wi-id>.design.md` is present. If it's missing, abort: `"design note missing on resume. Run /ado-refine-ticket <wi-id> first."` Continue at step 5.
     - **Multiple**: list them and abort `"more than one in-flight branch for this ticket — resolve manually."`
   - **In-progress state without a branch.** If `System.State` is one of `{Doing, Active, In Progress, Committed, Started}` and no branch exists, warn `"ticket is in-progress but no matching branch found. Proceed with fresh setup? [y/n]"`. Continue on yes; abort on no.
   - **Design note check (fresh path).** If no resume occurred, verify `.ado/<wi-id>.design.md` is present. If missing, abort: `"Run /ado-refine-ticket <wi-id> first."`

4. **Branch and state transition.** Derive `<slug>` per the Branch and commit conventions section above. Run `git checkout -b main-jragsd-<wi-id>-<slug>` (if the branch already exists, step 3 missed a race — abort with the conflict). Then run `ado wi states "<work-item-type>" --output json` and pick the first case-insensitive match from `Doing`, `Active`, `In Progress`, `Committed`, `Started`. If none match, abort with the state list. Otherwise run `ado wi update <wi-id> --state "<picked-state>"`. This is the first ADO mutation gate; the state change is approved implicitly by running this skill on a refined ticket.

5. **Implementation.** Create one task per AC criterion in the agent task tracker before coding. For each criterion:
   - State the intended approach in ≤3 sentences. If the approach has materially shifted from the design note, surface that to the developer and wait for an explicit yes before proceeding; update `.ado/<wi-id>.design.md` to match.
   - Make the change. Stay inside the scope captured in the design note.
   - Run directly-relevant tests if they exist; write them alongside the change if they don't.
   - Commit using conventional commits per the conventions section: `<type>(AB#<wi-id>): <criterion summary>`. Default cadence is one commit per AC item.

6. **Validation.** Use the Validation toolchain section: detect markers at repo root, run every matching family's build/lint/test stages, attempt one autofix pass on failure, commit autofixes if they pass, and bail without pushing if a rerun still fails. When all checks pass, print the AC list from `.ado/<wi-id>.design.md` and ask: `"Validation passed. Verify manually: ... Confirm before I push."` Wait for explicit yes.

7. **Push and open the PR.** This is the second ADO mutation gate.
   - `git ls-remote --exit-code origin refs/heads/<branch>`. If the remote branch exists, ask whether to fast-forward push, abort, or force-push. Before force-push, fetch the remote branch and require `git merge-base --is-ancestor origin/<branch> HEAD`; otherwise refuse.
   - Push with `git push -u origin <branch>`, or — only after approval and the ancestry check — `git push --force-with-lease -u origin <branch>`.
   - Create the PR:

     ```sh
     ado pr create --repo <repo> --title "<ticket-title>" --target main \
       --description "<summary>\n\nWork item: AB#<wi-id>" --output json
     ```

     Parse `.id`. The textual `Work item: AB#<wi-id>` back-reference is what shows up in search tools, audit exports, and mirror repos — keep it even though `link-work-item` handles ADO-side cross-linking.

   - Link the work item: `ado pr link-work-item <pr-id> --repo <repo> --work-item <wi-id>`.
   - Resolve the human-facing PR web URL: `ado pr view <pr-id> --repo <repo> --output json`. (The `.url` from `pr create` is the API URL, not the web URL.)

## Failure Handling

ADO exit codes are stable: `0` success, `1` unclassified, `2` not found, `3` validation, `4` auth, `5` ADO API error. Any non-zero `ado` exit aborts with the phase name, the command, the exit code, and stderr verbatim. Specific cases:

- `2`: name the missing resource (e.g. `"ticket <wi-id> not found - check the ID."`).
- `4`: `"ADO auth failed - check the local ado setup."`
- `5`: surface stderr without reinterpretation.

Do not promise or attempt rollback for any ADO mutation. On partial mutation failure, report created/changed IDs, the phase, the exact failing command, and stderr. Stop.

If validation fails after one autofix attempt, do not push, create a PR, or link the work item. The branch remains local; the ticket remains in its current state.

If any phase says abort, stop immediately. Leave ADO state as-is for phases not reached.

## Output

- **Success:** the PR web URL, alone, on the final line, with no formatting.
- **Abort:** `ABORT Phase <n> - <name>: <short reason>`.

Before the final line, include any required phase details, command output, stderr, changed IDs, skipped validation stages, and notable manual verification instructions.
