use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::cli::AddArgs;
use crate::config::{Config, expand_path};
use crate::error::{Error, Result};
use crate::github::{SkillSource, fetch, parse_source};
use crate::install::{install_to_master, link_into_agents};
use crate::registry::{InstalledSkill, Method, Registry, Scope};
use crate::ui;

/// Implements the `add` flow:
/// 1. parse source 2. resolve scope/agents 3. clone 4. populate master 5. link agents 6. record.
///
/// # Errors
///
/// Surfaces any underlying failure (network, FS, registry corruption).
pub async fn run(args: AddArgs) -> Result<()> {
    let source = parse_source(&args.source)?;
    let cfg = Config::load()?;

    let scope = resolve_scope(&args)?;
    let project_root = match scope {
        Scope::Project => Some(std::env::current_dir()?),
        Scope::Global => None,
    };

    let agents = resolve_agents(&args, &cfg)?;
    if agents.is_empty() {
        return Err(Error::ConfigError("no agents selected".to_string()));
    }
    validate_agents(&cfg, &agents)?;

    let skill_name = source.skill_name();
    let project_root_ref = project_root.as_deref();

    let mut registry = Registry::load()?;
    if registry
        .find(&skill_name, scope, project_root_ref)
        .is_some()
    {
        return Err(Error::DuplicateSkill(skill_name));
    }

    let method = if args.copy {
        Method::Copy
    } else {
        Method::Symlink
    };

    let fetched = fetch(&source).await?;

    let master_path = master_dir_for(&cfg, scope, project_root_ref).join(&skill_name);
    install_to_master(&fetched.skill_dir, &master_path)?;

    let agent_dirs = resolve_agent_dirs(&cfg, &agents, scope, project_root_ref)?;
    link_into_agents(&master_path, &agent_dirs, method)?;

    let now = Utc::now();
    registry.add(InstalledSkill {
        name: skill_name.clone(),
        source: source.canonical(),
        ref_: source.ref_.clone(),
        commit: fetched.commit.clone(),
        scope,
        project_path: project_root.clone(),
        method,
        agents: agents.clone(),
        store_path: master_path.clone(),
        installed_at: now,
        updated_at: now,
    });
    registry.save()?;

    print_summary(
        &skill_name,
        &source,
        &master_path,
        &agents,
        &agent_dirs,
        method,
    );
    Ok(())
}

fn resolve_scope(args: &AddArgs) -> Result<Scope> {
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

fn resolve_agents(args: &AddArgs, cfg: &Config) -> Result<Vec<String>> {
    if !args.agents.is_empty() {
        return Ok(args.agents.clone());
    }
    let defaults = cfg.default_agent_names();
    if args.yes || !ui::is_tty() {
        return Ok(defaults);
    }
    let all_names: Vec<String> = cfg.agents.iter().map(|a| a.name.clone()).collect();
    ui::multiselect_agents(&all_names, &defaults)
}

fn validate_agents(cfg: &Config, requested: &[String]) -> Result<()> {
    for name in requested {
        if cfg.agent(name).is_none() {
            return Err(Error::ConfigError(format!("unknown agent: {name}")));
        }
    }
    Ok(())
}

fn master_dir_for(cfg: &Config, scope: Scope, project_root: Option<&Path>) -> PathBuf {
    let base = match scope {
        Scope::Global => cfg.expand_global_store(),
        Scope::Project => cfg.expand_project_store(project_root.unwrap_or_else(|| Path::new("."))),
    };
    let segment = match scope {
        Scope::Global => "global",
        Scope::Project => "project",
    };
    base.join(segment)
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

fn print_summary(
    skill_name: &str,
    source: &SkillSource,
    master_path: &Path,
    agents: &[String],
    agent_dirs: &[PathBuf],
    method: Method,
) {
    let method_str = match method {
        Method::Symlink => "symlink",
        Method::Copy => "copy",
    };
    println!("Installed skill: {skill_name}");
    println!("  source : {}", source.canonical());
    if let Some(r) = &source.ref_ {
        println!("  ref    : {r}");
    }
    println!("  master : {}", master_path.display());
    println!("  method : {method_str}");
    println!("  agents :");
    for (name, dir) in agents.iter().zip(agent_dirs.iter()) {
        println!("    - {name} -> {}", dir.join(skill_name).display());
    }
}
