# ado

`ado` is a small Azure DevOps CLI for everyday project work. It manages Git repositories, pull requests, pipelines, and work items from one executable.

## Features

- Configure a default Azure DevOps organization and project.
- List, create, clone, and delete Git repositories.
- Create, inspect, review, comment on, and complete pull requests.
- List pipelines, start runs, and check or watch run status.
- Create, update, link, comment on, attach files to, and open work items.
- Print text, table, or JSON output for scripting.

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
ADO_REPO=your-repo
ADO_PAT=your-personal-access-token
```

You can also save non-secret defaults in the OS config directory:

```sh
ado config set --org https://dev.azure.com/your-org --project your-project
ado config show
```

Configuration precedence is:

1. CLI flags: `--org`, `--project`
2. Environment variables, including values loaded from `.env`
3. Saved config from `ado config set`

`ADO_PAT` is only read from the environment or `.env`; it is not written to the saved config file. `ADO_REPO` is optional and is used by PR commands when `--repo` is omitted. If neither is set, PR commands try to infer the repo from the current git `origin` remote.

## Global Options

Global options can be passed before or after the command:

```sh
ado --org https://dev.azure.com/your-org --project your-project repo list
ado repo list --output table
ado pr view 42 --repo my-service --output json
```

Output modes:

- `text`: concise human-readable output, the default.
- `table`: aligned columns where useful.
- `json`: full API response for scripts and automation.

## Command Guide

Use `ado --help`, `ado help <command>`, or `ado <command> --help` to navigate the built-in help tree.

### Repositories

```sh
ado repo list --output table
ado repo create --name my-service
ado repo clone my-service ./my-service
ado repo delete old-service --yes
```

Aliases:

- `ado repo ls` is the same as `ado repo list`.
- `ado repo rm` is the same as `ado repo delete`.

### Pull Requests

```sh
ado pr create --repo my-service --title "Add health check" --target main
ado pr list --repo my-service --status active --output table
ado pr view 42 --repo my-service
ado pr update 42 --repo my-service --title "Add readiness check" --field draft=false
ado pr link-work-item 42 --repo my-service --work-item 123
ado pr approve 42 --repo my-service --vote 10
ado pr comment 42 --repo my-service --text "Looks good to me."
ado pr threads 42 --repo my-service
ado pr thread-reply 42 7 --repo my-service --text "Fixed in the latest push."
ado pr thread-resolve 42 7 --repo my-service
ado pr complete 42 --repo my-service --merge-strategy squash --delete-source-branch
ado pr open 42 --repo my-service
```

PR status values are `active`, `completed`, `abandoned`, and `all`.

Merge strategies are `squash`, `noFastForward`, `rebase`, and `rebaseMerge`.

`ado pr link-work-item` creates Azure DevOps' native pull request relation on the work item. This differs from `ado wi link --hyperlink`: Azure DevOps recognizes the native relation as a PR link and shows it from both the pull request and work item views.

Useful aliases:

- `ado pr ls` is the same as `ado pr list`.
- `ado pr show` is the same as `ado pr view`.
- `ado pr browse` is the same as `ado pr open`.

### Pipelines

```sh
ado pipeline list --output table
ado pipeline run build-main --branch main
ado pipeline run 67 --branch feature/login --var environment=dev --var smoke=true
ado pipeline status 12345 --pipeline-id 67
ado pipeline status 12345 --pipeline-id 67 --watch
```

The `pipeline run` command accepts either a numeric pipeline ID or an exact pipeline name. The status command needs both the run ID and pipeline ID.

### Work Items

`work-item` and `wi` are the same top-level command:

```sh
ado wi create --title "Fix login redirect" --type Bug --assigned-to me
ado wi list --assigned-to me --state Active --output table
ado wi view 123
ado wi update 123 --state Closed --field priority=2
ado wi comment 123 --text "<p>Validated in staging.</p>"
ado wi comments 123
ado wi comment-edit 123 456 --text "<p>Updated comment.</p>"
ado wi comment-delete 123 456
ado wi link 123 --child 456 --comment "Split implementation task"
ado wi links 123
ado wi link-rm 123 --index 0
ado wi attach 123 ./screenshot.png --comment "Error dialog"
ado wi history 123 --limit 10
ado wi open 123
ado wi delete 123
```

Useful aliases:

- `ado wi ls` is the same as `ado wi list`.
- `ado wi show` is the same as `ado wi view`.
- `ado wi rm` is the same as `ado wi delete`.
- `ado wi browse` is the same as `ado wi open`.

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
cargo run -- pr --help
cargo run -- pr link-work-item --help
cargo run -- wi --help
cargo run -- wi create --help
cargo run -- pr complete --help
```
