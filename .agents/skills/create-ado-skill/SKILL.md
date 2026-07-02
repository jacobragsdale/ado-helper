---
name: create-ado-skill
description: Use this skill when creating or updating an Agent Skill for Azure DevOps or the local ado CLI workflows in this repo. Produces compact, predictable, slash-invoked, self-contained ADO skills that inline conventions from AGENTS.md rather than link to it.
disable-model-invocation: true
metadata:
  version: "3.0"
---

# Create ADO Skill

Create or update a project Agent Skill for ADO work. Store new skills at:

```text
.agents/skills/<skill-name>/SKILL.md
```

## Runtime portability — the key constraint

Skills authored in this repo are invoked from **other product repos** where only the compiled `ado` binary is on `$PATH`. The ado-helper repo, its `AGENTS.md`, and its other SKILL.md files are **not** readable at runtime. Every SKILL.md must be **self-contained**.

This means: when a skill needs a convention (HTML escaping, exit codes, branch format, etc.), copy the relevant section of `AGENTS.md` into the SKILL.md body verbatim (or lightly adapted). Do **not** insert links like `../../../AGENTS.md#section` — they won't resolve from another repo. AGENTS.md exists as the editorial source of truth for the maintainer; it is never a runtime dependency.

## Defaults

- Set `disable-model-invocation: true` in every generated skill unless the user explicitly requests automatic invocation.
- Use lowercase hyphenated names only. The `name` frontmatter must match the parent folder.
- Write a description that starts with "Use this skill when..." Keep it concise and under 1024 characters.
- Do not add `paths` unless the skill is genuinely file-scoped.
- Keep the main `SKILL.md` focused. Move bulky details to `references/` inside the skill folder only when the skill says exactly when to read each file (references travel with the skill, so they remain portable).
- Prefer one `SKILL.md` and no scripts. Add scripts only for deterministic parsing or validation that prose cannot handle reliably.

## Which AGENTS.md sections to inline

The table below maps each AGENTS.md section to the skills that typically need it. When generating a skill, copy each applicable section into the SKILL.md body (in `Preconditions`, a dedicated section, or `Failure Handling` as noted). Adapt wording to fit the skill's voice but preserve the rules verbatim.

| AGENTS.md section            | Skills that need it                                    | Where to inline in the SKILL.md                                  |
| ---------------------------- | ------------------------------------------------------ | ---------------------------------------------------------------- |
| ADO CLI boundary             | All ADO skills                                         | `## Preconditions` (one paragraph)                               |
| Authentication               | All ADO skills                                         | `## Preconditions` (one sentence, no `ADO_PAT`)                  |
| Exit code contract           | All ADO skills                                         | `## Failure Handling` (table + specific cases)                   |
| HTML field formatting        | Any skill that writes Description / AcceptanceCriteria | Dedicated `## HTML field formatting` section                     |
| No-rollback rule             | Any skill that mutates ADO                             | `## Failure Handling`                                            |
| Approval gates               | Any skill that mutates ADO                             | In the relevant workflow step where the gate appears             |
| Branch and commit conventions | Skills that touch git (developer workflow, future PR skills) | Dedicated `## Branch and commit conventions` section          |
| Validation toolchain matrix  | Skills that run repo-local toolchains                  | Dedicated `## Validation toolchain` section (keep the table)     |
| Local artifact rules         | Skills that read/write `.ado/<wi-id>.design.md`        | Dedicated `## Local artifact rules` section                      |
| Output contract              | All skills                                             | `## Output` (final-line format + abort line)                     |

If a needed convention is not yet in AGENTS.md, add it to AGENTS.md first, then inline it into the new skill so it survives outside this repo.

## Grounding

Before writing, collect the smallest useful local context:

1. Read the user request for the desired skill name, workflow, and whether the workflow is read-only or mutating.
2. Read `AGENTS.md` (the editorial source) and any existing SKILL.md in `.agents/skills/` that overlaps in scope — both for content to reuse and for stylistic consistency.
3. Use `ado --help`, `ado <command> --help`, or `ado schema <command-path>` only when exact command flags or JSON fields are unclear.
4. Ask one focused question only if the skill's purpose, safety boundary, or output contract cannot be inferred.

## Skill Shape

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
- <inlined ADO CLI boundary + authentication paragraphs>
- <other skill-specific environment checks>

## Command Surface
- Lead with `ado --help` as the general-discovery entry point. Do NOT enumerate `ado wi --help`, `ado pr --help`, etc. — agents can run those ad-hoc when a flag is unclear.
- Then enumerate the specific reads, mutations, and `ado schema <command-path>` calls the workflow actually runs.
- Group as discovery / reads / mutations when there are several of each.

## <Inlined convention sections as needed>
e.g. HTML field formatting / Branch and commit conventions / Validation toolchain / Local artifact rules

## Workflow
1. <ordered, deterministic steps with explicit approval gates>

## Failure Handling
- <inlined exit code table + cases>
- <inlined no-rollback rule>
- <skill-specific failure cases>

## Output
- <inlined output contract: success final line + abort line>
```

## Predictability Checklist

Before finishing, verify the generated skill:

- Has `disable-model-invocation: true` unless the user explicitly opted into automatic invocation.
- Description starts with "Use this skill when..." and is under 1024 characters.
- Uses concrete commands, fields, and output formats instead of vague phrases like "handle appropriately" or "follow best practices".
- **Command Surface is lean.** Only `ado --help` is listed for general discovery; the rest of the section enumerates only the specific reads, mutations, and `ado schema` calls the workflow actually runs. No `ado <subcmd> --help` enumeration.
- **Is self-contained.** No relative-path links to `AGENTS.md` or to other SKILL.md files. No assumption that any file from the ado-helper repo is readable at runtime.
- **Inlines every applicable AGENTS.md section** from the mapping table above. Copy text verbatim where practical so multiple SKILL.md files stay consistent.
- States when to ask the user for approval, especially before mutating ADO.
- Keeps required context in the main file and optional context behind clear, conditional references to skill-internal files (`references/`).
- Avoids copying entire source docs when a concise workflow is enough.
- Contains no secrets, org-specific tokens, or accidental real ADO IDs unless the user requested a skill for that exact environment.
