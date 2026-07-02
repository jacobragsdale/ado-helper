---
name: ado-refine-ticket
description: Use this skill when refining a single existing Azure DevOps work item — interview the developer, update Title/Description/AcceptanceCriteria, and attach a local design note.
disable-model-invocation: true
---

# ADO Refine Ticket

Refine one existing Azure DevOps work item so it is ready for implementation. The only input is `<wi-id>`, an integer work item ID. This skill does not change state, create a branch, or break the ticket into children — it is purely a refinement step.

After running this skill, the ticket should be in a state where `/ado-developer-workflow <wi-id>` can proceed without further refinement.

## Inputs

- `<wi-id>`: ADO work item ID (integer).

## Preconditions

- `ado` CLI on `$PATH` and configured for the target Azure DevOps org, project, and team. Do not inspect, print, request, log, or manage `ADO_PAT`; treat it as opaque.
- Current directory may be any directory; this skill writes a local design note relative to the current working directory.
- All Azure DevOps access goes through the local `ado` CLI. Do not use the ADO web UI, raw undocumented REST calls, or guessed JSON shapes. Discover commands with `ado --help`, `ado <subcmd> --help`. Discover JSON shapes with `ado schema <command-path>`.

## Command Surface

General discovery (run `ado <subcmd> --help` ad-hoc when a flag is unclear):

```sh
ado --help
```

JSON shape (the workflow parses this command's output):

```sh
ado schema wi view
```

ADO reads:

```sh
ado wi view <wi-id> --output json
```

ADO mutations (run only at the approval gate in step 5):

```sh
ado wi update <wi-id> --title "<title>" --description "<html>" --acceptance-criteria "<html>"
ado wi attach <wi-id> .ado/<wi-id>.design.md --comment "design (auto-generated)"
```

Local file commands:

```sh
mkdir -p .ado
test -f .gitignore
```

## Workflow

1. **Hydrate the ticket.** Run `ado wi view <wi-id> --output json`. Extract from `.fields`:
   - `System.Title`
   - `System.Description` (HTML)
   - `Microsoft.VSTS.Common.AcceptanceCriteria` (HTML)
   - `System.State`
   - `System.WorkItemType`
   - `System.AssignedTo.displayName`

   Convert Description and Acceptance Criteria from HTML to plain text for display (strip tags, decode entities, collapse whitespace). Print a one-screen summary:

   ```
   Ticket #<id> · <type> · <state> · assigned to <name>
   Title:        <title>
   Description:  <plain-text, ≤200 chars>
   AC:           <bullet list, or "(empty)">
   ```

2. **Interview the developer.** Show the existing acceptance criteria verbatim first so the developer can react to them rather than start blank. Then ask, one question at a time:

   1. **Approach** — what's the chosen implementation strategy at a high level?
   2. **Touchpoints** — which files, modules, services, or layers will change?
   3. **Success criteria** — what observable behavior defines done? (drives the new AC)
   4. **Edge cases / risks** — what could go wrong; anything to deliberately not handle?
   5. **Test strategy** — unit, integration, manual; concrete test names if known.

   Add at most one or two follow-ups when an answer is unclear. Five sharp questions beats nine soft ones.

3. **Compose updated field values.** Treat all developer-supplied prose as plain text. The `ado` CLI passes `--description` and `--acceptance-criteria` through unchanged — un-escaped prose will break the rendered field.

   Escape every prose value before inserting into any tag:

   | Char | Escape   |
   | ---- | -------- |
   | `&`  | `&amp;`  |
   | `<`  | `&lt;`   |
   | `>`  | `&gt;`   |
   | `"`  | `&quot;` |
   | `'`  | `&#39;`  |

   The only HTML tags this skill may generate are structural: `<p>`, `<ul>`, `<li>`, `<strong>`, `<em>`, `<br>`.

   - **Title:** plain text. Keep existing unless the interview produced a clearly better one.
   - **Description:** one or two short `<p>…</p>` paragraphs restating the problem plus the chosen approach.
   - **Acceptance Criteria:** a `<ul>` with one `<li>` per criterion. Prefer Given/When/Then phrasing. Each item must be independently verifiable.

4. **Persist the local design note.** The `.ado/` directory holds local design notes and must never be committed. If `.ado/` does not exist, create it and add `.ado/` to `.gitignore` (if not already ignored). Write `.ado/<wi-id>.design.md`:

   ```md
   # <wi-id> — <title>

   ## Approach
   <answer to Q1>

   ## Touchpoints
   <answer to Q2>

   ## Acceptance Criteria
   <markdown bullet list mirroring the AC field>

   ## Risks
   <answer to Q4>

   ## Test strategy
   <answer to Q5>
   ```

5. **Approval gate.** Show a diff-style preview for every field that changed:

   ```
   Title:
     - <old>
     + <new>

   AC:
     - <old bullets>
     + <new bullets>
   ```

   Wait for explicit yes. If the developer wants edits, revise the design note and field values, then re-show. Do not proceed to step 6 without approval. A "yes" approves the previewed change only; subsequent changes need new approvals.

6. **Apply the field updates.** Run one `ado wi update` containing only the changed fields. If no fields changed, skip the update.

   ```sh
   ado wi update <wi-id> \
     --title "<new title>" \
     --description "<new html description>" \
     --acceptance-criteria "<new html ac>"
   ```

   Then attach the design note:

   ```sh
   ado wi attach <wi-id> .ado/<wi-id>.design.md --comment "design (auto-generated)"
   ```

   Do not post any comment on the work item. Do not transition state.

## Failure Handling

ADO exit codes are stable: `0` success, `1` unclassified, `2` not found, `3` validation, `4` auth, `5` ADO API error. Any non-zero `ado` exit aborts with the phase name, the command, the exit code, and stderr verbatim. Specific cases:

- `2`: name the missing resource (e.g. `"ticket <wi-id> not found - check the ID."`).
- `4`: `"ADO auth failed - check the local ado setup."`
- `5`: surface stderr without reinterpretation.

Do not promise or attempt rollback. If `ado wi update` succeeds but `ado wi attach` fails, report both: the updated field names and the exact failing attach command. Do not retry the update.

If the developer rejects the preview at step 5, revise and re-preview; do not abort.

## Output

Print the success line on its own as the final line of stdout, with no formatting:

```
Refined ticket AB#<wi-id>: <N> fields updated, design.md attached.
```

Where `<N>` is the count of fields included in the `ado wi update` call (0 if only the attach ran). On abort:

```
ABORT Phase <n> - <name>: <short reason>
```
