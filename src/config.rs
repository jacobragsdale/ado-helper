use anyhow::Result;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level args for `ado config`
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Set configuration values
    Set(SetArgs),

    /// Print current configuration
    Show,
}

#[derive(Args)]
pub struct SetArgs {
    /// Azure DevOps organization URL (e.g. https://dev.azure.com/myorg)
    #[arg(long)]
    pub org: Option<String>,

    /// Default project name
    #[arg(long)]
    pub project: Option<String>,
}

/// Persisted configuration stored in %APPDATA%\ado\config.toml on Windows
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Full organization URL, e.g. "https://dev.azure.com/myorg"
    pub org: Option<String>,

    /// Default project name used when --project is not provided
    pub project: Option<String>,
}

impl Config {
    /*
     * IMPLEMENTATION NOTES — Config::path()
     *
     * Use dirs::config_dir() to get the OS config directory:
     *   - Windows: %APPDATA%  (e.g. C:\Users\jacob\AppData\Roaming)
     *   - macOS:   ~/.config
     *   - Linux:   ~/.config
     *
     * Append "ado/config.toml" to that path and return it.
     * Return an error if dirs::config_dir() returns None (very rare, means HOME is unset).
     */
    pub fn path() -> Result<PathBuf> {
        todo!("return dirs::config_dir()?.join(\"ado\").join(\"config.toml\")")
    }

    /*
     * IMPLEMENTATION NOTES — Config::load()
     *
     * 1. Get the config file path from Config::path().
     * 2. If the file does not exist, return Config::default() (empty config is valid).
     * 3. Read the file to a String with std::fs::read_to_string().
     * 4. Parse with toml::from_str::<Config>(&content) and return it.
     */
    pub fn load() -> Result<Self> {
        todo!("read config.toml from disk, return default if file doesn't exist")
    }

    /*
     * IMPLEMENTATION NOTES — Config::save()
     *
     * 1. Get the config file path from Config::path().
     * 2. Create the parent directory with std::fs::create_dir_all(path.parent().unwrap()).
     *    This handles the case where %APPDATA%\ado\ doesn't exist yet.
     * 3. Serialize self with toml::to_string(self)?.
     * 4. Write to disk with std::fs::write(path, content)?.
     */
    pub fn save(&self) -> Result<()> {
        todo!("serialize self to TOML and write to config path")
    }
}

/*
 * IMPLEMENTATION NOTES — run()
 *
 * Match on args.command:
 *
 * ConfigCommand::Set(set_args):
 *   1. Load current config with Config::load().
 *   2. If set_args.org is Some, update config.org.
 *   3. If set_args.project is Some, update config.project.
 *   4. Save config with config.save().
 *   5. Print "Configuration saved." to stdout.
 *
 * ConfigCommand::Show:
 *   1. Load config with Config::load().
 *   2. Print each field on its own line:
 *        org:     {value or "(not set)"}
 *        project: {value or "(not set)"}
 *   3. Also note that the PAT token is read from the PAT environment variable
 *      and never stored — indicate whether PAT is currently set:
 *        pat:     (set via PAT environment variable) or (not set — export PAT=...)
 */
pub async fn run(args: ConfigArgs) -> anyhow::Result<()> {
    todo!("implement config set and config show")
}
