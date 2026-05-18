use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::cli::AddArgs;
use crate::config::{Config, expand_path};
use crate::error::{Error, Result};
use crate::github::{
    DiscoveredSkill, FetchedRepo, GitSource, SkillSource, fetch_repo, parse_source,
};
use crate::install::{install_to_master, link_into_agents};
use crate::registry::{InstalledSkill, Method, Registry, Scope};
use crate::ui;

/// Implements the `add` flow:
/// 1. parse source 2. resolve scope/agents 3. clone 4. discover SKILL.md(s)
/// 5. select 6. for each: populate master, link agents, register.
///
/// # Errors
///
/// Surfaces any underlying failure (network, FS, registry corruption).
pub async fn run(args: AddArgs) -> Result<()> {
    let source = parse_source(&args.source)?;
    let cfg = Config::load()?;

    let scope = resolve_scope(args.global, args.project, args.yes)?;
    let project_root = match scope {
        Scope::Project => Some(std::env::current_dir()?),
        Scope::Global => None,
    };
    let project_root_ref = project_root.as_deref();

    let agents = resolve_agents(&args.agents, args.yes, &cfg)?;
    if agents.is_empty() {
        return Err(Error::ConfigError("no agents selected".to_string()));
    }
    validate_agents(&cfg, &agents)?;

    let method = if args.copy {
        Method::Copy
    } else {
        Method::Symlink
    };

    let repo = fetch_repo(&source).await?;
    let candidates = candidates_for(&source, &repo)?;
    let selected = select_skills(candidates, &args)?;

    let mut registry = Registry::load()?;
    let ref_str = source.ref_().map(str::to_string);
    // Pre-flight: refuse before doing any FS work if any selected name clashes
    // within the current scope, or already owns a shared master from a
    // different source/ref. Cache the canonical source and any existing entry
    // so the install loop below doesn't redo the work.
    let mut preflights: Vec<(String, Option<InstalledSkill>)> = Vec::with_capacity(selected.len());
    for cand in &selected {
        if registry.find(&cand.name, scope, project_root_ref).is_some() {
            return Err(Error::DuplicateSkill(cand.name.clone()));
        }
        let canonical = canonical_for(&source, cand)?;
        let existing = registry.find_by_name(&cand.name).cloned();
        if let Some(other) = &existing
            && (other.source != canonical || other.ref_ != ref_str)
        {
            return Err(Error::DuplicateSkill(format!(
                "{} already installed from {}{} (refusing to overwrite shared master with {}{})",
                cand.name,
                other.source,
                ref_display(other.ref_.as_deref()),
                canonical,
                ref_display(ref_str.as_deref()),
            )));
        }
        preflights.push((canonical, existing));
    }

    let agent_dirs = resolve_agent_dirs(&cfg, &agents, scope, project_root_ref)?;
    let master_root = master_dir_for(&cfg);

    let mut installed_summaries = Vec::with_capacity(selected.len());
    for (cand, (canonical, existing)) in selected.iter().zip(preflights) {
        let (master_path, commit) = if let Some(other) = existing {
            // Master already on disk for this name+source+ref — reuse it as-is
            // so other sharers' contents stay in sync.
            (other.store_path, other.commit)
        } else {
            let path = master_root.join(&cand.name);
            install_to_master(&cand.abs_path, &path)?;
            (path, repo.commit.clone())
        };
        link_into_agents(&master_path, &agent_dirs, method)?;

        let now = Utc::now();
        registry.add(InstalledSkill {
            name: cand.name.clone(),
            source: canonical,
            ref_: ref_str.clone(),
            commit,
            scope,
            project_path: project_root.clone(),
            method,
            agents: agents.clone(),
            store_path: master_path.clone(),
            installed_at: now,
            updated_at: now,
        });
        installed_summaries.push((cand.name.clone(), master_path));
    }
    registry.save()?;

    print_summary(&source, &installed_summaries, &agents, &agent_dirs, method);
    Ok(())
}

/// If the user pinpointed a sub-path (or pointed at a single-skill directory),
/// install just that. Otherwise discover all `SKILL.md` files in the source.
fn candidates_for(source: &SkillSource, repo: &FetchedRepo) -> Result<Vec<DiscoveredSkill>> {
    let sub = source.sub_path();
    if !sub.as_os_str().is_empty() {
        return Ok(vec![repo.skill_at_subpath(sub)?]);
    }
    if repo.root.join("SKILL.md").is_file() {
        return Ok(vec![repo.skill_at_subpath(Path::new(""))?]);
    }
    let all = repo.discover_all()?;
    if all.is_empty() {
        return Err(Error::SkillMdMissing(repo.root.display().to_string()));
    }
    Ok(all)
}

/// Apply `--all` / `--skill <name>...` / interactive prompt rules.
fn select_skills(candidates: Vec<DiscoveredSkill>, args: &AddArgs) -> Result<Vec<DiscoveredSkill>> {
    if candidates.len() == 1 {
        return Ok(candidates);
    }
    if args.all {
        return Ok(candidates);
    }
    if !args.skills.is_empty() {
        let wanted: HashSet<&str> = args.skills.iter().map(String::as_str).collect();
        let chosen: Vec<DiscoveredSkill> = candidates
            .iter()
            .filter(|c| wanted.contains(c.name.as_str()))
            .cloned()
            .collect();
        let found_names: HashSet<&str> = chosen.iter().map(|c| c.name.as_str()).collect();
        for w in &wanted {
            if !found_names.contains(w) {
                return Err(Error::SkillNotFound(format!(
                    "{w} (not present in source; available: {})",
                    candidates
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        return Ok(chosen);
    }
    if ui::is_tty() && !args.yes {
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        let picks = ui::multiselect_skills(&names)?;
        let chosen: HashSet<usize> = picks.into_iter().collect();
        let result: Vec<DiscoveredSkill> = candidates
            .into_iter()
            .enumerate()
            .filter(|(i, _)| chosen.contains(i))
            .map(|(_, c)| c)
            .collect();
        return Ok(result);
    }
    let names = candidates
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(Error::ConfigError(format!(
        "found {} skills in source; pass --skill <name> (repeatable) or --all (or run on a TTY for an interactive prompt). Found: {names}",
        candidates.len()
    )))
}

/// Build the registry `source` string for one discovered skill.
///
/// - Local: the skill's own absolute path (round-trips via `parse_source`).
/// - GitHub URL/shorthand: `owner/repo[/sub_path]` (normalized).
/// - Other URL: only round-trippable when `sub_path` is empty; otherwise we
///   reject the install and explain.
fn canonical_for(source: &SkillSource, discovered: &DiscoveredSkill) -> Result<String> {
    match source {
        SkillSource::Local(_) => Ok(discovered.abs_path.to_string_lossy().to_string()),
        SkillSource::Git(g) => canonical_for_git(g, &discovered.sub_path),
    }
}

fn canonical_for_git(g: &GitSource, sub_path: &Path) -> Result<String> {
    if let Some(owner_repo) = github_owner_repo(&g.clone_url) {
        if sub_path.as_os_str().is_empty() {
            return Ok(owner_repo);
        }
        return Ok(format!("{owner_repo}/{}", sub_path.to_string_lossy()));
    }
    if sub_path.as_os_str().is_empty() {
        return Ok(g.canonical.clone());
    }
    Err(Error::InvalidSource(format!(
        "discovered skill at sub-path {} inside non-GitHub URL {}; \
         re-run with that sub-path included in the source so the entry is round-trippable",
        sub_path.display(),
        g.canonical
    )))
}

fn github_owner_repo(clone_url: &str) -> Option<String> {
    let inner = clone_url.strip_prefix("https://github.com/")?;
    Some(inner.trim_end_matches(".git").to_string())
}

pub(crate) fn resolve_scope(global: bool, project: bool, yes: bool) -> Result<Scope> {
    match (global, project) {
        (true, true) => Err(Error::InvalidScope(
            "--global and --project are mutually exclusive".to_string(),
        )),
        (true, false) => Ok(Scope::Global),
        (false, true) => Ok(Scope::Project),
        (false, false) if ui::is_tty() && !yes => ui::select_scope(Some(Scope::Global)),
        (false, false) => Err(Error::InvalidScope(
            "specify --global or --project (no TTY for interactive prompt)".to_string(),
        )),
    }
}

pub(crate) fn resolve_agents(agents: &[String], yes: bool, cfg: &Config) -> Result<Vec<String>> {
    if !agents.is_empty() {
        return Ok(agents.to_vec());
    }
    let defaults = cfg.default_agent_names();
    if yes || !ui::is_tty() {
        return Ok(defaults);
    }
    let all_names: Vec<String> = cfg.agents.iter().map(|a| a.name.clone()).collect();
    ui::multiselect_agents(&all_names, &defaults)
}

pub(crate) fn validate_agents(cfg: &Config, requested: &[String]) -> Result<()> {
    for name in requested {
        if cfg.agent(name).is_none() {
            return Err(Error::ConfigError(format!("unknown agent: {name}")));
        }
    }
    Ok(())
}

/// Master store root for skill data. Always the user-level shared store so
/// installs in different scopes/projects deduplicate on disk.
pub(crate) fn master_dir_for(cfg: &Config) -> PathBuf {
    cfg.expand_global_store()
}

fn ref_display(r: Option<&str>) -> String {
    r.map_or_else(String::new, |s| format!("#{s}"))
}

pub(crate) fn resolve_agent_dirs(
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

fn print_summary(
    source: &SkillSource,
    installed: &[(String, PathBuf)],
    agents: &[String],
    agent_dirs: &[PathBuf],
    method: Method,
) {
    let label = if installed.len() == 1 {
        "skill"
    } else {
        "skills"
    };
    println!("Installed {} {label}:", installed.len());
    println!("  source : {}", source.canonical());
    if let Some(r) = source.ref_() {
        println!("  ref    : {r}");
    }
    println!("  method : {method}");
    for (name, master_path) in installed {
        println!("  - {name}");
        println!("      master: {}", master_path.display());
        for (agent, dir) in agents.iter().zip(agent_dirs.iter()) {
            println!("      {agent}: {}", dir.join(name).display());
        }
    }
}
