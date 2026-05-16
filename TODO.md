# TODO

Backlog of next steps for the `ado` CLI. Roughly ordered by user-visible impact.

## 1. PR threads & comments

Round out the PR review flow. Right now `pr approve` works but there's no way to leave inline or top-level feedback from the terminal — every code-review interaction except the vote requires bouncing to the web UI.

**Scope**
- `pr comment <id> --text "..."` — post a top-level (general) comment.
- `pr threads <id>` — list threads with their status (`active`, `closed`, `pending`).
- `pr thread-reply <id> <thread-id> --text "..."` — reply to an existing thread.
- `pr thread-resolve <id> <thread-id>` — close a thread.
- Inline comments on a specific file/line are a stretch goal — the API requires a `threadContext` with `filePath` + `rightFileStart/End` line numbers; usable but more arg-modeling.

**Endpoints**
- `GET/POST /{project}/_apis/git/repositories/{repo}/pullRequests/{id}/threads?api-version=7.1`
- `PATCH .../threads/{thread-id}` for status changes
- `POST .../threads/{thread-id}/comments` for replies

**Notes**
- ADO models discussions as a `Thread` containing `comments[]`. A "comment on a PR" is really "create a thread with one comment."
- Reuse the `repo` resolution chain (`--repo` → `ADO_REPO` → `git remote`) already in `pr.rs`.

---

## 2. `pr files` / diff inspection

Make code review without leaving the terminal viable. The current `pr view` shows metadata; `pr files` would show *what changed*.

**Scope**
- `pr files <id>` — list changed files with status (add/edit/delete) and per-file line counts.
- `pr diff <id> [--file path]` — print the diff hunks for one file, or all files. Could shell out to `git diff <merge-base>..<head>` when both refs are local, falling back to the ADO API when not.

**Endpoints**
- `GET .../pullRequests/{id}/iterations` → most recent iteration ID
- `GET .../pullRequests/{id}/iterations/{iter-id}/changes?api-version=7.1` → file list with change types
- `GET .../diffs/commits?baseVersion=...&targetVersion=...` for the actual diff bodies

**Notes**
- Showing colorized diffs inline is a UX bonus but adds complexity. Could pipe through `delta`/`bat` if installed and fall back to plain.
- "Run from anywhere" matters here — most users will be sitting on the source branch when they want to review *another* PR.

---

## 3. Quality pass

The codebase has 5 unit tests today, all in `fields.rs` and `workitem/flags.rs`. The `pr`, `pipeline`, and `repo` modules have zero coverage. Pure helpers are easy wins; integration coverage is harder.

**Scope**
- **Unit tests** for pure functions across modules:
  - `pr::resolve_pr_field`, `pr::strip_refs_heads`, `pr::repo_from_remote` (parsing-only, no IO)
  - `repo::inject_pat` (already covered, but could add more edge cases)
  - `pipeline::resolve_pipeline` name-vs-ID branching (mock the list call)
- **Integration tests** with recorded HTTP fixtures:
  - Pick a tool: `wiremock`, `mockito`, or roll a tiny `axum` mock server.
  - Snapshot real ADO responses (sanitized of PATs/IDs) and replay them in tests.
  - Lets us verify the end-to-end flow (CLI → request → parse → format) without hitting prod.
- **Lint pass**: enable `#![deny(warnings)]` in `main.rs` once we're comfortable, plus `clippy` in CI.

**Risks**
- Integration tests are a real time sink. Worth deferring until we hit a regression that justifies them.

---

## 4. Packaging / install story

Today the only way to use `ado` is `cargo build`. That's fine for the author but blocks anyone else from picking it up.

**Scope**
- **GitHub Releases**: GitHub Actions workflow that cross-compiles for `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc` on tag push and uploads binaries. Use `taiki-e/upload-rust-binary-action` or similar.
- **Homebrew tap**: `homebrew-tap` repo with a formula that downloads the macOS binary from Releases. `brew install jacobragsdale/tap/ado`.
- **`cargo install ado-cli`**: publish to crates.io. Renames the package (the `ado` name on crates.io is taken). Cheapest distribution channel for Rust users.
- **README**: install instructions, quickstart, `.env` template walkthrough, the precedence table for `--flag` → env → TOML config.

**Notes**
- Cross-compiling `rustls-tls` is the easy path (no system OpenSSL). Already configured.
- Versioning: bump to `0.2.0` for the first public release once `pr` threads or another headline feature lands.
