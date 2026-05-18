//! Stdin batching for mutation commands.
//!
//! `read_ids` is the canonical entrypoint: handlers pass their (possibly
//! empty) CLI-supplied ids; if none are present and stdin is piped, ids
//! are read from there. Used by `wi update`, `pr link-work-item`, and
//! future sprint commands so an agent can pipe `ado wi query --output json`
//! into a mutation without an awk dance.

use anyhow::Result;
use std::io::{self, IsTerminal, Read};

use crate::error::CliError;

/// Resolve a list of work item / PR ids from CLI args or stdin.
///
/// - If `cli_args` is non-empty, returns it unchanged.
/// - Else, if stdin is a TTY (no pipe), returns a `Validation` error.
/// - Else, reads stdin and parses either a JSON array (`[1, 2, 3]`) or
///   one id per line (blanks ignored, leading `#` allowed for paste-friendly
///   `#123` output).
pub fn read_ids(cli_args: &[u32]) -> Result<Vec<u32>> {
    if !cli_args.is_empty() {
        return Ok(cli_args.to_vec());
    }
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Err(CliError::Validation(
            "no ids provided — pass them as args or pipe them on stdin (one per line or a JSON array)"
                .into(),
        )
        .into());
    }
    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Validation(format!("reading stdin: {e}")))?;
    parse_ids(&buf)
}

/// Parse a buffer into a list of ids. Exposed for testing.
pub fn parse_ids(input: &str) -> Result<Vec<u32>> {
    let trimmed = input.trim_start();
    if trimmed.starts_with('[') {
        let parsed: Vec<u32> = serde_json::from_str(trimmed)
            .map_err(|e| CliError::Validation(format!("invalid JSON id array: {e}")))?;
        if parsed.is_empty() {
            return Err(CliError::Validation("stdin had no ids".into()).into());
        }
        return Ok(parsed);
    }

    let mut out: Vec<u32> = Vec::new();
    for (lineno, line) in input.lines().enumerate() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        // Tolerate the `#123` and `#123 …trailing comment` forms that fall
        // out of `ado wi list`'s text output.
        let token = s.split_whitespace().next().unwrap();
        let token = token.trim_start_matches('#');
        let id: u32 = token.parse().map_err(|e| {
            CliError::Validation(format!("line {}: invalid id `{token}`: {e}", lineno + 1))
        })?;
        out.push(id);
    }
    if out.is_empty() {
        return Err(CliError::Validation("stdin had no ids".into()).into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ids_accepts_newline_separated() {
        assert_eq!(parse_ids("1\n2\n3\n").unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn parse_ids_tolerates_blank_lines_and_hash_prefix() {
        let input = "\n#10\n\n  #11  \n#12 [Task] [Doing] something\n";
        assert_eq!(parse_ids(input).unwrap(), vec![10, 11, 12]);
    }

    #[test]
    fn parse_ids_accepts_json_array() {
        assert_eq!(parse_ids("[10, 20, 30]").unwrap(), vec![10, 20, 30]);
        assert_eq!(parse_ids("   [1]\n").unwrap(), vec![1]);
    }

    #[test]
    fn parse_ids_rejects_empty_input() {
        assert!(parse_ids("").is_err());
        assert!(parse_ids("   \n\n").is_err());
        assert!(parse_ids("[]").is_err());
    }

    #[test]
    fn parse_ids_rejects_garbage() {
        assert!(parse_ids("not-an-id\n").is_err());
        assert!(parse_ids("[1, \"two\"]").is_err());
    }
}
