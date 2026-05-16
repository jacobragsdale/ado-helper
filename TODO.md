# Cleanup, Test, And Smoke-Test Plan

Goal: make the code easier to change, add focused low-cost tests, then run a real Azure DevOps smoke test against the configured project and validate every command path.

## 1. Baseline And Guardrails

- Keep any current code changes intact. `src/commands/pr.rs` is already modified in the working tree, so inspect it carefully before editing.
- Run the baseline checks:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- Capture the command surface from `ado --help` plus each top-level command help so the smoke checklist stays tied to the actual CLI.
- Do not print or commit secrets. Use the existing `.env` loading path for `ADO_ORG_URL`, `ADO_PROJECT`, `ADO_PAT`, and optionally `ADO_REPO`.

## 2. Cleanup Pass

- Split pure parsing/formatting helpers away from network and process code where it is cheap:
  - PR: repo remote parsing, PR field alias resolution, thread status/location formatting, comment preview truncation.
  - Pipeline: pipeline selector logic separate from ADO fetching, variable body construction separate from POST.
  - Work items: WIQL clause construction, relation selection, attachment filename/path encoding.
  - Repo: clone URL credential injection and repo lookup result formatting.
- Fix obvious small correctness issues while staying scoped:
  - Percent-encoding should encode UTF-8 bytes, not Unicode scalar values.
  - Truncation helpers should avoid slicing strings on non-character byte boundaries.
  - Project/repo path segments should be consistently encoded anywhere user-provided names enter URLs.
- Normalize repeated request patterns:
  - Add a `patch_json` helper to `AdoClient` for PR/comment endpoints that need `application/json`.
  - Keep `patch_json_patch` only for work item JSON Patch endpoints.
- Keep command behavior stable. This pass should make the existing commands easier to test, not redesign the CLI.

## 3. Simple Tests

- Add unit tests for pure helpers first:
  - `fields::split_field_arg` and `coerce_value` edge cases already exist; add whitespace, negative number, and float/string ambiguity cases if useful.
  - `repo::inject_pat` with existing password/userinfo, trailing `.git`, and non-ADO HTTPS URL.
  - `pr::resolve_pr_field`, `strip_refs_heads`, `comment_preview`, `thread_status_label`, `thread_location`, and a pure repo-name-from-remote helper after refactor.
  - `workitem::helpers::encode_path`, `escape_wiql`, `field_str`, and `workitem_url`.
  - `workitem::flags::resolve_field_name` for common aliases and unknown alias errors.
- Add request-body construction tests without hitting ADO:
  - PR create/update `--field` overrides.
  - Pipeline run variables and branch ref body.
  - Work item relation ops for parent, child, related, predecessor, successor, and hyperlink.
- Add CLI parse smoke tests with `clap::CommandFactory` or `trycmd` only if the unit tests leave gaps. Keep them small.
- Target: get from 5 tests to roughly 20-30 fast tests before adding heavier integration fixtures.

## 4. Real ADO Smoke Test Setup

- Use one disposable prefix for everything:
  - `SMOKE_PREFIX=ado-helper-smoke-$(date +%Y%m%d-%H%M%S)`
  - `SMOKE_REPO=$SMOKE_PREFIX-repo`
  - `SMOKE_WI_TITLE="$SMOKE_PREFIX work item"`
  - `SMOKE_BRANCH=$SMOKE_PREFIX-branch`
- Build once with `cargo build`, then use `target/debug/ado` for every command.
- Save command output to a local smoke log directory, for example `smoke/$SMOKE_PREFIX/`, with secrets redacted.
- Record IDs as they are created:
  - repo name and clone path
  - work item IDs
  - comment IDs
  - PR ID
  - PR thread ID
  - pipeline ID and run ID, if a safe pipeline is available
- Before mutating anything, run read-only checks:
  - `ado config show`
  - `ado repo list --output json`
  - `ado pr list --status all --output json`
  - `ado pipeline list --output json`
  - `ado wi list --output json`

## 5. Smoke Test Matrix

- Config:
  - Validate `config show` against `.env`.
  - Validate `config set` using a temporary config directory, not the user's real global config.
- Repo:
  - `repo list`
  - `repo create --name $SMOKE_REPO`
  - `repo clone $SMOKE_REPO <temp-dir>`
  - `repo delete $SMOKE_REPO --yes` during cleanup.
- Work items:
  - `wi create --type Task --title "$SMOKE_WI_TITLE" --description ...`
  - `wi list --search "$SMOKE_PREFIX"`
  - `wi view <id>`
  - `wi update <id> --state ... --tags "$SMOKE_PREFIX" --field priority=2`
  - `wi comment <id> --text ...`
  - `wi comments <id>`
  - `wi comment-edit <id> <comment-id> --text ...`
  - `wi comment-delete <id> <comment-id>`
  - Create a second disposable work item, then validate `wi link`, `wi links`, and `wi link-rm`.
  - `wi attach <id> <small-temp-file>`
  - `wi history <id> --limit 10`
  - `wi open <id>` only after confirming browser-launch side effects are acceptable, or after adding a print/dry-run URL path.
  - `wi delete <id>` for all disposable work items.
- PR:
  - In the cloned smoke repo, push an initial commit to `main`, create `$SMOKE_BRANCH`, push a change, then run `pr create`.
  - `pr list --repo $SMOKE_REPO --status active`
  - `pr view <id> --repo $SMOKE_REPO`
  - `pr update <id> --repo $SMOKE_REPO --title ... --field draft=false`
  - `pr approve <id> --repo $SMOKE_REPO --vote 10`
  - `pr comment <id> --repo $SMOKE_REPO --text ...`
  - `pr threads <id> --repo $SMOKE_REPO`
  - `pr thread-reply <id> <thread-id> --repo $SMOKE_REPO --text ...`
  - `pr thread-resolve <id> <thread-id> --repo $SMOKE_REPO`
  - `pr abandon <id> --repo $SMOKE_REPO`
  - `pr reactivate <id> --repo $SMOKE_REPO`
  - `pr complete <id> --repo $SMOKE_REPO --delete-source-branch`
  - `pr open <id>` only after confirming browser-launch side effects are acceptable, or after adding a print/dry-run URL path.
- Pipeline:
  - `pipeline list`
  - If the project has a known safe pipeline, run it on a safe branch:
    - `pipeline run <pipeline-id-or-name> --branch <branch> --var ADO_HELPER_SMOKE=$SMOKE_PREFIX`
    - `pipeline status <run-id> --pipeline-id <pipeline-id>`
  - Avoid `--watch` unless a short-running safe pipeline is chosen.

## 6. Validation And Cleanup

- For every command, validate both exit status and one concrete output fact:
  - JSON parses where `--output json` is used.
  - Created IDs can be viewed.
  - Updated fields are visible after a follow-up read.
  - Deleted comments/relations no longer appear.
  - Completed or abandoned PR status matches the requested transition.
- Cleanup must run even after partial failure:
  - Delete disposable work items.
  - Delete or complete disposable PRs.
  - Delete the disposable repo.
  - Remove temp clone and attachment files.
- End with:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - A short smoke report listing commands passed, skipped, failed, and any ADO cleanup leftovers.
