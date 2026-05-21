# Engineering Manager Automation Ideas

You now have two high-leverage AI workflows:

- **Idea to tickets:** turn a rough feature idea into a parent ADO item, dev tasks, and a QA task.
- **Tickets to code:** take a refined ADO ticket through implementation, validation, and PR creation.

That covers the two big rails. The next automation layer is everything around those rails: intake, prioritization, sprint health, QA, release communication, and team leverage.

## Context Notes

- The team works across many repos, currently more than 20.
- Any automation that summarizes work, plans sprints, routes reviews, creates QA packets, or generates release notes needs to be repo-aware.
- Ticket creation should continue to make repo ownership explicit so junior developers and QA analysts are not forced to infer where work belongs.

## Backlog and Intake

- **Idea inbox:** turn rough notes, customer asks, Slack snippets, meeting notes, or voice-dump style ideas into candidate ADO work items.
- **Duplicate detector:** compare a new idea against existing ADO tickets and warn when it sounds like already-planned work.
- **Ticket quality auditor:** scan backlog items for missing repo, vague descriptions, missing AC, missing QA plan, no estimate, or unclear owner.
- **Ready-for-dev checker:** mark tickets as ready only when scope, repo, AC, dependencies, QA path, and estimate are present.
- **Auto-prioritizer:** suggest priority from customer impact, urgency, risk, dependencies, and effort.
- **Dependency mapper:** identify tickets blocked by other tickets, PRs, teams, migrations, or undecided product/technical questions.

## Sprint Planning

- **Capacity-aware sprint planner:** given candidate tickets, estimates, and team capacity, propose a sprint plan.
- **Carryover assistant:** inspect unfinished work, summarize why it carried over, and recommend rollover, split, close, or re-plan.
- **Sprint risk forecast:** flag overload, sequencing problems, single-owner bottlenecks, high-risk tickets, and cross-repo dependency clusters.
- **Task balancer:** suggest assignments based on repo ownership, prior work, capacity, and learning goals.
- **Sprint goal generator:** summarize selected tickets into a crisp sprint goal.
- **Mid-sprint scope watcher:** detect work added after sprint start and explain its impact on committed work.

## Daily Management

- **Standup digest:** infer yesterday/today/blockers per engineer from ADO state, PRs, commits, comments, and CI status.
- **Blocker detector:** flag items with no movement, repeated failed CI, old PRs, unresolved review comments, or missing dependencies.
- **Manager question assistant:** suggest what to ask about in standup or 1:1s based on stale work, risk, and open decisions.
- **Aging WIP report:** list tickets stuck in Doing, PRs open too long, QA tickets idle, and parent items whose children are not moving.
- **Focus recommender:** for each engineer, suggest the next best ticket to pick up.

## PR and Code Review

- **PR readiness checker:** confirm linked work item, passing checks, test coverage, AC coverage, and risk notes before review.
- **Review routing:** suggest reviewers based on touched repo/files, ownership history, domain knowledge, and current review load.
- **PR summary for managers:** summarize what changed, why it matters, linked tickets, risk level, and test evidence.
- **Stale review nudge generator:** create polite Slack/Teams/ADO-ready reminders for old PRs or unresolved comments.
- **CI failure explainer:** summarize failing pipeline logs and suggest likely ownership or next investigation steps.

## QA and Release

- **QA packet generator:** from a parent item and its dev tasks, produce a single validation guide with setup, environments, test data, expected behavior, regression checks, and evidence requirements.
- **Test evidence collector:** remind QA what screenshots, logs, links, data records, or run results should be attached to the QA task.
- **Bug-from-failure creator:** turn failed QA notes into a clean bug ticket with repro steps, expected/actual behavior, environment, severity, and links back to the parent.
- **Release notes generator:** summarize completed parent issues into stakeholder-friendly release notes.
- **Release readiness checklist:** flag open bugs, incomplete QA, failed builds, missing approvals, migrations, feature flags, and unresolved rollout risks.
- **Regression suite recommender:** given changed areas and repos, suggest manual or automated regression checks.

## Stakeholder Communication

- **Weekly engineering update:** summarize shipped work, in-progress work, blockers, risks, and next-week focus.
- **Exec digest:** create a non-technical summary with impact, confidence, dates, and asks.
- **Customer-facing changelog draft:** translate completed work into release-note language suitable for customers.
- **Risk register:** automatically maintain risks from blocked tickets, aging work, cross-repo dependencies, and incidents.
- **Decision log:** capture important technical/product decisions from ticket comments, PRs, and planning notes.

## Team Growth

- **Junior task coach:** generate "how to approach this task" guidance from a ticket without solving the implementation outright.
- **Learning opportunity matcher:** suggest lower-risk tickets for engineers who want growth in a repo, domain, or technology.
- **Code review feedback summarizer:** identify recurring themes per engineer for coaching conversations.
- **Onboarding path generator:** create a first-five-tickets path to learn a repo, product area, or team workflow.
- **Bus factor report:** identify repos or features mostly touched by one person.

## Operational Hygiene

- **Stale backlog cleanup:** recommend tickets to close, archive, refresh, split, or re-prioritize.
- **Orphan detector:** find PRs without tickets, tickets without PRs, QA tasks without dev parents, and children without parents.
- **State consistency checker:** flag parent Done with active children, QA Done before dev merge, tickets Doing with no branch or PR, and merged PRs whose tickets remain active.
- **Estimate drift report:** compare remaining work, actual cycle time, carryover, and repeated underestimation patterns.
- **ADO taxonomy enforcer:** keep tags, areas, iterations, activity fields, parent links, and repo notation consistent.

## Best Starting Points

1. **Sprint planner:** takes candidate tickets, capacity, team members, and cross-repo constraints, then proposes a sprint.
2. **Standup/team status digest:** gives daily visibility without manually checking ADO and PRs.
3. **Ticket quality auditor:** keeps the backlog from decaying.
4. **Release/stakeholder update generator:** turns completed work into useful communication for leadership, product, support, or customers.
5. **QA packet and bug-from-QA workflow:** closes the loop between dev completion, validation, failed QA, and follow-up bugs.

## Starting Focus

Start by refining:

- **Item 4: Product stakeholder update generator**
  - Read-only workflow.
  - Produces a markdown file only; do not auto-send AI output to clients, Slack, Teams, email, or ADO comments.
  - Intended audience is product/client stakeholders, not engineers.
  - Source scope is a sprint.
  - Should summarize completed work plus notable in-progress and blocked items from the sprint.
  - Should explain work in product terms: what changed, why it matters, user impact, known limitations, blocked/in-progress context, and what is next.
  - Do not mention ticket IDs in the stakeholder-facing body.
  - Do not mention QA status.
  - Should hide noisy implementation detail while still naming meaningful scope, risks, and delivery-impacting blockers.

- **Item 5: Sprint QA test plan aggregator**
  - Read-only planning workflow.
  - Starts from all QA tasks in a sprint where `Microsoft.VSTS.Common.Activity` / `activity` is `Testing`; those QA tasks should already be well defined by the idea-to-ticket workflow.
  - Aggregates the QA tasks into one sprint-level test plan.
  - Removes duplicate or repeated validation steps when they cover the same behavior.
  - Groups similar testing areas by product area and user workflow so QA can move through related checks efficiently.
  - Includes a recommended execution order, such as smoke checks first, high-risk workflows next, then broader regression.
  - QA testing happens on deployed instances, so do not include environment setup sections by default.
  - Outputs just the test steps and expected outcomes; do not add sign-off checkboxes or evidence placeholders by default.
  - Should preserve traceability back to the original QA task IDs and parent work items.

These pair well with the existing idea-to-ticket and ticket-to-code workflows because they cover the handoff after implementation: validation, evidence, release confidence, and stakeholder communication.
