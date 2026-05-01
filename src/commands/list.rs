use tabled::builder::Builder;
use tabled::settings::Style;

use crate::cli::ListArgs;
use crate::error::Result;
use crate::registry::{InstalledSkill, Method, Registry, Scope};

/// Implements the `list` flow: load registry, filter by scope, render JSON or table.
///
/// # Errors
///
/// Returns [`crate::error::Error::Io`] / [`crate::error::Error::Json`] if the registry
/// cannot be loaded, or [`crate::error::Error::Io`] if the cwd cannot be read for
/// `--project` filtering.
#[expect(
    clippy::unused_async,
    reason = "command dispatcher uses async fn signatures uniformly"
)]
pub async fn run(args: ListArgs) -> Result<()> {
    let registry = Registry::load()?;

    let cwd = if args.project {
        Some(std::env::current_dir()?)
    } else {
        None
    };

    let filtered: Vec<&InstalledSkill> = if args.global {
        registry.iter_scope(Scope::Global, None).collect()
    } else if let Some(cwd) = cwd.as_deref() {
        registry.iter_scope(Scope::Project, Some(cwd)).collect()
    } else {
        registry.skills.iter().collect()
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
        return Ok(());
    }

    if filtered.is_empty() {
        println!("No skills installed.");
        return Ok(());
    }

    println!("{}", render_table(&filtered));
    Ok(())
}

fn render_table(skills: &[&InstalledSkill]) -> String {
    let mut builder = Builder::default();
    builder.push_record(["NAME", "SCOPE", "METHOD", "SOURCE", "AGENTS"]);
    for s in skills {
        builder.push_record([
            s.name.clone(),
            scope_str(s.scope).to_string(),
            method_str(s.method).to_string(),
            s.source.clone(),
            s.agents.join(","),
        ]);
    }
    let mut table = builder.build();
    table.with(Style::psql());
    table.to_string()
}

fn scope_str(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "global",
        Scope::Project => "project",
    }
}

fn method_str(method: Method) -> &'static str {
    match method {
        Method::Symlink => "symlink",
        Method::Copy => "copy",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;

    use super::*;

    fn skill(name: &str, scope: Scope) -> InstalledSkill {
        let now = Utc::now();
        InstalledSkill {
            name: name.to_string(),
            source: "owner/repo/path".to_string(),
            ref_: None,
            commit: "deadbeef".to_string(),
            scope,
            project_path: None,
            method: Method::Symlink,
            agents: vec!["claude-code".to_string()],
            store_path: PathBuf::from("/tmp/store/global/foo"),
            installed_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn render_table_includes_header_and_row() {
        let s = skill("foo", Scope::Global);
        let refs: Vec<&InstalledSkill> = vec![&s];
        let out = render_table(&refs);
        assert!(out.contains("NAME"));
        assert!(out.contains("foo"));
        assert!(out.contains("global"));
        assert!(out.contains("symlink"));
        assert!(out.contains("claude-code"));
    }

    #[test]
    fn scope_and_method_strings() {
        assert_eq!(scope_str(Scope::Global), "global");
        assert_eq!(scope_str(Scope::Project), "project");
        assert_eq!(method_str(Method::Symlink), "symlink");
        assert_eq!(method_str(Method::Copy), "copy");
    }
}
