//! Typed errors that map to documented exit codes so agents can branch on
//! failure class without parsing stderr. `main` downcasts an `anyhow::Error`
//! to `CliError` to pick the right exit code.

use std::fmt;

#[derive(Debug)]
pub enum CliError {
    /// `2` — the requested resource does not exist (HTTP 404)
    NotFound(String),
    /// `3` — caller supplied invalid arguments (missing field, conflicting flags).
    /// Used by stdin batching (Phase 4) and future arg-validation paths; the
    /// variant exists today so the exit-code contract is complete.
    #[allow(dead_code)]
    Validation(String),
    /// `4` — authentication or authorization failure (HTTP 401/403)
    Auth(String),
    /// `5` — the ADO REST API returned an error
    Api(String),
    /// Sentinel raised by mutating client helpers when `--explain` is set. Caught
    /// in `main` and mapped to exit code 0 so a dry-run is unambiguously a success.
    Explain,
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Explain => 0,
            Self::NotFound(_) => 2,
            Self::Validation(_) => 3,
            Self::Auth(_) => 4,
            Self::Api(_) => 5,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(m) | Self::Validation(m) | Self::Auth(m) | Self::Api(m) => {
                f.write_str(m)
            }
            Self::Explain => f.write_str("dry-run — no request sent"),
        }
    }
}

impl std::error::Error for CliError {}
