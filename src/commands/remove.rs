use std::path::{Path, PathBuf};

use crate::cli::RemoveArgs;
use crate::config::{Config, expand_path};
use crate::error::{Error, Result};
use crate::install::{remove_master, uninstall_from_agents};
use crate::registry::{InstalledSkill, Registry, Scope};
use crate::ui;

/// Implements the `remove` flow:
/// 1. resolve scope filter 2. pick targets 3. confirm 4. uninstall + delete master 5. update registry.
///
/// # Errors
///
/// Returns [`Error::SkillNotFound`] when a requested name doesn't exist in the
/// chosen scope, [`Error::TtyRequired`] for interactive prompts without a TTY,
/// or any underlying I/O / registry failure.
#[expect(
    clippy::unused_async,
    reason = "dispatched via async match arm in cli::run"
)]
pub async fn run(args: RemoveArgs) -> Result<()> {
    if args.global && args.project {
        return Err(Error::InvalidScope(
            "--global and --project are mutually exclusive".to_string(),
        ));
    }

    let cfg = Config::load()?;
    let mut registry = Registry::load()?;
    let cwd = std::env::current_dir()?;

    let scope_filter = ScopeFilter::from_args(args.global, args.project);

    let targets = resolve_targets(&args.skills, &registry, scope_filter, &cwd)?;
    if targets.is_empty() {
        println!("No skills selected.");
        return Ok(());
    }

    if !args.yes && ui::is_tty() {
        let prompt = format!("Remove {} skill(s)?", targets.len());
        if !ui::confirm(&prompt)? {
            println!("Aborted.");
            return Ok(());
        }
    }

    for target in &targets {
        let skill = registry
            .find(&target.name, target.scope, target.project_path.as_deref())
            .ok_or_else(|| Error::SkillNotFound(target.name.clone()))?
            .clone();

        let agent_dirs = resolve_agent_dirs(&cfg, &skill);
        uninstall_from_agents(&agent_dirs, &skill.name)?;
        remove_master(&skill.store_path)?;

        registry.remove(&skill.name, skill.scope, skill.project_path.as_deref());
    }

    registry.save()?;

    for target in &targets {
        println!("Removed: {} [{}]", target.name, scope_label(target.scope));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ScopeFilter {
    Global,
    Project,
    Either,
}

impl ScopeFilter {
    fn from_args(global: bool, project: bool) -> Self {
        match (global, project) {
            (true, _) => Self::Global,
            (_, true) => Self::Project,
            _ => Self::Either,
        }
    }
}

#[derive(Debug, Clone)]
struct Target {
    name: String,
    scope: Scope,
    project_path: Option<PathBuf>,
}

fn resolve_targets(
    requested: &[String],
    registry: &Registry,
    filter: ScopeFilter,
    cwd: &Path,
) -> Result<Vec<Target>> {
    let candidates = collect_candidates(registry, filter, cwd);

    if requested.is_empty() {
        return interactive_pick(&candidates);
    }

    let mut out = Vec::with_capacity(requested.len());
    for name in requested {
        let target = pick_for_name(name, &candidates, filter, cwd)?;
        out.push(target);
    }
    Ok(out)
}

fn collect_candidates(registry: &Registry, filter: ScopeFilter, cwd: &Path) -> Vec<Target> {
    registry
        .skills
        .iter()
        .filter(|s| matches_filter(s, filter, cwd))
        .map(|s| Target {
            name: s.name.clone(),
            scope: s.scope,
            project_path: s.project_path.clone(),
        })
        .collect()
}

fn matches_filter(skill: &InstalledSkill, filter: ScopeFilter, cwd: &Path) -> bool {
    match (filter, skill.scope) {
        (ScopeFilter::Global | ScopeFilter::Either, Scope::Global) => true,
        (ScopeFilter::Project | ScopeFilter::Either, Scope::Project) => {
            skill.project_path.as_deref() == Some(cwd)
        }
        _ => false,
    }
}

fn pick_for_name(
    name: &str,
    candidates: &[Target],
    filter: ScopeFilter,
    cwd: &Path,
) -> Result<Target> {
    let matches: Vec<&Target> = candidates.iter().filter(|t| t.name == name).collect();
    match matches.as_slice() {
        [] => Err(Error::SkillNotFound(name.to_string())),
        [only] => Ok((*only).clone()),
        many => disambiguate(name, many, filter, cwd),
    }
}

fn disambiguate(
    name: &str,
    matches: &[&Target],
    filter: ScopeFilter,
    cwd: &Path,
) -> Result<Target> {
    if matches!(filter, ScopeFilter::Global | ScopeFilter::Project)
        && let Some(first) = matches.first()
    {
        // Filter already narrowed scope; >1 entries here would mean duplicates
        // for the same scope/cwd, which shouldn't happen — take the first.
        return Ok((*first).clone());
    }
    if let Some(t) = matches
        .iter()
        .find(|t| t.scope == Scope::Project && t.project_path.as_deref() == Some(cwd))
    {
        return Ok((*t).clone());
    }
    if let Some(t) = matches.iter().find(|t| t.scope == Scope::Global) {
        return Ok((*t).clone());
    }
    Err(Error::SkillNotFound(name.to_string()))
}

fn interactive_pick(candidates: &[Target]) -> Result<Vec<Target>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    if !ui::is_tty() {
        return Err(Error::TtyRequired);
    }
    let labels: Vec<String> = candidates
        .iter()
        .map(|t| format!("{} [{}]", t.name, scope_label(t.scope)))
        .collect();
    let picks = ui::multiselect_agents(&labels, &[])?;
    let chosen: Vec<Target> = labels
        .iter()
        .zip(candidates.iter())
        .filter_map(|(label, target)| {
            if picks.iter().any(|p| p == label) {
                Some(target.clone())
            } else {
                None
            }
        })
        .collect();
    Ok(chosen)
}

fn resolve_agent_dirs(cfg: &Config, skill: &InstalledSkill) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(skill.agents.len());
    for name in &skill.agents {
        // Agent removed from config since install -> skip silently rather
        // than block removal of the skill itself.
        let Some(agent) = cfg.agent(name) else {
            continue;
        };
        let dir = match skill.scope {
            Scope::Global => expand_path(&agent.global_dir),
            Scope::Project => {
                let raw = expand_path(&agent.project_dir);
                if raw.is_absolute() {
                    raw
                } else {
                    let root = skill
                        .project_path
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("."));
                    root.join(raw)
                }
            }
        };
        out.push(dir);
    }
    out
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "global",
        Scope::Project => "project",
    }
}
