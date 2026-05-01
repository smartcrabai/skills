use std::env;
use std::path::Path;

use crate::cli::InitArgs;
use crate::config::write_atomic;
use crate::error::{Error, Result};

/// Implements the `init` flow: scaffold a new `SKILL.md` template.
///
/// Resolves the skill name from `args.name` or, when absent, the basename of
/// the current working directory. With an explicit name, writes
/// `<cwd>/<name>/SKILL.md` (creating the directory). Without a name, writes
/// `<cwd>/SKILL.md`. Refuses to overwrite an existing target file.
///
/// # Errors
///
/// Returns [`Error::ConfigError`] when the working directory has no usable
/// basename or the target file already exists, or [`Error::Io`] on filesystem
/// failures.
#[expect(
    clippy::unused_async,
    reason = "kept async to match dispatcher signature"
)]
pub async fn run(args: InitArgs) -> Result<()> {
    let cwd = env::current_dir()?;

    let (name, target) = if let Some(provided) = args.name {
        let target = cwd.join(&provided).join("SKILL.md");
        (provided, target)
    } else {
        let basename = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                Error::ConfigError(
                    "cannot determine skill name: current directory has no basename".to_string(),
                )
            })?;
        let target = cwd.join("SKILL.md");
        (basename, target)
    };

    if target.exists() {
        return Err(Error::ConfigError(format!(
            "SKILL.md already exists at {}",
            target.display()
        )));
    }

    let body = render_template(&name);
    write_atomic(&target, body.as_bytes())?;

    let display = display_path(&cwd, &target);
    println!("Created {display}");
    Ok(())
}

fn render_template(name: &str) -> String {
    format!(
        "---\n\
name: {name}\n\
description: A brief description of what this skill does\n\
---\n\
\n\
# {name}\n\
\n\
Instructions for the agent to follow when this skill is activated.\n\
\n\
## When to use\n\
\n\
Describe when this skill should be used.\n\
\n\
## Instructions\n\
\n\
1. First step\n\
2. Second step\n\
3. Additional steps as needed\n"
    )
}

fn display_path(cwd: &Path, target: &Path) -> String {
    target.strip_prefix(cwd).map_or_else(
        |_| target.display().to_string(),
        |rel| Path::new(".").join(rel).display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_includes_name_in_frontmatter_and_heading() {
        let out = render_template("my-skill");
        assert!(out.contains("name: my-skill"));
        assert!(out.contains("\n# my-skill\n"));
        assert!(out.starts_with("---\n"));
    }
}
