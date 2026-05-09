use std::path::PathBuf;

use chrono::Utc;

use crate::cli::CreateArgs;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::install::{install_to_master, link_into_agents};
use crate::registry::{InstalledSkill, Method, Registry, Scope};

/// # Errors
///
/// Returns an error if config loading, agent validation, creator invocation, or registry save fails.
pub async fn run(args: CreateArgs) -> Result<()> {
    let cfg = Config::load()?;

    let creator = args.creator.unwrap_or_else(|| cfg.default_creator.clone());
    let (bin, flag) = creator_cli(&creator)?;

    let scope = super::add::resolve_scope(args.global, args.project, args.yes)?;
    let project_root = match scope {
        Scope::Project => Some(std::env::current_dir()?),
        Scope::Global => None,
    };

    let agents = super::add::resolve_agents(&args.agents, args.yes, &cfg)?;
    if agents.is_empty() {
        return Err(Error::ConfigError("no agents selected".to_string()));
    }
    super::add::validate_agents(&cfg, &agents)?;

    let skill_name = match args.name {
        Some(n) => n,
        None => slugify(&args.description)?,
    };

    let mut registry = Registry::load()?;
    if registry
        .find(&skill_name, scope, project_root.as_deref())
        .is_some()
    {
        return Err(Error::DuplicateSkill(skill_name));
    }

    let method = if args.copy {
        Method::Copy
    } else {
        Method::Symlink
    };

    let work_dir = tempfile::tempdir()?;
    invoke_creator(bin, flag, &skill_name, &args.description, work_dir.path()).await?;

    let project_root_ref = project_root.as_deref();
    let master_path = super::add::master_dir_for(&cfg, scope, project_root_ref).join(&skill_name);
    install_to_master(work_dir.path(), &master_path)?;

    let agent_dirs = super::add::resolve_agent_dirs(&cfg, &agents, scope, project_root_ref)?;
    link_into_agents(&master_path, &agent_dirs, method)?;

    let now = Utc::now();
    let skill = InstalledSkill {
        name: skill_name.clone(),
        source: "local".to_string(),
        ref_: None,
        commit: "local".to_string(),
        scope,
        project_path: project_root,
        method,
        agents,
        store_path: master_path.clone(),
        installed_at: now,
        updated_at: now,
    };
    print_summary(
        &skill_name,
        &creator,
        &master_path,
        &skill.agents,
        &agent_dirs,
        method,
    );
    registry.add(skill);
    registry.save()?;
    Ok(())
}

fn slugify(s: &str) -> Result<String> {
    let joined = s
        .to_lowercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = joined.trim_matches('-');
    if slug.is_empty() {
        Err(Error::ConfigError(
            "description contains no usable characters for a skill name; use --name to specify one explicitly".to_string(),
        ))
    } else {
        Ok(slug.to_string())
    }
}

fn creator_cli(agent: &str) -> Result<(&'static str, &'static str)> {
    match agent {
        "claude-code" => Ok(("claude", "-p")),
        _ => Err(Error::ConfigError(format!(
            "no creator CLI defined for agent '{agent}'"
        ))),
    }
}

async fn invoke_creator(
    bin: &str,
    flag: &str,
    skill_name: &str,
    description: &str,
    work_dir: &std::path::Path,
) -> Result<()> {
    let prompt = format!(
        "Create a SKILL.md file in the current directory for an agent skill.\n\
         Skill name: {skill_name}\n\
         Purpose: {description}\n\
         \n\
         The file must be named SKILL.md and follow this frontmatter format:\n\
         ---\n\
         name: {skill_name}\n\
         description: <one-sentence description>\n\
         ---\n\
         \n\
         Followed by markdown content explaining when and how to use the skill,\n\
         including a \"## When to use\" and \"## Instructions\" section."
    );

    let status = tokio::process::Command::new(bin)
        .arg(flag)
        .arg(&prompt)
        .current_dir(work_dir)
        .status()
        .await
        .map_err(|e| Error::CreatorFailed(format!("failed to spawn {bin}: {e}")))?;

    if !status.success() {
        return Err(Error::CreatorFailed(format!("{bin} exited with {status}")));
    }

    std::fs::metadata(work_dir.join("SKILL.md"))
        .map_err(|_| Error::SkillMdMissing(work_dir.display().to_string()))?;
    Ok(())
}

fn print_summary(
    skill_name: &str,
    creator: &str,
    master_path: &std::path::Path,
    agents: &[String],
    agent_dirs: &[PathBuf],
    method: Method,
) {
    println!("Created skill: {skill_name}");
    println!("  creator: {creator}");
    println!("  master : {}", master_path.display());
    println!("  method : {method}");
    println!("  agents :");
    for (name, dir) in agents.iter().zip(agent_dirs.iter()) {
        println!("    - {name} -> {}", dir.join(skill_name).display());
    }
}
