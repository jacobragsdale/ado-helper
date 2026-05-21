# Repo Inventory

This folder is a semi-structured map of the repos, environments, deployments, pipelines, and operational breadcrumbs used by the team. It is meant to be filled in gradually and used by AI workflows before they create tickets, summarize sprint work, generate QA plans, or reason about cross-repo changes.

Do not store secrets here. Record where secrets live, who can grant access, and which secret names are required, but never paste secret values, PATs, passwords, private keys, or production tokens.

## Structure

```text
repo-inventory/
  README.md
  index.md
  projects/
    _template.md
  repos/
    _template.md
  environments/
    _template.md
```

Use one file per project group, repo, and environment:

- `projects/<project-slug>.md` groups related repos by product area or workflow.
- `repos/<repo-name>.md` describes one repository and how to work with it.
- `environments/<environment-slug>.md` describes deployed instances, images, logs, and pipelines for an environment.

## Good Defaults

- Prefer exact local paths, repo names, ADO project names, pipeline names, image names, and log locations.
- Keep descriptions short but concrete.
- Link related files by relative path, for example `../repos/api-service.md`.
- Use `unknown` when a field is not known yet. That is better than leaving important fields invisible.
- Keep local machine paths separate from remote clone URLs.
- Keep product behavior grouped in `projects/`; keep implementation and ops detail in `repos/` and `environments/`.

## Useful Discovery Commands

Run these from a repo checkout:

```sh
pwd
git remote -v
git branch --show-current
git rev-parse --show-toplevel
```

Use `ado` for Azure DevOps metadata:

```sh
ado repo list --output table
ado repo branches --repo <repo-name> --output table
ado pipeline list --output table
ado pipeline status <run-id> --pipeline-id <pipeline-id>
ado wi query --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.ChangedDate] DESC" --output table
```

Use local/container tooling as applicable:

```sh
docker images
docker ps
kubectl config current-context
kubectl get pods --all-namespaces
```

Only include command output after reviewing it for secrets.
