//! Per-invocation command context. One value is built in `main` from global
//! flags and threaded through every handler so we don't have to grow the
//! handler signature each time a new global is added.

use crate::client::AdoClient;
use crate::output::OutputFormat;

pub struct CmdCtx<'a> {
    pub client: &'a AdoClient,
    pub output: OutputFormat,
    /// Suppress decorative/progress output. Result lines and errors still print.
    pub quiet: bool,
    /// Resolved default team for commands that scope to a team (iteration,
    /// capacity, board, sprint). Precedence: `--team` flag → ADO_TEAM env →
    /// `config.team`. `None` means no team was resolved; team-scoped commands
    /// should fail with a clear "no team set" error.
    pub team: Option<String>,
}

// Note: `--explain` lives on `AdoClient` instead — it's a property of the
// HTTP layer, not the dispatch context. Handlers that need it (e.g. raw-body
// mutations like `wi attach`) call `client.explain_enabled()` directly.
