# ado

`ado` is a small Azure DevOps CLI for everyday project work. It manages Git repositories, pull requests, pipelines, and work items from one executable.

## Features

- Configure a default Azure DevOps organization, project, and team.
- Show the caller's identity (`ado me`), team membership, iterations, and area paths.
- List, create, clone, delete, and inspect Git repository branches, tags, and commits.
- Create, inspect, review, comment on, and complete pull requests.
- List pipelines and runs, start runs, inspect logs, preview YAML, and check or watch run status.
- Create, query, update, link, comment on, attach/download files to/from, and open work items.
- Plan sprint work: backlog, taskboard, plan-into, capacity, burndown, rollover, and summaries.
- Discover work item types, states, and field reference names without scraping the web UI.
- Print text, table, or JSON output for scripting, with stable typed schemas (`ado schema`).
- Agent-friendly chassis: predictable exit codes, `--explain` dry-runs, `--quiet`, and stdin batching for mutations.

## Installation

Build from source:

```sh
cargo build
```

Run without installing:

```sh
cargo run -- repo list
cargo run -- wi list --assigned-to me
```

Install locally from this checkout:

```sh
cargo install --path .
```

After installing, the binary is available as `ado`.

## Configuration

`ado` needs an Azure DevOps organization URL, project, and Personal Access Token.

Copy `.env.example` to `.env` for per-checkout settings:

```sh
ADO_ORG_URL=https://dev.azure.com/your-org
ADO_PROJECT=your-project
ADO_TEAM=your-team
ADO_REPO=your-repo
ADO_PAT=your-personal-access-token
```

You can also save non-secret defaults in the OS config directory:

```sh
ado config set --org https://dev.azure.com/your-org --project your-project --team "Your Team"
ado config show
```

Configuration precedence is:

1. CLI flags: `--org`, `--project`, `--team`
2. Environment variables, including values loaded from `.env`
3. Saved config from `ado config set`

`ADO_PAT` is only read from the environment or `.env`; it is not written to the saved config file. `ADO_REPO` is optional and is used by PR and repo inspection commands when `--repo` is omitted. If neither is set, those commands try to infer the repo from the current git `origin` remote. `ADO_TEAM` is used by team-scoped commands (`iteration` and `sprint`); set it once with `ado team set <name>` and `ado config show` will report the resolved value.

## Global Options

Global options can be passed before or after the command:

```sh
ado --org https://dev.azure.com/your-org --project your-project repo list
ado repo list --output table
ado pr view 42 --repo my-service --output json
```

| Flag | Purpose |
| --- | --- |
| `--org <URL>` | Override the saved organization URL for this call. |
| `--project <NAME>` | Override the saved project for this call. |
| `--team <NAME>` | Override the saved team for this call (used by iteration and future sprint commands). |
| `--output <text\|table\|json>` | Output mode (default `text`). |
| `--quiet` | Suppress decorative banners and progress hints. Result lines and errors still print. |
| `--explain` | Dry-run: print the would-be REST call(s) for any mutation and exit 0 without touching ADO. |

Output modes:

- `text`: concise human-readable output, the default.
- `table`: aligned columns where useful.
- `json`: typed, stable schema discoverable via `ado schema <command>`.

## Output Contract

Every command that supports `--output json` emits a typed struct rather than a raw passthrough of the ADO REST response. The shape is stable across patch releases and discoverable at runtime:

```sh
ado schema --list        # every command path with a registered schema
ado schema wi view       # the JSON Schema for `ado wi view --output json`
ado schema iteration current --output json
```

Agents should rely on the published schema, not on field-by-field shape inference. Adding new fields to an existing schema is a non-breaking change; renaming or removing a field is not. `ado schema --list --output json` returns a JSON array of paths suitable for automation.

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success. `--explain` dry-runs also exit `0`. |
| `1` | Unclassified error (CLI argument parsing failures, I/O errors, unexpected panics caught by the runtime). |
| `2` | Not found — the targeted work item, iteration, schema path, or other resource does not exist. |
| `3` | Validation — missing or malformed input (bad ids on stdin, missing required flags). |
| `4` | Auth — Azure DevOps rejected the PAT (HTTP 401/403). |
| `5` | API — Azure DevOps returned a 4xx/5xx that was not auth/not-found. |

Agents can branch on these codes without parsing stderr.

## Stdin Batching

Mutation commands accept ids on stdin so an agent can chain `ado wi query --output json` (or `ado wi list`) into a follow-up mutation without an awk dance:

```sh
ado wi list --assigned-to me | ado wi update --tags "rolled-over"
ado wi query --wiql "..." --output json | jq '[.[].id]' | ado wi update --state Closed
ado wi query --wiql "..." --output json | jq '[.[].id]' | ado sprint plan-into --iteration @next
echo '[123, 124]' | ado pr link-work-item 42 --repo my-service
```

Accepted input forms when ids are read from stdin:

- One id per line (blanks ignored; a leading `#` is tolerated so `ado wi list` text output drops in directly).
- A JSON array of integers (e.g. `[1, 2, 3]`).

If no ids are provided as arguments and stdin is a TTY, the command exits `3` (validation) with a clear message rather than blocking on stdin. Today this works for `ado wi update`, `ado sprint plan-into`, and `ado pr link-work-item`.

## Command Guide

Use `ado --help`, `ado help <command>`, or `ado <command> --help` to navigate the built-in help tree.

### Foundation

These commands describe *you*, your team, and the iteration/area shape of your project. Higher-level workflows (sprint planning, standup digests) are built on top of them.

```sh
ado me                       # show the caller's identity (cached after first call)
ado me refresh               # force a fresh fetch and overwrite the cache

ado team list --output table
ado team set "My Team"       # persist as the default team for this project
ado team current
ado team members

ado iteration list --output table
ado iteration current        # alias for `iteration view @current`
ado iteration next           # alias for `iteration view @next`
ado iteration view @previous

ado area tree --depth 3
ado area list                # flat backslash-separated paths, paste-ready into --area
```

`ado me` writes the caller's identity to the config file. Commands that resolve "me" (`wi list --assigned-to me`, future `my queue`) read the cached identity instead of round-tripping `_apis/connectionData` on every call. Use `ado me refresh` after switching orgs.

Iteration shortcuts: `@current`, `@next`, `@previous` (and the aliases `@now`, `@prev`) work anywhere an iteration reference is accepted. They resolve against the team selected by `--team` / `ADO_TEAM` / saved config.

### Sprint Planning

```sh
ado sprint backlog --iteration @next --output table
ado sprint backlog --type Bug --state Active --tag blocked --top 20
ado sprint board --iteration @current --output table
ado sprint plan-into 123 124 --iteration @next --assigned-to me --state Active
ado wi query --wiql "..." --output json | jq '[.[].id]' | ado sprint plan-into --iteration @next

ado sprint capacity --iteration @current --output table
ado sprint capacity set --member me --hours-per-day 6 --activity Development

ado sprint burndown --iteration @current
ado sprint burndown --by member --output json
ado sprint rollover --from @current --to @next --dry-run
ado sprint rollover --state-filter "Active,New" --reset-remaining
ado sprint summary --iteration @current --output json
```

`sprint backlog`, `board`, `capacity`, `burndown`, and `summary` are read-only. `sprint plan-into`, `capacity set`, and `rollover` mutate ADO and honor the global `--explain` flag where their write calls are reached. `sprint rollover --dry-run` previews the exact set of unfinished items without writing.

`sprint goal` is intentionally not implemented yet. The stock Azure DevOps Work/WIT REST APIs do not expose a verified first-party sprint-goal text endpoint; keep using the web UI or provide a deliberate storage model before adding a CLI command.

### Schemas & Metadata Discovery

```sh
ado schema --list                   # every command path with a registered output schema
ado schema wi view                  # JSON Schema for `ado wi view --output json`
ado schema wi attachments           # JSON Schema for attachment listing
ado schema wi attachment-download   # JSON Schema for download results
ado wi types --output table         # work item types defined in the project
ado wi states "User Story"          # valid states for a work item type
ado wi fields --type Bug            # field names + reference names for a type
ado wi fields                       # all fields in the project
```

`reference_name` (e.g. `Microsoft.VSTS.Common.Priority`) is the canonical identifier used in WIQL and `--field NAME=VALUE`. The `name` column is the human label shown in the web UI.

### Repositories

```sh
ado repo list --output table
ado repo branches --repo my-service --output table
ado repo tags --repo my-service --output table
ado repo commits --repo my-service --branch main --max 10 --output table
ado repo create --name my-service
ado repo clone my-service ./my-service
ado repo delete old-service --yes
```

Aliases:

- `ado repo ls` is the same as `ado repo list`.
- `ado repo rm` is the same as `ado repo delete`.

When `--repo` is omitted for `repo branches`, `repo tags`, or `repo commits`, `ado` uses `ADO_REPO` or the current git `origin` remote. Branch and tag commands accept `--filter` as a prefix relative to `refs/heads/` or `refs/tags/`. Commit listing accepts `--branch`, `--author`, `--from`, `--to`, and `--max`.

### Pull Requests

```sh
ado pr create --repo my-service --title "Add health check" --target main
ado pr list --repo my-service --status active --output table
ado pr view 42 --repo my-service
ado pr update 42 --repo my-service --title "Add readiness check" --field draft=false
ado pr link-work-item 42 --repo my-service --work-item 123
ado pr link-work-item 42 --repo my-service --work-item 123 --work-item 124
echo "123\n124" | ado pr link-work-item 42 --repo my-service
ado pr checks 42 --repo my-service
ado pr approve 42 --repo my-service --vote 10
ado pr comment 42 --repo my-service --text "Looks good to me."
ado pr threads 42 --repo my-service
ado pr thread-reply 42 7 --repo my-service --text "Fixed in the latest push."
ado pr thread-resolve 42 7 --repo my-service
ado pr checkout 42 --repo my-service
ado pr checkout-clean --all
ado pr complete 42 --repo my-service --merge-strategy squash --delete-source-branch
ado pr open 42 --repo my-service
```

`ado pr checks` lists every branch-policy evaluation on a PR (build validation, required reviewers, comment resolution, status posts) and shows a `<approved>/<total>` rollup for blocking policies. `ado pr check` is an alias.

`ado pr checkout` pulls a PR's source branch locally for review. When run inside a clone of the PR's repo it fetches and checks out in place; otherwise it clones the repo to `~/.ado/reviews/<repo>-pr-<id>` (override with `--dir`). Pass `--detach` for a detached HEAD or `--branch <name>` to rename the local branch. `ado pr checkout-clean <ID>` or `--all` removes review clones under `~/.ado/reviews`; `--dry-run` previews without removing. Forked PRs are not yet supported.

PR status values are `active`, `completed`, `abandoned`, and `all`.

Merge strategies are `squash`, `noFastForward`, `rebase`, and `rebaseMerge`.

`ado pr link-work-item` creates Azure DevOps' native pull request relation on the work item. This differs from `ado wi link --hyperlink`: Azure DevOps recognizes the native relation as a PR link and shows it from both the pull request and work item views.

`ado pr list` searches across every repo in the project when `--repo` is omitted. Other repo-specific PR commands use `ADO_REPO` or the current git `origin` remote when `--repo` is omitted.

Useful aliases:

- `ado pr ls` is the same as `ado pr list`.
- `ado pr show` is the same as `ado pr view`.
- `ado pr browse` is the same as `ado pr open`.

### Pipelines

```sh
ado pipeline list --output table
ado pipeline run build-main --branch main
ado pipeline run 67 --branch feature/login --var environment=dev --var smoke=true
ado pipeline runs build-main --branch main --max 5 --output table
ado pipeline status 12345 --pipeline-id 67
ado pipeline status 12345 --pipeline-id 67 --watch
ado pipeline logs 12345 --pipeline-id 67
ado pipeline logs 12345 --pipeline-id 67 2 --follow
ado pipeline preview build-main --branch main --var smoke=true
ado pipeline preview 67 --ref refs/heads/main --yaml-file azure-pipelines.yml --output json
```

Commands that take a pipeline accept either a numeric pipeline ID or an exact pipeline name. The status and logs commands need both the run ID and pipeline ID. `pipeline logs` lists available logs when `LOG_ID` is omitted, and prints plain log content when a log ID is provided. `pipeline preview` prints the rendered `finalYaml` in text mode and the full preview response in JSON mode.

### Work Items

`work-item` and `wi` are the same top-level command:

```sh
ado wi create --title "Fix login redirect" --type Bug --assigned-to me
ado wi list --assigned-to me --state Active --output table
ado wi query --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC"
ado wi query --file ./bugs.wiql --output table
ado wi view 123
ado wi update 123 --state Closed --field priority=2
ado wi update 123 124 125 --tags "release;docs"
ado wi list --assigned-to me | ado wi update --state Closed
ado wi comment 123 --text "<p>Validated in staging.</p>"
ado wi comments 123
ado wi comment-edit 123 456 --text "<p>Updated comment.</p>"
ado wi comment-delete 123 456
ado wi link 123 --child 456 --comment "Split implementation task"
ado wi links 123
ado wi link-rm 123 --index 0
ado wi attach 123 ./screenshot.png --comment "Error dialog"
ado wi attachments 123 --output table
ado wi attachment-download 123 0 --dir ./attachments
ado wi attachment-download 123 report.xlsx --force
ado wi attachment-download 123 --all --dir ./attachments
ado wi history 123 --limit 10
ado wi open 123
ado wi delete 123
```

Useful aliases:

- `ado wi ls` is the same as `ado wi list`.
- `ado wi show` is the same as `ado wi view`.
- `ado wi rm` is the same as `ado wi delete`.
- `ado wi browse` is the same as `ado wi open`.

`ado wi query` runs raw WIQL against the configured project. Pass exactly one query source: `--wiql` for inline WIQL or `--file` for a saved query file. Query results are hydrated in batches of up to 200 work item IDs and printed with the same text, table, or JSON shape as `ado wi list`.

`ado wi attachments` lists only file attachments from the work item's relations, including the attachment index, filename, size, UUID, comment, and raw relation index. Use `ado wi attachment-download <ID> <SELECTOR>` to download one attachment by list index, attachment UUID, or exact filename. Use `--all` to download every attachment. Downloads write to the current directory by default, `--dir` writes into a directory, `--file` writes one selected attachment to an exact path, and existing files require `--force`.

## Field Aliases

Work item `--field NAME=VALUE` accepts full Azure DevOps field names and these aliases:

| Alias | Azure DevOps field |
| --- | --- |
| `title` | `System.Title` |
| `state` | `System.State` |
| `reason` | `System.Reason` |
| `description` | `System.Description` |
| `assigned-to` | `System.AssignedTo` |
| `iteration`, `iteration-path` | `System.IterationPath` |
| `area`, `area-path` | `System.AreaPath` |
| `tags` | `System.Tags` |
| `history` | `System.History` |
| `priority` | `Microsoft.VSTS.Common.Priority` |
| `severity` | `Microsoft.VSTS.Common.Severity` |
| `activity` | `Microsoft.VSTS.Common.Activity` |
| `value-area` | `Microsoft.VSTS.Common.ValueArea` |
| `risk` | `Microsoft.VSTS.Common.Risk` |
| `stack-rank` | `Microsoft.VSTS.Common.StackRank` |
| `acceptance-criteria` | `Microsoft.VSTS.Common.AcceptanceCriteria` |
| `story-points` | `Microsoft.VSTS.Scheduling.StoryPoints` |
| `effort` | `Microsoft.VSTS.Scheduling.Effort` |
| `original-estimate` | `Microsoft.VSTS.Scheduling.OriginalEstimate` |
| `remaining-work` | `Microsoft.VSTS.Scheduling.RemainingWork` |
| `completed-work` | `Microsoft.VSTS.Scheduling.CompletedWork` |
| `start-date` | `Microsoft.VSTS.Scheduling.StartDate` |
| `target-date` | `Microsoft.VSTS.Scheduling.TargetDate` |
| `repro-steps` | `Microsoft.VSTS.TCM.ReproSteps` |
| `system-info` | `Microsoft.VSTS.TCM.SystemInfo` |

PR `--field NAME=VALUE` accepts full Azure DevOps keys and these aliases:

| Alias | Azure DevOps key |
| --- | --- |
| `title` | `title` |
| `description` | `description` |
| `draft`, `is-draft` | `isDraft` |
| `status` | `status` |
| `auto-complete`, `auto-complete-set-by` | `autoCompleteSetBy` |

Values are lightly coerced for JSON payloads: booleans, numbers, and `null` become JSON values when possible.

## Safety Notes

- Treat `ADO_PAT` as a secret. Prefer `.env` or your shell environment, and do not commit real tokens.
- `ado repo clone` injects the PAT into the clone URL for authentication, then rewrites `origin` back to the credential-free remote by default. Only use `--keep-pat-in-remote` in controlled automation.
- `ado repo delete --yes` permanently deletes the Azure DevOps repository.
- `ado wi delete` soft-deletes to the recycle bin by default. `ado wi delete --destroy` permanently destroys the work item.
- `ado pr link-work-item` mutates the target work item's relations. Use `ado wi links` and `ado wi link-rm` if you need to inspect or remove the created PR relation.
- `ado pr open` and `ado wi open` launch the default browser.

## Development

Run the standard checks before sending changes for review:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Useful help smoke checks:

```sh
cargo run -- --help
cargo run -- me --help
cargo run -- team --help
cargo run -- iteration --help
cargo run -- area --help
cargo run -- schema --help
cargo run -- sprint --help
cargo run -- sprint backlog --help
cargo run -- sprint board --help
cargo run -- sprint plan-into --help
cargo run -- sprint capacity --help
cargo run -- sprint burndown --help
cargo run -- sprint rollover --help
cargo run -- sprint summary --help
cargo run -- pr --help
cargo run -- pr link-work-item --help
cargo run -- pr checks --help
cargo run -- pr checkout --help
cargo run -- pr checkout-clean --help
cargo run -- wi --help
cargo run -- wi create --help
cargo run -- wi update --help
cargo run -- wi query --help
cargo run -- wi types --help
cargo run -- wi states --help
cargo run -- wi fields --help
cargo run -- pr complete --help
cargo run -- pipeline runs --help
cargo run -- pipeline logs --help
cargo run -- pipeline preview --help
cargo run -- repo branches --help
cargo run -- repo tags --help
cargo run -- repo commits --help
```

Live end-to-end smoke (requires `.env` with a valid PAT/org/project; `ADO_TEAM` or a saved team is needed for iteration and sprint commands):

```sh
cargo run -- me
cargo run -- team list --output table
cargo run -- iteration current
cargo run -- sprint backlog --iteration @current --output table
cargo run -- sprint capacity --output table
cargo run -- area tree --depth 3
cargo run -- wi types --output table
cargo run -- schema --list
cargo run -- schema wi view

# Chassis behaviours
cargo run -- wi view 999999999          # expect exit 2 (NotFound)
cargo run -- --explain wi update 123 --state Closed   # prints DRY-RUN, exit 0
echo "123\n124" | cargo run -- --explain wi update --state Closed
```

Full live E2E suite (creates disposable ADO repos, PRs, work items, comments, attachments, and a YAML pipeline, then cleans them up):

```sh
tests/live-e2e.sh
```

Useful toggles:

```sh
ADO_E2E_KEEP_RESOURCES=1 tests/live-e2e.sh      # keep created ADO resources for debugging
ADO_E2E_SKIP_PIPELINE=1 tests/live-e2e.sh       # skip pipeline creation/run/log checks
ADO_E2E_PIPELINE_TIMEOUT_SECONDS=600 tests/live-e2e.sh
```
