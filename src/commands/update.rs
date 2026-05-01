use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::cli::UpdateArgs;
use crate::config::{Config, expand_path};
use crate::error::{Error, Result};
use crate::github::{SkillSource, fetch, parse_source};
use crate::install::{install_to_master, link_into_agents};
use crate::registry::{InstalledSkill, Registry, Scope};
use crate::ui;

/// Implements the `update` flow: re-fetch each target, replace its master copy
/// when the upstream commit changed, re-link agent dirs (preserving the stored
/// `Method`), and for global-scope skills also link any newly-defaulted agents.
///
/// # Errors
///
/// Surfaces parser, network, FS, and registry errors.
pub async fn run(args: UpdateArgs) -> Result<()> {
    let scope = resolve_scope(&args)?;
    let project_root = match scope {
        Scope::Project => Some(std::env::current_dir()?),
        Scope::Global => None,
    };
    let project_root_ref = project_root.as_deref();

    let cfg = Config::load()?;
    let mut registry = Registry::load()?;

    let targets = resolve_targets(&args, &registry, scope, project_root_ref)?;
    if targets.is_empty() {
        println!("No skills to update.");
        return Ok(());
    }

    if !args.yes && ui::is_tty() && !ui::confirm(&format!("Update {} skill(s)?", targets.len()))? {
        println!("Aborted.");
        return Ok(());
    }

    for name in targets {
        update_one(&cfg, &mut registry, &name, scope, project_root_ref).await?;
    }

    registry.save()?;
    Ok(())
}

fn resolve_scope(args: &UpdateArgs) -> Result<Scope> {
    match (args.global, args.project) {
        (true, true) => Err(Error::InvalidScope(
            "--global and --project are mutually exclusive".to_string(),
        )),
        (true, false) => Ok(Scope::Global),
        (false, true) => Ok(Scope::Project),
        (false, false) if ui::is_tty() && !args.yes => ui::select_scope(Some(Scope::Global)),
        (false, false) => Err(Error::InvalidScope(
            "specify --global or --project (no TTY for interactive prompt)".to_string(),
        )),
    }
}

fn resolve_targets(
    args: &UpdateArgs,
    registry: &Registry,
    scope: Scope,
    project_root: Option<&Path>,
) -> Result<Vec<String>> {
    if args.skills.is_empty() {
        return Ok(registry
            .iter_scope(scope, project_root)
            .map(|s| s.name.clone())
            .collect());
    }
    for name in &args.skills {
        if registry.find(name, scope, project_root).is_none() {
            return Err(Error::SkillNotFound(name.clone()));
        }
    }
    Ok(args.skills.clone())
}

async fn update_one(
    cfg: &Config,
    registry: &mut Registry,
    name: &str,
    scope: Scope,
    project_root: Option<&Path>,
) -> Result<()> {
    let installed = registry
        .find(name, scope, project_root)
        .ok_or_else(|| Error::SkillNotFound(name.to_string()))?
        .clone();

    let source = source_from_installed(&installed)?;
    let fetched = fetch(&source).await?;

    if fetched.commit == installed.commit {
        println!("{name}: up-to-date");
        return Ok(());
    }

    install_to_master(&fetched.skill_dir, &installed.store_path)?;

    let new_agents = augmented_agents(scope, &installed.agents, cfg);
    let agent_dirs = resolve_agent_dirs(cfg, &new_agents, scope, project_root)?;
    link_into_agents(&installed.store_path, &agent_dirs, installed.method)?;

    let new_commit = fetched.commit;
    let old_short = short_sha(&installed.commit);
    let new_short = short_sha(&new_commit);
    println!("{name}: updated {old_short} -> {new_short}");

    let entry = registry
        .find_mut(name, scope, project_root)
        .ok_or_else(|| Error::SkillNotFound(name.to_string()))?;
    entry.commit = new_commit;
    entry.updated_at = Utc::now();
    entry.agents = new_agents;
    Ok(())
}

/// Re-derive a `SkillSource` from the canonical `owner/repo[/sub_path]` form
/// stored in the registry, attaching the saved `ref_` if any.
fn source_from_installed(installed: &InstalledSkill) -> Result<SkillSource> {
    let parsed = parse_source(&installed.source)?;
    Ok(SkillSource {
        owner: parsed.owner,
        repo: parsed.repo,
        sub_path: parsed.sub_path,
        ref_: installed.ref_.clone(),
    })
}

/// For global scope, union the existing agents with any newly-default agents
/// that are still defined in `config.agents`. For project scope, just keep the
/// existing list.
fn augmented_agents(scope: Scope, existing: &[String], cfg: &Config) -> Vec<String> {
    let mut out: Vec<String> = existing.to_vec();
    if scope == Scope::Global {
        for name in &cfg.default_agents {
            if cfg.agent(name).is_some() && !out.iter().any(|n| n == name) {
                out.push(name.clone());
            }
        }
    }
    out
}

fn resolve_agent_dirs(
    cfg: &Config,
    agents: &[String],
    scope: Scope,
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let mut out = Vec::with_capacity(agents.len());
    for name in agents {
        let agent = cfg
            .agent(name)
            .ok_or_else(|| Error::ConfigError(format!("unknown agent: {name}")))?;
        let dir = match scope {
            Scope::Global => expand_path(&agent.global_dir),
            Scope::Project => {
                let raw = expand_path(&agent.project_dir);
                if raw.is_absolute() {
                    raw
                } else {
                    project_root.unwrap_or_else(|| Path::new(".")).join(raw)
                }
            }
        };
        out.push(dir);
    }
    Ok(out)
}

fn short_sha(commit: &str) -> &str {
    let end = commit.len().min(7);
    commit.get(..end).unwrap_or(commit)
}
