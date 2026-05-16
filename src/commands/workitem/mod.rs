//! Work item commands. Split into:
//! - `args`     — clap derives for every subcommand
//! - `types`    — ADO API response shapes + JSON Patch op
//! - `flags`    — FieldFlags struct + alias map + build_field_ops
//! - `helpers`  — small wi-specific utilities (`me` resolution, encoding)
//! - `handlers` — actual command bodies

mod args;
mod flags;
mod handlers;
mod helpers;
mod types;

pub use args::WorkItemArgs;

use anyhow::Result;

use crate::client::AdoClient;
use crate::output::OutputFormat;

pub async fn run(args: WorkItemArgs, client: &AdoClient, output: &OutputFormat) -> Result<()> {
    handlers::dispatch(args, client, output).await
}
