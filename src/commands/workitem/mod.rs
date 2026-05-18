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
pub mod types;

pub use args::WorkItemArgs;
pub use types::WorkItem;

use anyhow::Result;

use crate::context::CmdCtx;

pub async fn run(args: WorkItemArgs, ctx: &CmdCtx<'_>) -> Result<()> {
    handlers::dispatch(args, ctx).await
}
