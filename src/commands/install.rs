//! `skills install`: restore project-scoped skills from `./skills-lock.json`.
//!
//! First-class equivalent of `experimental_install` in `vercel-labs/skills`.
//! Co-located skills sharing the same `(source, ref)` are bundled into a
//! single `add` call so we only clone once per upstream.

use crate::cli::{AddArgs, InstallArgs};
use crate::commands;
use crate::error::{Error, Result};
use crate::lock::{self, Lockfile};

/// # Errors
///
/// Returns [`Error::PartialInstallFailure`] when at least one source failed,
/// or surfaces I/O / parse errors loading the lockfile.
pub async fn run(args: InstallArgs) -> Result<()> {
    let project_root = std::env::current_dir()?;
    let lock_path = Lockfile::path(&project_root);
    let Some(lock) = Lockfile::load(&lock_path)? else {
        eprintln!(
            "skills-lock.json not found in {}; nothing to install",
            project_root.display()
        );
        return Ok(());
    };
    if lock.skills.is_empty() {
        eprintln!("skills-lock.json has no entries; nothing to install");
        return Ok(());
    }

    let mut failed_sources: Vec<String> = Vec::new();
    for grp in lock::group_by_source(&lock) {
        let source_str = grp
            .ref_
            .as_ref()
            .map_or_else(|| grp.source.clone(), |r| format!("{}#{r}", grp.source));
        let add_args = AddArgs {
            source: source_str.clone(),
            global: false,
            project: true,
            symlink: args.symlink,
            agents: grp.agents,
            skills: grp.skill_names,
            all: false,
            yes: true,
        };
        if let Err(e) = commands::add::run(add_args).await {
            // Pre-flight in `add` refuses already-installed skills; treat as
            // skip so `install` stays idempotent.
            if let Error::DuplicateSkill(msg) = &e {
                eprintln!("install: skip (already installed): {msg}");
            } else {
                eprintln!("install: failed for {source_str}: {e}");
                failed_sources.push(source_str);
            }
        }
    }
    if !failed_sources.is_empty() {
        return Err(Error::PartialInstallFailure(failed_sources));
    }
    Ok(())
}
