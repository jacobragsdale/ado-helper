#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f "$ROOT/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  . "$ROOT/.env"
  set +a
fi

RUN_STAMP="$(date +%Y%m%d-%H%M%S)"
RUN_ID="${ADO_E2E_RUN_ID:-ado-live-e2e-$RUN_STAMP}"
RUN_DIR="${ADO_E2E_RUN_DIR:-$ROOT/smoke/$RUN_ID}"
OUT_DIR="$RUN_DIR/out"
ERR_DIR="$RUN_DIR/err"
STATUS_DIR="$RUN_DIR/status"
TMP_DIR="$RUN_DIR/tmp"
BIN_DIR="$RUN_DIR/bin"
HOME_DIR="$RUN_DIR/home"
WORK_DIR="$RUN_DIR/work"
REPORT="$RUN_DIR/report.md"
REAL_HOME="${HOME:-}"

mkdir -p "$OUT_DIR" "$ERR_DIR" "$STATUS_DIR" "$TMP_DIR" "$BIN_DIR" "$HOME_DIR" "$WORK_DIR"
: > "$REPORT"

PASS=0
FAIL=0
SKIP=0
FAILURES=()
SKIPS=()
WORK_ITEM_IDS=()
PR_IDS=()

REPO_NAME="${ADO_E2E_REPO_NAME:-ado-live-e2e-${RUN_STAMP//-/}-$$}"
PIPELINE_NAME="${ADO_E2E_PIPELINE_NAME:-$REPO_NAME-pipeline}"
PIPELINE_ID=""
PIPELINE_RUN_ID=""
ADO_BIN="$ROOT/target/debug/ado"
CLONE_DIR="$WORK_DIR/$REPO_NAME"
KEEP_RESOURCES="${ADO_E2E_KEEP_RESOURCES:-0}"
CLEANED_UP=0

export RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"
export HOME="$HOME_DIR"
export PATH="$BIN_DIR:$PATH"
export GIT_TERMINAL_PROMPT=0
export GIT_ASKPASS="$BIN_DIR/git-askpass"
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL="$TMP_DIR/gitconfig"
# Keep every git invocation in this smoke run away from macOS Keychain. The
# empty credential.helper value resets helper lists from any lower-precedence
# config, and the temporary global config keeps user config isolated.
export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0=credential.helper
export GIT_CONFIG_VALUE_0=

cat > "$GIT_ASKPASS" <<'SH'
#!/usr/bin/env sh
case "$1" in
  *Username*) printf 'ado\n' ;;
  *Password*) printf '%s\n' "$ADO_PAT" ;;
  *) printf '\n' ;;
esac
SH
chmod +x "$GIT_ASKPASS"

cat > "$GIT_CONFIG_GLOBAL" <<'GITCONFIG'
[credential]
	helper =
[credential "https://dev.azure.com"]
	useHttpPath = true
GITCONFIG

cat > "$BIN_DIR/open" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$*" >> "${ADO_E2E_BROWSER_LOG:-/tmp/ado-live-e2e-browser.log}"
exit 0
SH
chmod +x "$BIN_DIR/open"

cat > "$BIN_DIR/xdg-open" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$*" >> "${ADO_E2E_BROWSER_LOG:-/tmp/ado-live-e2e-browser.log}"
exit 0
SH
chmod +x "$BIN_DIR/xdg-open"
export ADO_E2E_BROWSER_LOG="$RUN_DIR/browser-open.log"

note() {
  printf '%s\n' "$*" | tee -a "$REPORT"
}

ok() {
  PASS=$((PASS + 1))
  note "PASS $1"
}

fail() {
  FAIL=$((FAIL + 1))
  FAILURES+=("$1")
  note "FAIL $1"
}

skip() {
  SKIP=$((SKIP + 1))
  SKIPS+=("$1: $2")
  note "SKIP $1 - $2"
}

safe_name() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

artifact_out() {
  printf '%s/%s.out' "$OUT_DIR" "$(safe_name "$1")"
}

artifact_err() {
  printf '%s/%s.err' "$ERR_DIR" "$(safe_name "$1")"
}

artifact_status() {
  printf '%s/%s.status' "$STATUS_DIR" "$(safe_name "$1")"
}

run_inner() {
  local name="$1"
  local expected="$2"
  local dir="$3"
  local stdin_file="$4"
  shift 4

  local out err status_file
  out="$(artifact_out "$name")"
  err="$(artifact_err "$name")"
  status_file="$(artifact_status "$name")"

  note "RUN $name"
  if [[ -n "$stdin_file" ]]; then
    (cd "$dir" && "$@") < "$stdin_file" > "$out" 2> "$err"
  else
    (cd "$dir" && "$@") > "$out" 2> "$err"
  fi
  local code=$?
  printf '%s\n' "$code" > "$status_file"

  if [[ "$code" -eq "$expected" ]]; then
    ok "$name"
    return 0
  fi

  fail "$name (exit $code, expected $expected; stdout=$out stderr=$err)"
  return 1
}

run_cmd() {
  local name="$1"
  shift
  run_inner "$name" 0 "$ROOT" "" "$@"
}

run_in_dir() {
  local name="$1"
  local dir="$2"
  shift 2
  run_inner "$name" 0 "$dir" "" "$@"
}

run_with_stdin() {
  local name="$1"
  local stdin_file="$2"
  shift 2
  run_inner "$name" 0 "$ROOT" "$stdin_file" "$@"
}

run_expect_code() {
  local name="$1"
  local expected="$2"
  shift 2
  run_inner "$name" "$expected" "$ROOT" "" "$@"
}

run_allow_codes() {
  local name="$1"
  local allowed="$2"
  local dir="$3"
  shift 3

  local out err status_file
  out="$(artifact_out "$name")"
  err="$(artifact_err "$name")"
  status_file="$(artifact_status "$name")"

  note "RUN $name"
  (cd "$dir" && "$@") > "$out" 2> "$err"
  local code=$?
  printf '%s\n' "$code" > "$status_file"

  local allowed_code
  for allowed_code in $allowed; do
    if [[ "$code" -eq "$allowed_code" ]]; then
      if [[ "$code" -eq 0 ]]; then
        ok "$name"
      else
        skip "$name" "exit $code is allowed for this data-dependent command"
      fi
      return 0
    fi
  done

  fail "$name (exit $code, allowed: $allowed; stdout=$out stderr=$err)"
  return 1
}

assert_jq() {
  local name="$1"
  local file="$2"
  local expr="$3"
  local err="$ERR_DIR/$(safe_name "$name").jq.err"

  if jq -e "$expr" "$file" > /dev/null 2> "$err"; then
    ok "$name"
  else
    fail "$name (jq assertion failed: $expr; file=$file stderr=$err)"
  fi
}

assert_jq_arg() {
  local name="$1"
  local file="$2"
  local arg_name="$3"
  local arg_value="$4"
  local expr="$5"
  local err="$ERR_DIR/$(safe_name "$name").jq.err"

  if jq -e --arg "$arg_name" "$arg_value" "$expr" "$file" > /dev/null 2> "$err"; then
    ok "$name"
  else
    fail "$name (jq assertion failed: $expr; file=$file stderr=$err)"
  fi
}

urlencode_segment() {
  python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"
}

api_url() {
  local path="$1"
  printf '%s/%s' "${ADO_ORG_URL%/}" "${path#/}"
}

api_request() {
  local method="$1"
  local path="$2"
  local body_file="$3"
  local out_file="$4"
  local url http

  url="$(api_url "$path")"
  if [[ -n "$body_file" ]]; then
    http="$(curl -sS -u ":$ADO_PAT" -o "$out_file" -w "%{http_code}" \
      -X "$method" \
      -H "Accept: application/json" \
      -H "Content-Type: application/json" \
      --data-binary "@$body_file" \
      "$url")"
  else
    http="$(curl -sS -u ":$ADO_PAT" -o "$out_file" -w "%{http_code}" \
      -X "$method" \
      -H "Accept: application/json" \
      "$url")"
  fi
  printf '%s\n' "$http" > "$out_file.http"
  [[ "$http" =~ ^2 ]]
}

cleanup_resources() {
  CLEANED_UP=1
  note ""
  note "## Cleanup"

  if [[ "$KEEP_RESOURCES" == "1" ]]; then
    note "Keeping ADO resources because ADO_E2E_KEEP_RESOURCES=1."
    note "Repo: $REPO_NAME"
    [[ -n "$PIPELINE_ID" ]] && note "Pipeline ID: $PIPELINE_ID"
    [[ "${#WORK_ITEM_IDS[@]}" -gt 0 ]] && note "Work items: ${WORK_ITEM_IDS[*]}"
    return
  fi

  local pr_id wi_id
  if [[ -n "$REPO_NAME" ]]; then
    for pr_id in "${PR_IDS[@]}"; do
      "$ADO_BIN" pr abandon "$pr_id" --repo "$REPO_NAME" --output json \
        > "$OUT_DIR/cleanup-pr-$pr_id.out" 2> "$ERR_DIR/cleanup-pr-$pr_id.err" || true
    done
  fi

  for wi_id in "${WORK_ITEM_IDS[@]}"; do
    "$ADO_BIN" wi delete "$wi_id" \
      > "$OUT_DIR/cleanup-wi-$wi_id.out" 2> "$ERR_DIR/cleanup-wi-$wi_id.err" || true
  done

  if [[ -n "$PIPELINE_ID" ]]; then
    local delete_out="$OUT_DIR/cleanup-pipeline-$PIPELINE_ID.out"
    api_request DELETE "$PROJECT_SEG/_apis/build/definitions/$PIPELINE_ID?api-version=7.1" "" "$delete_out" \
      || api_request DELETE "$PROJECT_SEG/_apis/pipelines/$PIPELINE_ID?api-version=7.1" "" "$delete_out" \
      || note "Pipeline cleanup failed for ID $PIPELINE_ID; see $delete_out"
  fi

  if [[ -n "$REPO_NAME" ]]; then
    "$ADO_BIN" repo delete "$REPO_NAME" --yes \
      > "$OUT_DIR/cleanup-repo-delete.out" 2> "$ERR_DIR/cleanup-repo-delete.err" || true
  fi
}

finish() {
  local code=$?
  if [[ "$CLEANED_UP" -eq 0 ]]; then
    cleanup_resources
  fi
  exit "$code"
}
trap finish EXIT INT TERM

require_env() {
  local missing=0
  local key
  for key in ADO_ORG_URL ADO_PROJECT ADO_PAT; do
    if [[ -z "${!key:-}" ]]; then
      note "Missing required environment variable: $key"
      missing=1
    fi
  done
  if [[ "$missing" -ne 0 ]]; then
    exit 3
  fi
}

wait_for_pipeline_completion() {
  local timeout="${ADO_E2E_PIPELINE_TIMEOUT_SECONDS:-300}"
  local deadline=$((SECONDS + timeout))
  local out="$OUT_DIR/pipeline-status-wait.out"
  local err="$ERR_DIR/pipeline-status-wait.err"
  local state result

  while (( SECONDS < deadline )); do
    "$ADO_BIN" pipeline status "$PIPELINE_RUN_ID" --pipeline-id "$PIPELINE_ID" --output json \
      > "$out" 2> "$err"
    if [[ $? -eq 0 ]]; then
      state="$(jq -r '.state // ""' "$out" 2> /dev/null || true)"
      result="$(jq -r '.result // ""' "$out" 2> /dev/null || true)"
      if [[ "$state" == "completed" ]]; then
        if [[ "$result" == "succeeded" ]]; then
          ok "pipeline-status-wait-succeeded"
        else
          fail "pipeline-status-wait-succeeded (result=$result; file=$out)"
        fi
        return
      fi
    fi
    sleep 10
  done

  fail "pipeline-status-wait-succeeded (timed out after ${timeout}s; stdout=$out stderr=$err)"
}

create_pipeline() {
  local body="$TMP_DIR/pipeline-create-body.json"
  local out="$OUT_DIR/setup-pipeline-create.out"
  jq -n \
    --arg name "$PIPELINE_NAME" \
    --arg repo_id "$REPO_ID" \
    --arg repo_name "$REPO_NAME" \
    --arg path "/azure-pipelines.ado-helper-smoke.yml" \
    '{
      name: $name,
      configuration: {
        type: "yaml",
        path: $path,
        repository: {
          id: $repo_id,
          name: $repo_name,
          type: "azureReposGit"
        }
      }
    }' > "$body"

  note "RUN setup-pipeline-create"
  if api_request POST "$PROJECT_SEG/_apis/pipelines?api-version=7.1" "$body" "$out"; then
    PIPELINE_ID="$(jq -r '.id // empty' "$out")"
    if [[ -n "$PIPELINE_ID" ]]; then
      ok "setup-pipeline-create"
    else
      fail "setup-pipeline-create (response had no id; file=$out)"
    fi
  else
    fail "setup-pipeline-create (HTTP $(cat "$out.http"); file=$out)"
  fi
}

write_pipeline_yaml() {
  cat > "$CLONE_DIR/azure-pipelines.ado-helper-smoke.yml" <<'YAML'
trigger: none
pr: none

parameters:
  - name: environment
    type: string
    default: dev

pool:
  server

steps:
  - task: Delay@1
    displayName: "Wait briefly for ado-helper live e2e (${{ parameters.environment }})"
    inputs:
      delayForMinutes: "1"
YAML
}

write_readme() {
  cat > "$CLONE_DIR/README.md" <<EOF_README
# $REPO_NAME

Disposable Azure DevOps repository created by ado live E2E run $RUN_ID.
EOF_README
}

write_branch_file() {
  local branch="$1"
  local file="$CLONE_DIR/changes/$branch.txt"
  mkdir -p "$CLONE_DIR/changes"
  cat > "$file" <<EOF_BRANCH
$RUN_ID
$branch
EOF_BRANCH
}

create_branch() {
  local branch="$1"
  run_in_dir "git-checkout-main-before-$branch" "$CLONE_DIR" git checkout main
  run_in_dir "git-checkout-new-$branch" "$CLONE_DIR" git checkout -b "$branch"
  write_branch_file "$branch"
  run_in_dir "git-add-$branch" "$CLONE_DIR" git add "changes/$branch.txt"
  run_in_dir "git-commit-$branch" "$CLONE_DIR" git commit -m "$RUN_ID $branch"
  run_in_dir "git-push-$branch" "$CLONE_DIR" git push -u origin "$branch"
}

require_env
PROJECT_SEG="$(urlencode_segment "$ADO_PROJECT")"

note "# ado live E2E report"
note ""
note "- Run: $RUN_ID"
note "- Org: ${ADO_ORG_URL%/}"
note "- Project: $ADO_PROJECT"
note "- Repo: $REPO_NAME"
note "- Output: $RUN_DIR"
note ""

run_cmd "cargo-build" cargo build --bin ado

run_cmd "help-root" "$ADO_BIN" --help
run_cmd "help-config" "$ADO_BIN" config --help
run_cmd "help-me" "$ADO_BIN" me --help
run_cmd "help-team" "$ADO_BIN" team --help
run_cmd "help-iteration" "$ADO_BIN" iteration --help
run_cmd "help-area" "$ADO_BIN" area --help
run_cmd "help-schema" "$ADO_BIN" schema --help
run_cmd "help-sprint" "$ADO_BIN" sprint --help
run_cmd "help-sprint-backlog" "$ADO_BIN" sprint backlog --help
run_cmd "help-sprint-board" "$ADO_BIN" sprint board --help
run_cmd "help-sprint-plan-into" "$ADO_BIN" sprint plan-into --help
run_cmd "help-sprint-capacity" "$ADO_BIN" sprint capacity --help
run_cmd "help-sprint-burndown" "$ADO_BIN" sprint burndown --help
run_cmd "help-sprint-rollover" "$ADO_BIN" sprint rollover --help
run_cmd "help-sprint-summary" "$ADO_BIN" sprint summary --help
run_cmd "help-repo" "$ADO_BIN" repo --help
run_cmd "help-pr" "$ADO_BIN" pr --help
run_cmd "help-pipeline" "$ADO_BIN" pipeline --help
run_cmd "help-wi" "$ADO_BIN" wi --help
run_cmd "readme-contract-pr-checkout-clean-help" "$ADO_BIN" pr checkout-clean --help
run_cmd "help-pr-checkout" "$ADO_BIN" pr checkout --help
if grep -Fq -- "--detach" "$(artifact_out help-pr-checkout)"; then
  ok "readme-contract-pr-checkout-detach-help"
else
  fail "readme-contract-pr-checkout-detach-help (--detach is documented in README but missing from pr checkout --help)"
fi

run_cmd "schema-list-json" "$ADO_BIN" schema --list --output json
assert_jq "schema-list-is-array" "$(artifact_out schema-list-json)" 'type == "array" and length > 20'

while IFS= read -r schema_path; do
  read -r -a parts <<< "$schema_path"
  run_cmd "schema-$schema_path" "$ADO_BIN" schema "${parts[@]}"
  assert_jq "schema-$schema_path-valid-json" "$(artifact_out "schema-$schema_path")" 'type == "object" and has("$schema")'
done < <(jq -r '.[]' "$(artifact_out schema-list-json)")

CONFIG_SET_ARGS=(config set --org "$ADO_ORG_URL" --project "$ADO_PROJECT")
if [[ -n "${ADO_TEAM:-}" ]]; then
  CONFIG_SET_ARGS+=(--team "$ADO_TEAM")
fi
run_cmd "config-set" "$ADO_BIN" "${CONFIG_SET_ARGS[@]}"
run_cmd "config-show" "$ADO_BIN" config show

run_cmd "me-refresh-json" "$ADO_BIN" me refresh --output json
assert_jq "me-refresh-has-identity" "$(artifact_out me-refresh-json)" '(.id // "") != "" or (.unique_name // "") != ""'
run_cmd "me-cached-text" "$ADO_BIN" me

run_cmd "team-list-json" "$ADO_BIN" team list --output json
assert_jq "team-list-has-array" "$(artifact_out team-list-json)" '.value | type == "array"'
if [[ -z "${ADO_TEAM:-}" ]]; then
  ADO_TEAM="$(jq -r '.value[0].name // empty' "$(artifact_out team-list-json)")"
  export ADO_TEAM
fi

if [[ -n "${ADO_TEAM:-}" ]]; then
  run_cmd "team-set" "$ADO_BIN" team set "$ADO_TEAM"
  run_cmd "team-current" "$ADO_BIN" team current
  run_cmd "team-members-json" "$ADO_BIN" team members --output json
  assert_jq "team-members-has-array" "$(artifact_out team-members-json)" '.value | type == "array"'
  run_allow_codes "iteration-list-json" "0 2" "$ROOT" "$ADO_BIN" iteration list --output json
  run_allow_codes "iteration-current-json" "0 2" "$ROOT" "$ADO_BIN" iteration current --output json
  run_allow_codes "iteration-next-json" "0 2" "$ROOT" "$ADO_BIN" iteration next --output json
  run_allow_codes "iteration-view-current-json" "0 2" "$ROOT" "$ADO_BIN" iteration view @current --output json
  run_allow_codes "iteration-view-previous-json" "0 2" "$ROOT" "$ADO_BIN" iteration view @previous --output json
  run_allow_codes "sprint-backlog-current-json" "0 2" "$ROOT" "$ADO_BIN" sprint backlog --iteration @current --output json
  run_allow_codes "sprint-board-current-json" "0 2" "$ROOT" "$ADO_BIN" sprint board --iteration @current --output json
  run_allow_codes "sprint-capacity-json" "0 2" "$ROOT" "$ADO_BIN" sprint capacity --output json
  run_allow_codes "sprint-burndown-json" "0 2" "$ROOT" "$ADO_BIN" sprint burndown --output json
  run_allow_codes "sprint-summary-json" "0 2" "$ROOT" "$ADO_BIN" sprint summary --output json
  run_allow_codes "sprint-rollover-dry-run-json" "0 2" "$ROOT" "$ADO_BIN" sprint rollover --dry-run --output json
else
  skip "team-set/current/members/iteration" "no team exists in project"
fi

run_cmd "area-list-json" "$ADO_BIN" area list --depth 3 --output json
assert_jq "area-list-is-array" "$(artifact_out area-list-json)" 'type == "array"'
run_cmd "area-tree-json" "$ADO_BIN" area tree --depth 3 --output json
assert_jq "area-tree-has-name" "$(artifact_out area-tree-json)" '(.name // "") != ""'

run_cmd "wi-types-json" "$ADO_BIN" wi types --output json
assert_jq "wi-types-has-array" "$(artifact_out wi-types-json)" '.value | type == "array" and length > 0'
WI_TYPE="$(jq -r '
  [.value[] | select((.isDisabled // false) | not) | .name] as $names
  | if ($names | index("Task")) then "Task"
    elif ($names | index("Bug")) then "Bug"
    elif ($names | index("User Story")) then "User Story"
    else $names[0]
    end
' "$(artifact_out wi-types-json)")"
if [[ -z "$WI_TYPE" || "$WI_TYPE" == "null" ]]; then
  fail "select-work-item-type (no enabled work item type found)"
  WI_TYPE="Task"
else
  ok "select-work-item-type ($WI_TYPE)"
fi
run_cmd "wi-states-json" "$ADO_BIN" wi states "$WI_TYPE" --output json
assert_jq "wi-states-has-array" "$(artifact_out wi-states-json)" '.value | type == "array" and length > 0'
run_cmd "wi-fields-json" "$ADO_BIN" wi fields --output json
assert_jq "wi-fields-has-array" "$(artifact_out wi-fields-json)" '.value | type == "array" and length > 0'
run_cmd "wi-fields-type-json" "$ADO_BIN" wi fields --type "$WI_TYPE" --output json
assert_jq "wi-fields-type-has-array" "$(artifact_out wi-fields-type-json)" '.value | type == "array"'

run_cmd "repo-list-json" "$ADO_BIN" repo list --output json
assert_jq "repo-list-has-array" "$(artifact_out repo-list-json)" '.value | type == "array"'
run_cmd "repo-create-json" "$ADO_BIN" repo create --name "$REPO_NAME" --output json
assert_jq_arg "repo-create-name" "$(artifact_out repo-create-json)" repo "$REPO_NAME" '.name == $repo'
REPO_ID="$(jq -r '.id' "$(artifact_out repo-create-json)")"
REMOTE_URL="$(jq -r '.remoteUrl // .remote_url // empty' "$(artifact_out repo-create-json)")"

run_cmd "repo-clone" "$ADO_BIN" repo clone "$REPO_NAME" "$CLONE_DIR"
run_in_dir "repo-clone-origin-url-safe" "$CLONE_DIR" git remote get-url origin
if grep -Fq "${ADO_PAT:-__missing_pat__}" "$(artifact_out repo-clone-origin-url-safe)"; then
  fail "repo-clone-origin-url-safe (PAT was left in origin URL)"
else
  ok "repo-clone-origin-url-safe-no-pat"
fi

run_in_dir "git-config-user-email" "$CLONE_DIR" git config user.email "ado-live-e2e@example.invalid"
run_in_dir "git-config-user-name" "$CLONE_DIR" git config user.name "ado live e2e"
run_in_dir "git-checkout-new-main" "$CLONE_DIR" git checkout -b main
write_readme
write_pipeline_yaml
run_in_dir "git-add-main" "$CLONE_DIR" git add README.md azure-pipelines.ado-helper-smoke.yml
run_in_dir "git-commit-main" "$CLONE_DIR" git commit -m "$RUN_ID initial content"
run_in_dir "git-push-main" "$CLONE_DIR" git push -u origin main
TAG_NAME="v-$RUN_ID"
run_in_dir "git-tag" "$CLONE_DIR" git tag "$TAG_NAME"
run_in_dir "git-push-tag" "$CLONE_DIR" git push origin "refs/tags/$TAG_NAME"

BRANCH_COMPLETE="$RUN_ID-complete"
BRANCH_ABANDON="$RUN_ID-abandon"
create_branch "$BRANCH_COMPLETE"
create_branch "$BRANCH_ABANDON"
run_in_dir "git-checkout-main-after-branches" "$CLONE_DIR" git checkout main

run_cmd "repo-branches-json" "$ADO_BIN" repo branches --repo "$REPO_NAME" --filter "$RUN_ID" --output json
assert_jq_arg "repo-branches-include-complete" "$(artifact_out repo-branches-json)" branch "$BRANCH_COMPLETE" '.value | any(.name == ("refs/heads/" + $branch))'
run_cmd "repo-tags-json" "$ADO_BIN" repo tags --repo "$REPO_NAME" --filter "$TAG_NAME" --output json
assert_jq_arg "repo-tags-include-tag" "$(artifact_out repo-tags-json)" tag "$TAG_NAME" '.value | any(.name == ("refs/tags/" + $tag))'
run_cmd "repo-commits-json" "$ADO_BIN" repo commits --repo "$REPO_NAME" --branch main --max 5 --output json
assert_jq "repo-commits-has-main-commit" "$(artifact_out repo-commits-json)" '.value | type == "array" and length >= 1'

if [[ "${ADO_E2E_SKIP_PIPELINE:-0}" == "1" ]]; then
  skip "pipeline-live-commands" "ADO_E2E_SKIP_PIPELINE=1"
else
  create_pipeline
  if [[ -n "$PIPELINE_ID" ]]; then
    run_cmd "pipeline-list-json" "$ADO_BIN" pipeline list --output json
    assert_jq_arg "pipeline-list-includes-created" "$(artifact_out pipeline-list-json)" name "$PIPELINE_NAME" '.value | any(.name == $name)'
    run_cmd "pipeline-preview-json" "$ADO_BIN" pipeline preview "$PIPELINE_ID" --branch main --param environment=e2e --output json
    assert_jq "pipeline-preview-has-yaml" "$(artifact_out pipeline-preview-json)" '(.finalYaml // "") | contains("Delay@1")'
    cat > "$TMP_DIR/pipeline-override.yml" <<'YAML'
trigger: none
pr: none
pool:
  server
steps:
  - task: Delay@1
    inputs:
      delayForMinutes: "1"
YAML
    run_cmd "pipeline-preview-yaml-file-json" "$ADO_BIN" pipeline preview "$PIPELINE_ID" --branch main --yaml-file "$TMP_DIR/pipeline-override.yml" --output json
    assert_jq "pipeline-preview-yaml-file-has-yaml" "$(artifact_out pipeline-preview-yaml-file-json)" '(.finalYaml // "") | contains("Delay@1")'
    run_cmd "pipeline-run-json" "$ADO_BIN" pipeline run "$PIPELINE_ID" --branch main --output json
    PIPELINE_RUN_ID="$(jq -r '.id // empty' "$(artifact_out pipeline-run-json)")"
    if [[ -n "$PIPELINE_RUN_ID" ]]; then
      ok "pipeline-run-id-captured ($PIPELINE_RUN_ID)"
      run_cmd "pipeline-status-initial-json" "$ADO_BIN" pipeline status "$PIPELINE_RUN_ID" --pipeline-id "$PIPELINE_ID" --output json
      wait_for_pipeline_completion
      run_cmd "pipeline-status-final-json" "$ADO_BIN" pipeline status "$PIPELINE_RUN_ID" --pipeline-id "$PIPELINE_ID" --output json
      assert_jq "pipeline-status-final-succeeded" "$(artifact_out pipeline-status-final-json)" '.state == "completed" and .result == "succeeded"'
      run_cmd "pipeline-runs-json" "$ADO_BIN" pipeline runs "$PIPELINE_ID" --branch main --state completed --result succeeded --max 5 --output json
      assert_jq_arg "pipeline-runs-includes-run" "$(artifact_out pipeline-runs-json)" run "$PIPELINE_RUN_ID" '.value | any((.id | tostring) == $run)'
      run_cmd "pipeline-logs-list-json" "$ADO_BIN" pipeline logs "$PIPELINE_RUN_ID" --pipeline-id "$PIPELINE_ID" --output json
      assert_jq "pipeline-logs-list-has-logs" "$(artifact_out pipeline-logs-list-json)" '.logs | type == "array" and length > 0'
      LOG_ID="$(jq -r '.logs[0].id // empty' "$(artifact_out pipeline-logs-list-json)")"
      if [[ -n "$LOG_ID" ]]; then
        ok "pipeline-log-id-captured ($LOG_ID)"
        run_cmd "pipeline-log-json" "$ADO_BIN" pipeline logs "$PIPELINE_RUN_ID" "$LOG_ID" --pipeline-id "$PIPELINE_ID" --output json
        assert_jq_arg "pipeline-log-json-id" "$(artifact_out pipeline-log-json)" log "$LOG_ID" '(.id | tostring) == $log'
        run_cmd "pipeline-log-text" "$ADO_BIN" pipeline logs "$PIPELINE_RUN_ID" "$LOG_ID" --pipeline-id "$PIPELINE_ID"
        run_cmd "pipeline-log-follow" "$ADO_BIN" pipeline logs "$PIPELINE_RUN_ID" "$LOG_ID" --pipeline-id "$PIPELINE_ID" --follow
      else
        fail "pipeline-log-id-captured (no log id in logs response)"
      fi
    else
      fail "pipeline-run-id-captured (pipeline run response had no id)"
    fi
  fi
fi

run_cmd "wi-create-1-json" "$ADO_BIN" wi create --type "$WI_TYPE" --title "$RUN_ID work item one" --description "<p>$RUN_ID body one</p>" --assigned-to me --tags "$RUN_ID" --output json
WI_ID_1="$(jq -r '.id // empty' "$(artifact_out wi-create-1-json)")"
[[ -n "$WI_ID_1" ]] && WORK_ITEM_IDS+=("$WI_ID_1")
assert_jq_arg "wi-create-1-title" "$(artifact_out wi-create-1-json)" title "$RUN_ID work item one" '.fields["System.Title"] == $title'

run_cmd "wi-create-2-json" "$ADO_BIN" wi create --type "$WI_TYPE" --title "$RUN_ID work item two" --description "<p>$RUN_ID body two</p>" --tags "$RUN_ID" --output json
WI_ID_2="$(jq -r '.id // empty' "$(artifact_out wi-create-2-json)")"
[[ -n "$WI_ID_2" ]] && WORK_ITEM_IDS+=("$WI_ID_2")
assert_jq_arg "wi-create-2-title" "$(artifact_out wi-create-2-json)" title "$RUN_ID work item two" '.fields["System.Title"] == $title'

run_cmd "wi-create-delete-json" "$ADO_BIN" wi create --type "$WI_TYPE" --title "$RUN_ID delete me" --description "<p>$RUN_ID delete</p>" --tags "$RUN_ID" --output json
WI_ID_DELETE="$(jq -r '.id // empty' "$(artifact_out wi-create-delete-json)")"
if [[ -n "$WI_ID_DELETE" ]]; then
  run_cmd "wi-delete" "$ADO_BIN" wi delete "$WI_ID_DELETE"
else
  fail "wi-delete (could not create disposable work item)"
fi

if [[ -n "$WI_ID_1" && -n "$WI_ID_2" ]]; then
  run_cmd "wi-list-search-json" "$ADO_BIN" wi list --search "$RUN_ID" --output json
  assert_jq "wi-list-search-finds-items" "$(artifact_out wi-list-search-json)" 'length >= 2'
  run_cmd "wi-list-assigned-me-json" "$ADO_BIN" wi list --assigned-to me --output json
  assert_jq "wi-list-assigned-me-is-array" "$(artifact_out wi-list-assigned-me-json)" 'type == "array"'
  run_cmd "wi-list-search-body-json" "$ADO_BIN" wi list --search-body "$RUN_ID" --output json
  assert_jq "wi-list-search-body-is-array" "$(artifact_out wi-list-search-body-json)" 'type == "array"'
  WIQL="SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.Title] CONTAINS '$RUN_ID' ORDER BY [System.ChangedDate] DESC"
  run_cmd "wi-query-inline-json" "$ADO_BIN" wi query --wiql "$WIQL" --output json
  assert_jq "wi-query-inline-finds-items" "$(artifact_out wi-query-inline-json)" 'length >= 2'
  printf '%s\n' "$WIQL" > "$TMP_DIR/query.wiql"
  run_cmd "wi-query-file-json" "$ADO_BIN" wi query --file "$TMP_DIR/query.wiql" --output json
  assert_jq "wi-query-file-finds-items" "$(artifact_out wi-query-file-json)" 'length >= 2'
  run_cmd "wi-view-json" "$ADO_BIN" wi view "$WI_ID_1" --output json
  assert_jq_arg "wi-view-id" "$(artifact_out wi-view-json)" id "$WI_ID_1" '(.id | tostring) == $id'
  run_cmd "wi-update-json" "$ADO_BIN" wi update "$WI_ID_1" --title "$RUN_ID work item one updated" --tags "$RUN_ID;updated" --field history="<p>$RUN_ID history</p>" --output json
  assert_jq_arg "wi-update-title" "$(artifact_out wi-update-json)" title "$RUN_ID work item one updated" '.[0].fields["System.Title"] == $title'
  printf '[%s,%s]\n' "$WI_ID_1" "$WI_ID_2" > "$TMP_DIR/wi-ids.json"
  run_with_stdin "wi-update-stdin-json" "$TMP_DIR/wi-ids.json" "$ADO_BIN" wi update --tags "$RUN_ID;stdin" --output json
  assert_jq "wi-update-stdin-updated-two" "$(artifact_out wi-update-stdin-json)" 'length == 2'
  run_cmd "wi-update-explain" "$ADO_BIN" --explain wi update "$WI_ID_1" --field priority=2
  if [[ -n "${ADO_TEAM:-}" ]]; then
    run_allow_codes "sprint-plan-into-explain" "0 2" "$ROOT" "$ADO_BIN" --explain sprint plan-into "$WI_ID_1" --iteration @current --assigned-to me
  fi
  run_cmd "wi-comment-json" "$ADO_BIN" wi comment "$WI_ID_1" --text "<p>$RUN_ID comment</p>" --output json
  WI_COMMENT_ID="$(jq -r '.id // empty' "$(artifact_out wi-comment-json)")"
  assert_jq_arg "wi-comment-id" "$(artifact_out wi-comment-json)" wi "$WI_ID_1" '(.workItemId | tostring) == $wi'
  run_cmd "wi-comments-json" "$ADO_BIN" wi comments "$WI_ID_1" --output json
  assert_jq_arg "wi-comments-contains-comment" "$(artifact_out wi-comments-json)" cid "$WI_COMMENT_ID" '.comments | any((.id | tostring) == $cid)'
  if [[ -n "$WI_COMMENT_ID" ]]; then
    run_cmd "wi-comment-edit-json" "$ADO_BIN" wi comment-edit "$WI_ID_1" "$WI_COMMENT_ID" --text "<p>$RUN_ID edited comment</p>" --output json
    assert_jq_arg "wi-comment-edit-id" "$(artifact_out wi-comment-edit-json)" cid "$WI_COMMENT_ID" '(.id | tostring) == $cid'
    run_cmd "wi-comment-delete" "$ADO_BIN" wi comment-delete "$WI_ID_1" "$WI_COMMENT_ID"
  else
    fail "wi-comment-edit/delete (comment id missing)"
  fi
  run_cmd "wi-link-json" "$ADO_BIN" wi link "$WI_ID_1" --related "$WI_ID_2" --comment "$RUN_ID relation" --output json
  assert_jq "wi-link-added-relation" "$(artifact_out wi-link-json)" '.relations | length >= 1'
  run_cmd "wi-links-json" "$ADO_BIN" wi links "$WI_ID_1" --output json
  assert_jq "wi-links-has-relation" "$(artifact_out wi-links-json)" 'length >= 1'
  run_cmd "wi-link-rm-json" "$ADO_BIN" wi link-rm "$WI_ID_1" --index 0 --output json
  run_cmd "wi-links-after-rm-json" "$ADO_BIN" wi links "$WI_ID_1" --output json
  printf '%s\n' "$RUN_ID attachment" > "$TMP_DIR/attachment.txt"
  run_cmd "wi-attach-json" "$ADO_BIN" wi attach "$WI_ID_1" "$TMP_DIR/attachment.txt" --comment "$RUN_ID attachment" --output json
  assert_jq "wi-attach-has-attachment" "$(artifact_out wi-attach-json)" '(.attachment.id // "") != ""'
  run_cmd "wi-history-json" "$ADO_BIN" wi history "$WI_ID_1" --limit 10 --output json
  assert_jq "wi-history-has-revisions" "$(artifact_out wi-history-json)" '.value | type == "array" and length > 0'
  run_cmd "wi-open-fake-browser" "$ADO_BIN" --quiet wi open "$WI_ID_1"
else
  fail "work-item-flow (missing created work item ids)"
fi

if [[ -n "${WI_ID_1:-}" && -n "${WI_ID_2:-}" ]]; then
  run_cmd "pr-create-complete-json" "$ADO_BIN" pr create --repo "$REPO_NAME" --source "$BRANCH_COMPLETE" --target main --title "$RUN_ID complete PR" --description "$RUN_ID PR body" --output json
  PR_ID_1="$(jq -r '.pullRequestId // empty' "$(artifact_out pr-create-complete-json)")"
  [[ -n "$PR_ID_1" ]] && PR_IDS+=("$PR_ID_1")
  assert_jq_arg "pr-create-complete-active" "$(artifact_out pr-create-complete-json)" title "$RUN_ID complete PR" '.title == $title and .status == "active"'
  run_cmd "pr-create-abandon-json" "$ADO_BIN" pr create --repo "$REPO_NAME" --source "$BRANCH_ABANDON" --target main --title "$RUN_ID abandon PR" --description "$RUN_ID abandon body" --output json
  PR_ID_2="$(jq -r '.pullRequestId // empty' "$(artifact_out pr-create-abandon-json)")"
  [[ -n "$PR_ID_2" ]] && PR_IDS+=("$PR_ID_2")
  assert_jq_arg "pr-create-abandon-active" "$(artifact_out pr-create-abandon-json)" title "$RUN_ID abandon PR" '.title == $title and .status == "active"'

  if [[ -n "${PR_ID_1:-}" ]]; then
    run_cmd "pr-list-active-json" "$ADO_BIN" pr list --repo "$REPO_NAME" --status active --output json
    assert_jq_arg "pr-list-active-includes-pr" "$(artifact_out pr-list-active-json)" pr "$PR_ID_1" '.value | any((.pullRequestId | tostring) == $pr)'
    run_in_dir "pr-list-project-all-json" "$WORK_DIR" env ADO_REPO= "$ADO_BIN" pr list --status all --output json
    assert_jq_arg "pr-list-project-all-includes-pr" "$(artifact_out pr-list-project-all-json)" pr "$PR_ID_1" '.value | any((.pullRequestId | tostring) == $pr)'
    run_cmd "pr-view-json" "$ADO_BIN" pr view "$PR_ID_1" --repo "$REPO_NAME" --output json
    assert_jq_arg "pr-view-id" "$(artifact_out pr-view-json)" pr "$PR_ID_1" '(.pullRequestId | tostring) == $pr'
    run_cmd "pr-update-json" "$ADO_BIN" pr update "$PR_ID_1" --repo "$REPO_NAME" --title "$RUN_ID complete PR updated" --description "$RUN_ID updated body" --field draft=false --output json
    assert_jq_arg "pr-update-title" "$(artifact_out pr-update-json)" title "$RUN_ID complete PR updated" '.title == $title'
    run_cmd "pr-update-explain" "$ADO_BIN" --explain pr update "$PR_ID_1" --repo "$REPO_NAME" --title "$RUN_ID dry run"
    run_cmd "pr-approve-json" "$ADO_BIN" pr approve "$PR_ID_1" --repo "$REPO_NAME" --vote 10 --output json
    assert_jq "pr-approve-vote" "$(artifact_out pr-approve-json)" '.vote == 10'
    run_cmd "pr-comment-json" "$ADO_BIN" pr comment "$PR_ID_1" --repo "$REPO_NAME" --text "$RUN_ID PR comment" --output json
    PR_THREAD_ID="$(jq -r '.id // empty' "$(artifact_out pr-comment-json)")"
    assert_jq_arg "pr-comment-thread-active" "$(artifact_out pr-comment-json)" content "$RUN_ID PR comment" '.comments | any(.content == $content)'
    run_cmd "pr-threads-json" "$ADO_BIN" pr threads "$PR_ID_1" --repo "$REPO_NAME" --output json
    assert_jq_arg "pr-threads-contains-comment-thread" "$(artifact_out pr-threads-json)" thread "$PR_THREAD_ID" '.value | any((.id | tostring) == $thread)'
    if [[ -n "$PR_THREAD_ID" ]]; then
      run_cmd "pr-thread-reply-json" "$ADO_BIN" pr thread-reply "$PR_ID_1" "$PR_THREAD_ID" --repo "$REPO_NAME" --text "$RUN_ID PR reply" --output json
      assert_jq_arg "pr-thread-reply-content" "$(artifact_out pr-thread-reply-json)" content "$RUN_ID PR reply" '.content == $content'
      run_cmd "pr-thread-resolve-json" "$ADO_BIN" pr thread-resolve "$PR_ID_1" "$PR_THREAD_ID" --repo "$REPO_NAME" --output json
      assert_jq "pr-thread-resolve-closed" "$(artifact_out pr-thread-resolve-json)" '.status == "closed" or .status == 4'
    else
      fail "pr-thread-reply/resolve (thread id missing)"
    fi
    run_cmd "pr-checks-json" "$ADO_BIN" pr checks "$PR_ID_1" --repo "$REPO_NAME" --output json
    assert_jq "pr-checks-has-array" "$(artifact_out pr-checks-json)" '.value | type == "array"'
    run_cmd "pr-link-work-item-json" "$ADO_BIN" pr link-work-item "$PR_ID_1" --repo "$REPO_NAME" --work-item "$WI_ID_1" --output json
    assert_jq_arg "pr-link-work-item-linked-one" "$(artifact_out pr-link-work-item-json)" wi "$WI_ID_1" '. | any((.id | tostring) == $wi)'
    printf '[%s]\n' "$WI_ID_2" > "$TMP_DIR/pr-link-wi-ids.json"
    run_with_stdin "pr-link-work-item-stdin-json" "$TMP_DIR/pr-link-wi-ids.json" "$ADO_BIN" pr link-work-item "$PR_ID_1" --repo "$REPO_NAME" --output json
    assert_jq_arg "pr-link-work-item-linked-stdin" "$(artifact_out pr-link-work-item-stdin-json)" wi "$WI_ID_2" '. | any((.id | tostring) == $wi)'
    run_in_dir "git-checkout-main-before-pr-checkout" "$CLONE_DIR" git checkout main
    run_in_dir "pr-checkout" "$CLONE_DIR" env ADO_REPO="$REPO_NAME" "$ADO_BIN" pr checkout "$PR_ID_1" --repo "$REPO_NAME" --branch "review/$BRANCH_COMPLETE"
    REVIEW_ROOT="$WORK_DIR/reviews"
    REVIEW_DIR="$REVIEW_ROOT/$REPO_NAME-pr-$PR_ID_1"
    run_in_dir "pr-checkout-detach-review-clone" "$WORK_DIR" env ADO_REPO= "$ADO_BIN" pr checkout "$PR_ID_1" --repo "$REPO_NAME" --detach --dir "$REVIEW_DIR"
    run_in_dir "pr-checkout-detach-review-clone-head" "$REVIEW_DIR" git rev-parse --verify HEAD
    run_cmd "pr-checkout-clean-dry-run" "$ADO_BIN" pr checkout-clean "$PR_ID_1" --dir "$REVIEW_ROOT" --dry-run
    run_cmd "pr-checkout-clean" "$ADO_BIN" pr checkout-clean "$PR_ID_1" --dir "$REVIEW_ROOT"
    if [[ -d "$REVIEW_DIR" ]]; then
      fail "pr-checkout-clean-removed-review-dir ($REVIEW_DIR still exists)"
    else
      ok "pr-checkout-clean-removed-review-dir"
    fi
    run_in_dir "git-checkout-main-before-pr-complete" "$CLONE_DIR" git checkout main
    run_cmd "pr-open-fake-browser" "$ADO_BIN" --quiet pr open "$PR_ID_1" --repo "$REPO_NAME"
    run_cmd "pr-complete-json" "$ADO_BIN" pr complete "$PR_ID_1" --repo "$REPO_NAME" --merge-strategy squash --delete-source-branch --output json
    assert_jq "pr-complete-status" "$(artifact_out pr-complete-json)" '.status == "completed"'
  fi

  if [[ -n "${PR_ID_2:-}" ]]; then
    run_cmd "pr-abandon-json" "$ADO_BIN" pr abandon "$PR_ID_2" --repo "$REPO_NAME" --output json
    assert_jq "pr-abandon-status" "$(artifact_out pr-abandon-json)" '.status == "abandoned"'
    run_cmd "pr-reactivate-json" "$ADO_BIN" pr reactivate "$PR_ID_2" --repo "$REPO_NAME" --output json
    assert_jq "pr-reactivate-status" "$(artifact_out pr-reactivate-json)" '.status == "active"'
    run_cmd "pr-abandon-final-json" "$ADO_BIN" pr abandon "$PR_ID_2" --repo "$REPO_NAME" --output json
    assert_jq "pr-abandon-final-status" "$(artifact_out pr-abandon-final-json)" '.status == "abandoned"'
    run_cmd "pr-list-abandoned-json" "$ADO_BIN" pr list --repo "$REPO_NAME" --status abandoned --output json
    assert_jq_arg "pr-list-abandoned-includes-pr" "$(artifact_out pr-list-abandoned-json)" pr "$PR_ID_2" '.value | any((.pullRequestId | tostring) == $pr)'
  fi
else
  fail "pull-request-flow (missing work item ids)"
fi

run_expect_code "wi-view-not-found-exit-2" 2 "$ADO_BIN" wi view 999999999
run_cmd "repo-delete-explain" "$ADO_BIN" --explain repo delete "$REPO_NAME" --yes
run_cmd "repo-delete-final" "$ADO_BIN" repo delete "$REPO_NAME" --yes
REPO_NAME=""

cleanup_resources

note ""
note "## Summary"
note ""
note "- Passed: $PASS"
note "- Failed: $FAIL"
note "- Skipped: $SKIP"
if [[ "$FAIL" -gt 0 ]]; then
  note ""
  note "### Failures"
  for item in "${FAILURES[@]}"; do
    note "- $item"
  done
fi
if [[ "$SKIP" -gt 0 ]]; then
  note ""
  note "### Skips"
  for item in "${SKIPS[@]}"; do
    note "- $item"
  done
fi

if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
