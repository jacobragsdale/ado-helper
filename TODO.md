Note: Mark checklist items as completed (`[x]`) when the feature is implemented, documented, and covered by appropriate tests or smoke checks.

# Feature Ideas

- [ ] Add `ado pipeline logs`
  - Fetch logs for a pipeline run using the Azure DevOps Pipelines Logs REST API.
  - Support arguments for pipeline ID, run ID, and optional log ID. If no log ID is provided, list available logs for the run.
  - Consider `--follow` for polling active runs and streaming new log output until completion.
  - Preserve `--output json` for scriptable log metadata and use plain text for log content.

- [ ] Add `ado pipeline runs`
  - List recent runs for a pipeline using the Pipelines Runs REST API.
  - Accept a pipeline name or ID, matching the existing `ado pipeline run` behavior.
  - Include useful filters such as branch, state, result, and max count if the API supports them cleanly.
  - Table/text output should show run ID, run name, branch, state, result, created date, and finished date.

- [ ] Add `ado pipeline preview`
  - Use the Pipelines Preview REST API to render the final YAML for a pipeline without starting a run.
  - Accept pipeline name or ID, branch/ref, variables, and an optional YAML override file.
  - Print final YAML in text mode and the full preview response in JSON mode.
  - This should help debug templates, parameters, and variable expansion before queuing a real run.

- [ ] Add `ado wi query`
  - Run raw WIQL against the configured project using the Work Item Tracking WIQL REST API.
  - Support inline queries with `--wiql` and file-based queries with `--file`.
  - Fetch returned work item IDs in batches of up to 200, matching the current `wi list` pattern.
  - Reuse existing work item table/text/json output where possible.

- [ ] Add work item metadata discovery commands
  - Add commands such as `ado wi types`, `ado wi fields`, `ado wi areas`, and `ado wi iterations`.
  - Use Work Item Tracking metadata APIs for work item types, fields, and classification nodes.
  - Make output easy to copy into existing flags like `--type`, `--field`, `--area`, and `--iteration`.
  - Include project override support where relevant.

- [ ] Add `ado pr checks` or `ado pr policy`
  - Show pull request statuses, policy results, and branch policy requirements for a PR.
  - Use Pull Request Statuses and Policy Configurations REST APIs where possible.
  - Include status state, context/name, description, creator, updated date, and target URL.
  - This should answer why a PR can or cannot be completed without opening the browser.

- [ ] Add `ado pr checkout`
  - Fetch and checkout a pull request source branch locally.
  - Resolve the PR through the existing PR view logic, then use Git commands to fetch the source ref.
  - Support a default local branch name and an override such as `--branch`.
  - Keep behavior clear when the target branch already exists locally.

- [ ] Add repository branch, tag, and commit commands
  - Add commands such as `ado repo branches`, `ado repo tags`, and `ado repo commits`.
  - Use Git Refs and Commits REST APIs from Azure DevOps 7.1.
  - Support repo name inference from `ADO_REPO` or the current git remote where consistent with PR commands.
  - Table output should show names, object IDs, authors, dates, and comments where applicable.

- [ ] Add board/sprint helper commands
  - Add commands such as `ado board current`, `ado sprint`, or `ado backlog`.
  - Use Work APIs for team iterations, boards, and backlogs.
  - Support team selection with `--team`, defaulting to the project team when possible.
  - Focus on high-value summaries: current iteration, assigned work, backlog items, and item states.

- [ ] Add organization/project/team discovery commands
  - Add commands such as `ado project list`, `ado team list`, `ado team members`, and `ado me`.
  - Use Core Teams/Projects APIs and identity APIs where appropriate.
  - Help users discover valid project/team names and resolve people for reviewers or work item assignment.
  - Consider whether discovered defaults should integrate with `ado config set`.
