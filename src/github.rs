use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::error::{Error, Result};

/// Origin of a skill: a remote git repo (any host) or a local directory.
///
/// `parse_source` accepts:
/// - `owner/repo[/sub_path][#ref]` — GitHub shorthand
/// - `https://github.com/owner/repo[.git]`
/// - `https://github.com/owner/repo/tree/<ref>/<sub_path>`
/// - `https://gitlab.com/...` and any other HTTP(S) git URL
/// - `git@host:owner/repo[.git]` and `ssh://...`
/// - `./path`, `../path`, `/abs/path`, `~/path` — local directory containing `SKILL.md`
#[derive(Debug, Clone)]
pub enum SkillSource {
    Git(GitSource),
    Local(LocalSource),
}

/// Anything that ends up cloned via `git`.
#[derive(Debug, Clone)]
pub struct GitSource {
    /// URL passed to `git clone`.
    pub clone_url: String,
    /// Round-trippable representation persisted in the registry.
    pub canonical: String,
    /// Path within the cloned tree where `SKILL.md` lives. Empty = repo root.
    pub sub_path: PathBuf,
    /// Branch / tag / commit, if pinned.
    pub ref_: Option<String>,
    /// Repository basename (used for `skill_name` when `sub_path` is empty).
    pub repo_basename: String,
}

/// A directory on the local filesystem containing a `SKILL.md`.
#[derive(Debug, Clone)]
pub struct LocalSource {
    pub path: PathBuf,
}

impl SkillSource {
    /// Skill name used as the directory name in the master store.
    #[must_use]
    pub fn skill_name(&self) -> String {
        match self {
            SkillSource::Git(g) => g.sub_path.file_name().map_or_else(
                || g.repo_basename.clone(),
                |n| n.to_string_lossy().to_string(),
            ),
            SkillSource::Local(l) => l
                .path
                .file_name()
                .map_or_else(|| "skill".to_string(), |n| n.to_string_lossy().to_string()),
        }
    }

    /// Round-trippable canonical form (sans `#ref`) stored in the registry.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            SkillSource::Git(g) => g.canonical.clone(),
            SkillSource::Local(l) => l.path.to_string_lossy().to_string(),
        }
    }

    /// Optional ref (branch / tag / commit). Always `None` for local sources.
    #[must_use]
    pub fn ref_(&self) -> Option<&str> {
        match self {
            SkillSource::Git(g) => g.ref_.as_deref(),
            SkillSource::Local(_) => None,
        }
    }

    /// Replace the ref. No-op for local sources.
    pub fn set_ref(&mut self, r: Option<String>) {
        if let SkillSource::Git(g) = self {
            g.ref_ = r;
        }
    }
}

/// Parse a user-supplied source string.
///
/// # Errors
///
/// Returns [`Error::InvalidSource`] if the input cannot be classified, or if a
/// local path does not resolve to an existing directory.
pub fn parse_source(s: &str) -> Result<SkillSource> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::InvalidSource("empty source".to_string()));
    }

    if is_local_path(s) {
        return parse_local(s);
    }

    let (rest, ref_) = split_ref(s);

    if rest.starts_with("git@") || rest.starts_with("ssh://") {
        return parse_ssh(rest, ref_);
    }

    if rest.starts_with("https://") || rest.starts_with("http://") {
        return Ok(parse_https(rest, ref_));
    }

    parse_shorthand(rest, ref_)
}

fn is_local_path(s: &str) -> bool {
    s == "."
        || s == ".."
        || s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with('/')
        || s.starts_with("~/")
        || s == "~"
}

fn split_ref(s: &str) -> (&str, Option<String>) {
    match s.split_once('#') {
        Some((r, h)) if !h.is_empty() => (r, Some(h.to_string())),
        _ => (s, None),
    }
}

fn parse_local(s: &str) -> Result<SkillSource> {
    let expanded = shellexpand::tilde(s).into_owned();
    let path = PathBuf::from(&expanded);
    let abs = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let canonical = std::fs::canonicalize(&abs).map_err(|e| {
        Error::InvalidSource(format!("local path {} not accessible: {e}", abs.display()))
    })?;
    if !canonical.is_dir() {
        return Err(Error::InvalidSource(format!(
            "local source must be a directory: {}",
            canonical.display()
        )));
    }
    Ok(SkillSource::Local(LocalSource { path: canonical }))
}

fn parse_shorthand(rest: &str, ref_: Option<String>) -> Result<SkillSource> {
    let parts: Vec<&str> = rest
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(Error::InvalidSource(format!(
            "expected owner/repo[/sub_path], a git URL, or a local path; got {rest:?}"
        )));
    }
    let owner = parts[0];
    let repo = parts[1].to_string();
    let sub_path: PathBuf = if parts.len() > 2 {
        parts[2..].iter().collect()
    } else {
        PathBuf::new()
    };
    Ok(github_source(owner, repo, sub_path, ref_))
}

fn parse_ssh(rest: &str, ref_: Option<String>) -> Result<SkillSource> {
    let path = if let Some(after) = rest.strip_prefix("ssh://") {
        // ssh://[user@]host[:port]/path/to/repo[.git]
        after
            .split_once('/')
            .map(|(_, p)| p)
            .ok_or_else(|| Error::InvalidSource(format!("missing path in ssh url: {rest}")))?
    } else {
        // git@host:owner/repo[.git]
        rest.rsplit_once(':')
            .map(|(_, p)| p)
            .ok_or_else(|| Error::InvalidSource(format!("expected git@host:path, got {rest}")))?
    };
    let basename = repo_basename_from_path(path);
    Ok(SkillSource::Git(GitSource {
        clone_url: rest.to_string(),
        canonical: rest.to_string(),
        sub_path: PathBuf::new(),
        ref_,
        repo_basename: basename,
    }))
}

fn parse_https(rest: &str, ref_: Option<String>) -> SkillSource {
    if let Some((owner, repo, url_ref, sub_path)) = parse_github_tree(rest) {
        let resolved_ref = ref_.or(Some(url_ref));
        return github_source(&owner, repo, sub_path, resolved_ref);
    }
    if let Some((owner, repo)) = parse_github_plain(rest) {
        return github_source(&owner, repo, PathBuf::new(), ref_);
    }
    let path_part = rest
        .strip_prefix("https://")
        .or_else(|| rest.strip_prefix("http://"))
        .unwrap_or(rest);
    let (_authority, path) = path_part.split_once('/').unwrap_or(("", path_part));
    let basename = repo_basename_from_path(path);
    SkillSource::Git(GitSource {
        clone_url: rest.to_string(),
        canonical: rest.to_string(),
        sub_path: PathBuf::new(),
        ref_,
        repo_basename: basename,
    })
}

fn github_source(
    owner: &str,
    repo: String,
    sub_path: PathBuf,
    ref_: Option<String>,
) -> SkillSource {
    let canonical = if sub_path.as_os_str().is_empty() {
        format!("{owner}/{repo}")
    } else {
        format!("{}/{}/{}", owner, repo, sub_path.to_string_lossy())
    };
    SkillSource::Git(GitSource {
        clone_url: format!("https://github.com/{owner}/{repo}.git"),
        canonical,
        sub_path,
        ref_,
        repo_basename: repo,
    })
}

fn parse_github_tree(url: &str) -> Option<(String, String, String, PathBuf)> {
    let path = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 5 || parts[0] != "github.com" || parts[3] != "tree" {
        return None;
    }
    let owner = parts[1].to_string();
    let repo = parts[2].trim_end_matches(".git").to_string();
    let url_ref = parts[4].to_string();
    let sub_path: PathBuf = parts[5..].iter().collect();
    Some((owner, repo, url_ref, sub_path))
}

fn parse_github_plain(url: &str) -> Option<(String, String)> {
    let path = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 3 || parts[0] != "github.com" {
        return None;
    }
    let owner = parts[1].to_string();
    let repo = parts[2].trim_end_matches(".git").to_string();
    Some((owner, repo))
}

fn repo_basename_from_path(path: &str) -> String {
    let last = path.rsplit('/').find(|p| !p.is_empty()).unwrap_or(path);
    last.trim_end_matches(".git").to_string()
}

/// Result of fetching a skill into a working directory.
///
/// Hold onto `tempdir` for the lifetime of any operations on `skill_dir` —
/// dropping it removes the clone.
pub struct FetchedSkill {
    pub tempdir: TempDir,
    pub skill_dir: PathBuf,
    pub commit: String,
}

/// Fetch a skill: clone if remote, validate `SKILL.md`, capture HEAD commit.
///
/// For [`SkillSource::Local`], `skill_dir` points at the original local path
/// (no copy), and `commit` is derived from `SKILL.md`'s mtime so reinstall
/// detects edits.
///
/// # Errors
///
/// Returns [`Error::GitClone`], [`Error::SkillMdMissing`], or an underlying
/// I/O error.
pub async fn fetch(source: &SkillSource) -> Result<FetchedSkill> {
    match source {
        SkillSource::Git(g) => fetch_git(g),
        SkillSource::Local(l) => fetch_local(l),
    }
}

fn fetch_git(source: &GitSource) -> Result<FetchedSkill> {
    let tempdir = TempDir::new()?;
    let clone_dir = tempdir.path().join("repo");

    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(r) = &source.ref_ {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(&source.clone_url).arg(&clone_dir);

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

fn fetch_local(source: &LocalSource) -> Result<FetchedSkill> {
    if !source.path.is_dir() {
        return Err(Error::InvalidSource(format!(
            "local source no longer exists: {}",
            source.path.display()
        )));
    }
    let skill_md = source.path.join("SKILL.md");
    if !skill_md.is_file() {
        return Err(Error::SkillMdMissing(source.path.display().to_string()));
    }
    let commit = local_commit(&skill_md)?;
    let tempdir = TempDir::new()?;
    Ok(FetchedSkill {
        tempdir,
        skill_dir: source.path.clone(),
        commit,
    })
}

fn local_commit(skill_md: &Path) -> Result<String> {
    let mtime = std::fs::metadata(skill_md)?
        .modified()
        .map_err(|e| Error::Io(std::io::Error::other(format!("mtime: {e}"))))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::Io(std::io::Error::other(format!("mtime before epoch: {e}"))))?
        .as_secs();
    Ok(format!("local-{mtime}"))
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

    fn unwrap_git(s: &SkillSource) -> &GitSource {
        match s {
            SkillSource::Git(g) => g,
            SkillSource::Local(_) => panic!("expected Git, got Local"),
        }
    }

    fn unwrap_local(s: &SkillSource) -> &LocalSource {
        match s {
            SkillSource::Local(l) => l,
            SkillSource::Git(_) => panic!("expected Local, got Git"),
        }
    }

    #[test]
    fn parse_owner_repo() -> Result<()> {
        let s = parse_source("a/b")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "a/b");
        assert_eq!(g.clone_url, "https://github.com/a/b.git");
        assert!(g.sub_path.as_os_str().is_empty());
        assert!(g.ref_.is_none());
        assert_eq!(s.skill_name(), "b");
        Ok(())
    }

    #[test]
    fn parse_with_subpath_and_ref() -> Result<()> {
        let s = parse_source("o/r/skills/foo#main")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "o/r/skills/foo");
        assert_eq!(g.clone_url, "https://github.com/o/r.git");
        assert_eq!(g.sub_path, PathBuf::from("skills/foo"));
        assert_eq!(g.ref_.as_deref(), Some("main"));
        assert_eq!(s.skill_name(), "foo");
        Ok(())
    }

    #[test]
    fn parse_invalid_solo_token() {
        assert!(parse_source("solo").is_err());
    }

    #[test]
    fn parse_github_https_url_normalizes_to_shorthand() -> Result<()> {
        let s = parse_source("https://github.com/expo/skills")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "expo/skills");
        assert_eq!(g.clone_url, "https://github.com/expo/skills.git");
        assert!(g.sub_path.as_os_str().is_empty());
        assert!(g.ref_.is_none());
        assert_eq!(s.skill_name(), "skills");
        Ok(())
    }

    #[test]
    fn parse_github_https_url_with_dot_git_suffix() -> Result<()> {
        let s = parse_source("https://github.com/expo/skills.git")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "expo/skills");
        assert_eq!(g.clone_url, "https://github.com/expo/skills.git");
        Ok(())
    }

    #[test]
    fn parse_github_tree_url_extracts_ref_and_subpath() -> Result<()> {
        let s = parse_source(
            "https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines",
        )?;
        let g = unwrap_git(&s);
        assert_eq!(
            g.canonical,
            "vercel-labs/agent-skills/skills/web-design-guidelines"
        );
        assert_eq!(
            g.clone_url,
            "https://github.com/vercel-labs/agent-skills.git"
        );
        assert_eq!(g.sub_path, PathBuf::from("skills/web-design-guidelines"));
        assert_eq!(g.ref_.as_deref(), Some("main"));
        assert_eq!(s.skill_name(), "web-design-guidelines");
        Ok(())
    }

    #[test]
    fn parse_github_tree_url_explicit_hash_ref_wins() -> Result<()> {
        let s = parse_source("https://github.com/o/r/tree/main/sub#release")?;
        let g = unwrap_git(&s);
        assert_eq!(g.ref_.as_deref(), Some("release"));
        assert_eq!(g.sub_path, PathBuf::from("sub"));
        Ok(())
    }

    #[test]
    fn parse_gitlab_url_kept_verbatim() -> Result<()> {
        let s = parse_source("https://gitlab.com/org/repo")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "https://gitlab.com/org/repo");
        assert_eq!(g.clone_url, "https://gitlab.com/org/repo");
        assert_eq!(s.skill_name(), "repo");
        Ok(())
    }

    #[test]
    fn parse_ssh_url_kept_verbatim() -> Result<()> {
        let s = parse_source("git@github.com:expo/skills.git")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "git@github.com:expo/skills.git");
        assert_eq!(g.clone_url, "git@github.com:expo/skills.git");
        assert_eq!(s.skill_name(), "skills");
        Ok(())
    }

    #[test]
    fn parse_ssh_protocol_url() -> Result<()> {
        let s = parse_source("ssh://git@github.com/expo/skills.git#main")?;
        let g = unwrap_git(&s);
        assert_eq!(g.canonical, "ssh://git@github.com/expo/skills.git");
        assert_eq!(g.ref_.as_deref(), Some("main"));
        assert_eq!(s.skill_name(), "skills");
        Ok(())
    }

    #[test]
    fn parse_local_relative_path() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let nested = dir.path().join("my-skill");
        std::fs::create_dir_all(&nested)?;
        std::fs::write(nested.join("SKILL.md"), "ok")?;
        let prev_cwd = std::env::current_dir()?;
        std::env::set_current_dir(dir.path())?;
        let result = parse_source("./my-skill");
        std::env::set_current_dir(prev_cwd)?;
        let s = result?;
        let l = unwrap_local(&s);
        assert!(l.path.is_absolute());
        assert!(l.path.ends_with("my-skill"));
        assert_eq!(s.skill_name(), "my-skill");
        Ok(())
    }

    #[test]
    fn parse_local_absolute_path() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let nested = dir.path().join("the-skill");
        std::fs::create_dir_all(&nested)?;
        std::fs::write(nested.join("SKILL.md"), "ok")?;
        let s = parse_source(&nested.display().to_string())?;
        let l = unwrap_local(&s);
        assert!(l.path.is_absolute());
        assert_eq!(s.skill_name(), "the-skill");
        Ok(())
    }

    #[test]
    fn parse_local_path_must_exist() {
        assert!(parse_source("./does-not-exist-xyz-123").is_err());
    }

    #[test]
    fn ref_setter_no_op_for_local() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let nested = dir.path().join("s");
        std::fs::create_dir_all(&nested)?;
        std::fs::write(nested.join("SKILL.md"), "ok")?;
        let mut s = parse_source(&nested.display().to_string())?;
        s.set_ref(Some("main".into()));
        assert!(s.ref_().is_none());
        Ok(())
    }
}
