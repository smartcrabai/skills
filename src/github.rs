use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::error::{Error, Result};

/// Parsed `owner/repo[/sub_path][#ref]` source string.
#[derive(Debug, Clone)]
pub struct SkillSource {
    pub owner: String,
    pub repo: String,
    pub sub_path: PathBuf,
    pub ref_: Option<String>,
}

impl SkillSource {
    /// HTTPS clone URL.
    #[must_use]
    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repo)
    }

    /// Skill name = the last component of `sub_path`, or `repo` if path is empty.
    #[must_use]
    pub fn skill_name(&self) -> String {
        self.sub_path
            .file_name()
            .map_or_else(|| self.repo.clone(), |n| n.to_string_lossy().to_string())
    }

    /// Canonical `owner/repo[/sub_path]` form (without `#ref`).
    #[must_use]
    pub fn canonical(&self) -> String {
        let sub = self.sub_path.to_string_lossy();
        if sub.is_empty() {
            format!("{}/{}", self.owner, self.repo)
        } else {
            format!("{}/{}/{}", self.owner, self.repo, sub)
        }
    }
}

/// Parse `owner/repo[/sub_path][#ref]`.
///
/// # Errors
///
/// Returns [`Error::InvalidSource`] if the input is missing `owner/repo`.
pub fn parse_source(s: &str) -> Result<SkillSource> {
    let (rest, ref_) = match s.split_once('#') {
        Some((r, h)) if !h.is_empty() => (r, Some(h.to_string())),
        _ => (s, None),
    };
    let parts: Vec<&str> = rest
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(Error::InvalidSource(format!(
            "expected owner/repo[/sub_path], got {s:?}"
        )));
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    let sub_path = if parts.len() > 2 {
        parts[2..].iter().collect::<PathBuf>()
    } else {
        PathBuf::new()
    };
    Ok(SkillSource {
        owner,
        repo,
        sub_path,
        ref_,
    })
}

/// Result of fetching a skill from GitHub.
///
/// Hold onto `tempdir` for the lifetime of any operations on `skill_dir` —
/// dropping it removes the clone.
pub struct FetchedSkill {
    pub tempdir: TempDir,
    pub skill_dir: PathBuf,
    pub commit: String,
}

/// Shallow-clone the source repo into a temp dir, validate `SKILL.md`, capture HEAD commit.
///
/// # Errors
///
/// Returns [`Error::GitClone`], [`Error::SkillMdMissing`], or an underlying I/O error.
pub async fn fetch(source: &SkillSource) -> Result<FetchedSkill> {
    let tempdir = TempDir::new()?;
    let clone_dir = tempdir.path().join("repo");

    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(r) = &source.ref_ {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(source.clone_url()).arg(&clone_dir);

    let output = cmd
        .output()
        .map_err(|e| Error::GitClone(format!("failed to spawn git: {e}")))?;
    if !output.status.success() {
        return Err(Error::GitClone(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let skill_dir = if source.sub_path.as_os_str().is_empty() {
        clone_dir.clone()
    } else {
        clone_dir.join(&source.sub_path)
    };
    let skill_md = skill_dir.join("SKILL.md");
    if !skill_md.is_file() {
        return Err(Error::SkillMdMissing(skill_dir.display().to_string()));
    }

    let commit = head_commit(&clone_dir)?;
    Ok(FetchedSkill {
        tempdir,
        skill_dir,
        commit,
    })
}

fn head_commit(repo_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|e| Error::GitClone(format!("failed to spawn git rev-parse: {e}")))?;
    if !output.status.success() {
        return Err(Error::GitClone(format!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_owner_repo() {
        let s = parse_source("a/b").expect("parse");
        assert_eq!(s.owner, "a");
        assert_eq!(s.repo, "b");
        assert!(s.sub_path.as_os_str().is_empty());
        assert!(s.ref_.is_none());
        assert_eq!(s.skill_name(), "b");
    }

    #[test]
    fn parse_with_subpath_and_ref() {
        let s = parse_source("o/r/skills/foo#main").expect("parse");
        assert_eq!(s.owner, "o");
        assert_eq!(s.repo, "r");
        assert_eq!(s.sub_path, PathBuf::from("skills/foo"));
        assert_eq!(s.ref_.as_deref(), Some("main"));
        assert_eq!(s.skill_name(), "foo");
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_source("solo").is_err());
    }
}
