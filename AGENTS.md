# AGENTS.md

**Editorial source of truth** for every Agent Skill authored in this repo. This file is **not** a runtime dependency — skills are invoked from arbitrary product repos where only the compiled `ado` binary is on `$PATH` and this repo is not present. Each SKILL.md must be **self-contained**: copy the conventions it needs from this file into the skill body, do not link back here.

AGENTS.md exists to keep multiple SKILL.md files consistent. When a convention changes, update it here first, then propagate the change into every SKILL.md that uses it. `/create-ado-skill` reads this file to generate compliant scaffolding.

Skills live in `.agents/skills/<skill-name>/SKILL.md` (Cursor's standard location; also picked up by other tools that follow the Agent Skills standard). Each skill is `disable-model-invocation: true` by default and is only triggered when a user explicitly types `/<skill-name>`.

---

## ADO CLI boundary

- All Azure DevOps access goes through the local `ado` CLI. Do not use the ADO web UI, raw undocumented REST calls, or guessed JSON shapes.
- `ado --help` lists every subcommand. Agents may run `ado <subcmd> --help` ad-hoc when a flag is unclear, but the **Command Surface** section of a SKILL.md should only declare `ado --help` for general discovery — not enumerate every `ado <subcmd> --help` variant. Reserve the Command Surface enumeration for the specific reads, mutations, and `ado schema <command-path>` calls the workflow actually runs.
- Prefer `--output json` for any command an agent parses. Rely on the published schema, not on field-by-field inference. Adding new fields is a non-breaking change; renaming or removing is not.
- Use `--explain` to dry-run any mutation before executing it. `--explain` exits `0` so dry-runs can be branched on cleanly.
- Treat the local `ado` executable as the configured ADO boundary. Skills do not run ADO setup; assume `ado` is installed and authenticated.

## Authentication

- Never inspect, print, request, log, or manage `ADO_PAT`. Treat it as opaque to every workflow.
- On auth failure (exit `4`), the standard message is: `ADO auth failed - check the local ado setup.`

## Exit code contract

| Code | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| `0`  | Success. `--explain` dry-runs also exit `0`.                     |
| `1`  | Unclassified error (arg parsing, I/O, unexpected runtime panic). |
| `2`  | Not found — work item, iteration, schema path, or resource.      |
| `3`  | Validation — missing or malformed input.                         |
| `4`  | Auth — ADO rejected the PAT (HTTP 401/403).                      |
| `5`  | ADO API — 4xx/5xx that was not auth or not-found.                |

Any non-zero exit from an `ado` invocation aborts the current skill with: **phase name, command, exit code, stderr verbatim**. Special cases:

- `2`: name the missing resource (`"ticket <wi-id> not found - check the ID."`).
- `4`: the standard auth message above.
- `5`: surface stderr without reinterpretation.

## HTML field formatting

ADO Description, Acceptance Criteria, and similar rich-text fields accept HTML. The `ado` CLI passes these flags through unchanged — un-escaped prose will break the rendered field.

- Treat all user-supplied prose as plain text. Escape before inserting into any tag:

  | Char | Escape    |
  | ---- | --------- |
  | `&`  | `&amp;`   |
  | `<`  | `&lt;`    |
  | `>`  | `&gt;`    |
  | `"`  | `&quot;`  |
  | `'`  | `&#39;`   |

- The only HTML tags a skill may generate are structural: `<p>`, `<ul>`, `<li>`, `<strong>`, `<em>`, `<br>`.
- Prefer `--acceptance-criteria` when `Microsoft.VSTS.Common.AcceptanceCriteria` is supported for the work item type; otherwise embed a labeled `<strong>Acceptance criteria</strong>` section inside `--description`.

## No-rollback rule

- Do not promise or attempt rollback for any ADO mutation. ADO state is not transactional from the CLI's perspective.
- On partial mutation failure, report: created/changed IDs so far, the phase, the exact failing command, and stderr. Stop. Do not chain repair attempts unless the user explicitly directs one.

## Approval gates

Every mutation phase shows a preview and waits for an explicit yes before running. A "yes" approves the previewed change only; subsequent changes need new approvals.

- For field updates: show a diff-style preview of every changed field.
- For ticket creation: show the full draft (parent, children, link plan, field plan) in execution order.
- For destructive git operations (e.g. force-push): require both explicit yes and an ancestry check (`git merge-base --is-ancestor origin/<branch> HEAD`).

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

## Validation toolchain matrix

Detect the project type from marker files at the repo root. Run every family whose marker is present. Skip silently absent families. If a family is present but a stage doesn't apply (e.g. no `lint` script), say so explicitly in output — don't silently skip.

| Marker file      | Build / type-check                          | Lint / format (check, not rewrite)                               | Tests             | Autofix                                |
| ---------------- | ------------------------------------------- | ---------------------------------------------------------------- | ----------------- | -------------------------------------- |
| `Cargo.toml`     | `cargo build`                               | `cargo fmt --check && cargo clippy --all-targets -- -D warnings` | `cargo test`      | `cargo fmt`                            |
| `package.json`   | `npm run build` (skip if no `build` script) | `npm run lint` (skip if absent), else `prettier --check .`       | `npm test`        | `npx eslint --fix . && npx prettier --write .` |
| `pyproject.toml` | `python -m compileall .`                    | `ruff check && ruff format --check`                              | `pytest`          | `ruff check --fix && ruff format`      |
| `go.mod`         | `go build ./...`                            | `gofmt -l . && go vet ./...`                                     | `go test ./...`   | `gofmt -w .`                           |

`cargo fmt` and `gofmt -w` rewrite files and exit `0`; they are autofix commands, not check commands. The Lint/format column uses the check variants on purpose.

**Failure protocol.** On a stage failure, attempt **one** autofix pass for that family and re-run the failing stage. If the rerun passes and the autofix changed files, commit them with `chore(AB#<wi-id>): lint autofix` before continuing — a passing local check with uncommitted fixes will fail CI. If the rerun still fails, bail without pushing or opening a PR.

## Local artifact rules

- The `.ado/` directory at the repo root holds local design notes and is git-ignored. It must never be committed.
- Per-ticket design notes live at `.ado/<wi-id>.design.md`. Sections: `## Approach`, `## Touchpoints`, `## Acceptance Criteria`, `## Risks`, `## Test strategy`.
- If `.ado/` does not exist when a skill needs it, create it and add `.ado/` to `.gitignore` if not already ignored.

## Output contract

- The final line of every skill is either the success artifact (PR URL, output file path, or a structured summary) or an abort line.
- Standard abort line: `ABORT Phase <n> - <name>: <short reason>`.
- Do not print large generated bodies (e.g. full markdown documents) unless the user asks. Print the path and a short summary instead.

## Skill authoring

When adding or updating a skill in this repo:

1. Use `/create-ado-skill` — it generates compliant SKILL.md scaffolding that follows the shape below.
2. Place the skill at `.agents/skills/<skill-name>/SKILL.md`. The `name:` frontmatter must match the parent folder name.
3. Set `disable-model-invocation: true` unless the user explicitly opts the skill into automatic invocation.
4. Write a `description:` that starts with "Use this skill when…" so the slash menu reads naturally.
5. **Inline the conventions the skill needs.** Skills run from arbitrary product repos with no access to this file — copy the relevant rules into the SKILL.md body verbatim (or close to it). Do not link to AGENTS.md from a SKILL.md. If a convention is missing here, add it here first, then inline it.
6. Keep `SKILL.md` focused on the workflow. Skill-internal files (`references/`, `scripts/`) ARE portable because they live inside the skill directory — use them when bulky reference material would clutter the main file. Add scripts only for deterministic parsing or validation that prose cannot handle reliably.
7. When AGENTS.md changes, update every SKILL.md that inlines the changed section. This is the editorial cost of runtime portability.

### Standard skill shape

```markdown
---
name: <skill-name>
description: Use this skill when ...
disable-model-invocation: true
---

# <Skill Title>

One-paragraph summary of what this skill does and when to use it.

## Inputs
- <required and optional inputs>

## Preconditions
- <required environment and safety checks; link to AGENTS.md sections>

## Command Surface
- <skill-specific commands; reference AGENTS.md for shared rules>

## Workflow
1. <ordered, deterministic steps with explicit approval gates>

## Failure Handling
- <skill-specific cases; reference AGENTS.md exit code contract for the general case>

## Output
- <exact final artifact or handoff format>
```
