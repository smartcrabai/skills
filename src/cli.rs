use clap::{Args, Parser, Subcommand};

use crate::commands;
use crate::error::Result;

/// `skills` — install and manage agent skills (Rust port of `vercel-labs/skills`).
#[derive(Debug, Parser)]
#[command(name = "skills", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install a skill from a GitHub source.
    Add(AddArgs),
    /// Generate a SKILL.md from a description using an AI creator agent, then install.
    Create(CreateArgs),
    /// List installed skills.
    List(ListArgs),
    /// Search for skills (remote API + local registry fallback).
    Find(FindArgs),
    /// Remove installed skill(s).
    Remove(RemoveArgs),
    /// Update installed skill(s) from upstream.
    Update(UpdateArgs),
    /// Scaffold a new skill (`SKILL.md` template).
    Init(InitArgs),
    /// Read or modify configuration.
    Config(ConfigArgs),
    /// Restore project-scoped skills from `./skills-lock.json`.
    #[command(alias = "i")]
    Install(InstallArgs),
}

/// Arguments for `add`.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "clap CLI flags are naturally bool-heavy"
)]
pub struct AddArgs {
    /// `owner/repo[/sub_path][#ref]`, a full GitHub/GitLab/git URL,
    /// `git@host:owner/repo[.git]`, or a local path (`./dir`, `/abs/dir`).
    pub source: String,
    /// Install for the current user (XDG global).
    #[arg(short = 'g', long = "global", conflicts_with = "project")]
    pub global: bool,
    /// Install into the current project.
    #[arg(short = 'p', long = "project", conflicts_with = "global")]
    pub project: bool,
    /// Symlink into agent dirs instead of deep copies (the default).
    #[arg(long = "symlink")]
    pub symlink: bool,
    /// Specific agents to wire up (repeatable).
    #[arg(short = 'a', long = "agent")]
    pub agents: Vec<String>,
    /// Specific skill name(s) to install when the source contains multiple
    /// `SKILL.md` files (repeatable).
    #[arg(short = 's', long = "skill")]
    pub skills: Vec<String>,
    /// Install every skill discovered in the source.
    #[arg(long = "all", conflicts_with = "skills")]
    pub all: bool,
    /// Skip interactive prompts; assume yes.
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(short = 'g', long = "global", conflicts_with = "project")]
    pub global: bool,
    #[arg(short = 'p', long = "project", conflicts_with = "global")]
    pub project: bool,
    #[arg(long = "json")]
    pub json: bool,
}

/// Arguments for `find`.
#[derive(Debug, Args)]
pub struct FindArgs {
    pub query: Option<String>,
    #[arg(long = "json")]
    pub json: bool,
}

/// Arguments for `remove`.
#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub skills: Vec<String>,
    #[arg(short = 'g', long = "global", conflicts_with = "project")]
    pub global: bool,
    #[arg(short = 'p', long = "project", conflicts_with = "global")]
    pub project: bool,
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `update`.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub skills: Vec<String>,
    #[arg(short = 'g', long = "global", conflicts_with = "project")]
    pub global: bool,
    #[arg(short = 'p', long = "project", conflicts_with = "global")]
    pub project: bool,
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Skill name (defaults to `basename(cwd)`).
    pub name: Option<String>,
}

/// Arguments for `create`.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "clap CLI flags are naturally bool-heavy"
)]
pub struct CreateArgs {
    /// Natural-language description of what the skill should do.
    pub description: String,
    /// Creator agent to use (e.g. "claude-code" runs `claude -p <prompt>`).
    #[arg(short = 'c', long = "creator")]
    pub creator: Option<String>,
    /// Explicit skill name (derived from description when omitted).
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,
    /// Install for the current user (XDG global).
    #[arg(short = 'g', long = "global", conflicts_with = "project")]
    pub global: bool,
    /// Install into the current project.
    #[arg(short = 'p', long = "project", conflicts_with = "global")]
    pub project: bool,
    /// Symlink into agent dirs instead of deep copies (the default).
    #[arg(long = "symlink")]
    pub symlink: bool,
    /// Specific agents to wire up (repeatable).
    #[arg(short = 'a', long = "agent")]
    pub agents: Vec<String>,
    /// Skip interactive prompts; assume yes.
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `install`.
#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Symlink into agent dirs instead of deep copies (the default).
    #[arg(long = "symlink")]
    pub symlink: bool,
    /// Skip interactive prompts; assume yes (currently always implied).
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `config`.
#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// `key`, or one of: `show` to dump full config.
    pub key: String,
    /// One of: `list`, `add`, `remove`, `set`. Optional for `show`.
    pub op: Option<String>,
    /// Value(s) for the operation.
    pub values: Vec<String>,
}

/// Parse argv and dispatch to a command handler.
///
/// Eagerly touches the config so first-run users get `config.json` even
/// for commands that fail with `NotImplemented`.
///
/// # Errors
///
/// Propagates whatever each command returns.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let _ = crate::config::Config::load();
    match cli.command {
        Command::Add(a) => commands::add::run(a).await,
        Command::Create(a) => commands::create::run(a).await,
        Command::List(a) => commands::list::run(a).await,
        Command::Find(a) => commands::find::run(a).await,
        Command::Remove(a) => commands::remove::run(a).await,
        Command::Update(a) => commands::update::run(a).await,
        Command::Init(a) => commands::init::run(a).await,
        Command::Config(a) => commands::config::run(a).await,
        Command::Install(a) => commands::install::run(a).await,
    }
}
