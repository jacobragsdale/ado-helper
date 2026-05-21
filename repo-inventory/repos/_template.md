# <Repo Name>

## Summary

- **Project group:** unknown
- **Purpose:** unknown
- **Primary language/framework:** unknown
- **Primary owner:** unknown
- **Backup owner:** unknown
- **Status:** active | maintenance | deprecated | unknown

## Local Checkout

- **Local path:** unknown
- **Default branch:** unknown
- **Package manager/toolchain:** unknown
- **Local setup command:** unknown
- **Local run command:** unknown
- **Local test command:** unknown
- **Local lint/typecheck command:** unknown

## Remotes

```text
origin: unknown
upstream: unknown
```

## Azure DevOps

- **ADO organization:** unknown
- **ADO project:** unknown
- **ADO repo name:** unknown
- **ADO repo URL:** unknown
- **Default branch:** unknown
- **Clone command:** `ado repo clone <repo-name> <target-dir>`

Useful commands:

```sh
ado repo branches --repo <repo-name> --output table
ado repo commits --repo <repo-name> --branch main --max 10 --output table
ado pr list --repo <repo-name> --status active --output table
```

## Pipelines

| Pipeline | Purpose | Trigger | Required for release? | Notes |
| --- | --- | --- | --- | --- |
| unknown | build/test/deploy | unknown | unknown | unknown |

Useful commands:

```sh
ado pipeline list --output table
ado pipeline preview <pipeline-name-or-id> --ref refs/heads/main --output json
ado pipeline status <run-id> --pipeline-id <pipeline-id>
ado pipeline logs <run-id> --pipeline-id <pipeline-id>
```

## Images and Artifacts

| Name | Type | Registry/location | Built by | Deployed to | Notes |
| --- | --- | --- | --- | --- | --- |
| unknown | container/package/static artifact | unknown | unknown | unknown | unknown |

## Environments

| Environment | URL/location | Deploy mechanism | Config source | Notes |
| --- | --- | --- | --- | --- |
| unknown | unknown | unknown | unknown | unknown |

## Logs and Observability

| Signal | Location | Query/filter | Notes |
| --- | --- | --- | --- |
| app logs | unknown | unknown | unknown |
| pipeline logs | unknown | unknown | unknown |
| metrics/dashboard | unknown | unknown | unknown |
| alerts | unknown | unknown | unknown |

## Configuration and Secrets

Do not paste secret values.

| Name/prefix | Purpose | Where managed | Required locally? | Notes |
| --- | --- | --- | --- | --- |
| unknown | unknown | unknown | unknown | unknown |

## Data Stores and External Services

| Dependency | Purpose | Environment(s) | Notes |
| --- | --- | --- | --- |
| unknown | unknown | unknown | unknown |

## Common Workflows

### Add or change a feature

1. unknown

### Run tests

1. unknown

### Deploy

1. unknown

### Debug production/staging issue

1. unknown

## Related Repos

| Repo | Relationship | Notes |
| --- | --- | --- |
| unknown | unknown | unknown |

## AI Workflow Hints

- **When creating tickets:** include repo name, likely touchpoints, test command, and deployment notes.
- **When implementing tickets:** inspect this repo's local setup, tests, pipelines, and related repos first.
- **When creating QA plans:** validate product workflows, not repository internals.
- **When creating product updates:** translate technical changes into product impact.
