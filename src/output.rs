use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum OutputFormat {
    /// Plain text, one item per line (default)
    Text,
    /// JSON — full API response, useful for scripting
    Json,
    /// Column-aligned table (only meaningful for list commands; falls back to text elsewhere)
    Table,
}

pub fn print_text(lines: &[String]) {
    for line in lines {
        println!("{line}");
    }
}

/// Print a decorative/progress line that should be suppressed under `--quiet`.
/// Result lines (e.g. "Created #123") should call `println!` directly — this is
/// only for banners like "(watching — Ctrl-C to stop)" or first-run hints.
pub fn banner(quiet: bool, msg: &str) {
    if !quiet {
        println!("{msg}");
    }
}

pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Render `rows` as a column-aligned table with `headers`. Each row must have
/// the same number of cells as `headers`. Empty input prints nothing.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            if cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }

    print_row(
        headers
            .iter()
            .map(|h| h.to_string())
            .collect::<Vec<_>>()
            .as_slice(),
        &widths,
    );
    let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    print_row(&sep, &widths);
    for row in rows {
        print_row(row, &widths);
    }
}

fn print_row(cells: &[String], widths: &[usize]) {
    let last = cells.len().saturating_sub(1);
    for (i, cell) in cells.iter().enumerate() {
        if i == last {
            // Don't pad the final column — avoids trailing whitespace.
            print!("{cell}");
        } else {
            print!("{:<width$}  ", cell, width = widths[i]);
        }
    }
    println!();
}
