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

    /// Source-type discriminator used by `skills-lock.json`.
    #[must_use]
    pub fn source_type(&self) -> &'static str {
        match self {
            SkillSource::Local(_) => "local",
            SkillSource::Git(g) if g.is_github() => "github",
            SkillSource::Git(_) => "git",
        }
    }
}

impl GitSource {
    /// `owner/repo` (no `.git` suffix) when the clone URL is GitHub HTTPS.
    #[must_use]
    pub fn github_owner_repo(&self) -> Option<String> {
        let inner = self.clone_url.strip_prefix("https://github.com/")?;
        Some(inner.trim_end_matches(".git").to_string())
    }

    #[must_use]
    pub fn is_github(&self) -> bool {
        self.clone_url.starts_with("https://github.com/")
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

/// Sub-path of the source within the cloned tree (or empty for a [`LocalSource`]).
impl SkillSource {
    #[must_use]
    pub fn sub_path(&self) -> &Path {
        match self {
            SkillSource::Git(g) => &g.sub_path,
            SkillSource::Local(_) => Path::new(""),
        }
    }
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

/// A repository (or local directory) that has been retrieved and is ready to
/// be searched for `SKILL.md` files.
pub struct FetchedRepo {
    pub tempdir: TempDir,
    /// Where to start searching: the cloned repo's working tree, or the local
    /// directory the user pointed at.
    pub root: PathBuf,
    pub commit: String,
}

/// One `SKILL.md` discovered inside a [`FetchedRepo`].
#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    /// Name from frontmatter, falling back to the directory basename.
    pub name: String,
    /// Path of the skill directory relative to [`FetchedRepo::root`]. Empty
    /// when the skill lives at the root.
    pub sub_path: PathBuf,
    /// Absolute path to the skill directory inside the working tree.
    pub abs_path: PathBuf,
}

/// Clone (or open) the source. Does not validate `SKILL.md` itself — call
/// [`FetchedRepo::skill_at_subpath`] or [`FetchedRepo::discover_all`].
///
/// # Errors
///
/// Returns [`Error::GitClone`] or an underlying I/O error.
pub async fn fetch_repo(source: &SkillSource) -> Result<FetchedRepo> {
    match source {
        SkillSource::Git(g) => fetch_repo_git(g),
        SkillSource::Local(l) => fetch_repo_local(l),
    }
}

/// Backwards-compatible single-skill fetch: clone, validate the pinpointed
/// `SKILL.md`, capture commit.
///
/// # Errors
///
/// Returns [`Error::GitClone`], [`Error::SkillMdMissing`], or an underlying
/// I/O error.
pub async fn fetch(source: &SkillSource) -> Result<FetchedSkill> {
    let repo = fetch_repo(source).await?;
    let discovered = repo.skill_at_subpath(source.sub_path())?;
    Ok(FetchedSkill {
        tempdir: repo.tempdir,
        skill_dir: discovered.abs_path,
        commit: repo.commit,
    })
}

fn fetch_repo_git(source: &GitSource) -> Result<FetchedRepo> {
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

    let commit = head_commit(&clone_dir)?;
    Ok(FetchedRepo {
        tempdir,
        root: clone_dir,
        commit,
    })
}

fn fetch_repo_local(source: &LocalSource) -> Result<FetchedRepo> {
    if !source.path.is_dir() {
        return Err(Error::InvalidSource(format!(
            "local source no longer exists: {}",
            source.path.display()
        )));
    }
    // The "commit" is derived from the latest SKILL.md mtime under `path` so
    // that `skills update` can detect edits. For collections this still works:
    // any edit to any contained SKILL.md bumps the commit.
    let commit = newest_skill_md_mtime(&source.path)?;
    let tempdir = TempDir::new()?;
    Ok(FetchedRepo {
        tempdir,
        root: source.path.clone(),
        commit,
    })
}

/// Directories the recursive walk should never descend into.
const DISCOVERY_PRUNE_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
];

impl FetchedRepo {
    /// Resolve the skill at exactly `sub` relative to [`Self::root`]. `sub`
    /// may be empty to mean "the root itself".
    ///
    /// # Errors
    ///
    /// Returns [`Error::SkillMdMissing`] if no `SKILL.md` exists at that path.
    pub fn skill_at_subpath(&self, sub: &Path) -> Result<DiscoveredSkill> {
        let abs = if sub.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.root.join(sub)
        };
        let skill_md = abs.join("SKILL.md");
        if !skill_md.is_file() {
            return Err(Error::SkillMdMissing(abs.display().to_string()));
        }
        let name = read_skill_name(&skill_md).unwrap_or_else(|| dir_basename_or(&abs, "skill"));
        Ok(DiscoveredSkill {
            name,
            sub_path: sub.to_path_buf(),
            abs_path: abs,
        })
    }

    /// Walk [`Self::root`] recursively and return every `SKILL.md` directory.
    ///
    /// Skips common build / VCS directories ([`DISCOVERY_PRUNE_DIRS`]) so
    /// large repos stay fast.
    ///
    /// # Errors
    ///
    /// Returns I/O errors from the directory walk.
    pub fn discover_all(&self) -> Result<Vec<DiscoveredSkill>> {
        let mut out = Vec::new();
        let walker = walkdir::WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_pruned_dir(e));
        for entry in walker {
            let entry = entry.map_err(io_from_walkdir)?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.file_name() != "SKILL.md" {
                continue;
            }
            let skill_md = entry.path().to_path_buf();
            let Some(dir) = skill_md.parent() else {
                continue;
            };
            let sub_path = dir
                .strip_prefix(&self.root)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let name = read_skill_name(&skill_md).unwrap_or_else(|| dir_basename_or(dir, "skill"));
            out.push(DiscoveredSkill {
                name,
                sub_path,
                abs_path: dir.to_path_buf(),
            });
        }
        // Stable order: by sub_path so output is reproducible.
        out.sort_by(|a, b| a.sub_path.cmp(&b.sub_path));
        Ok(out)
    }
}

fn is_pruned_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name();
    DISCOVERY_PRUNE_DIRS.iter().any(|p| name == *p)
}

fn io_from_walkdir(e: walkdir::Error) -> Error {
    Error::Io(
        e.into_io_error()
            .unwrap_or_else(|| std::io::Error::other("walkdir error")),
    )
}

fn dir_basename_or(dir: &Path, fallback: &str) -> String {
    dir.file_name()
        .map_or_else(|| fallback.to_string(), |n| n.to_string_lossy().to_string())
}

/// Read the `name:` field from a `SKILL.md` frontmatter block. Returns `None`
/// if the file is missing the block, the field is absent, or I/O fails.
fn read_skill_name(skill_md: &Path) -> Option<String> {
    let body = std::fs::read_to_string(skill_md).ok()?;
    let mut lines = body.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn newest_skill_md_mtime(root: &Path) -> Result<String> {
    let mut newest: u64 = 0;
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_pruned_dir(e));
    for entry in walker {
        let entry = entry.map_err(io_from_walkdir)?;
        if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
            let mtime = std::fs::metadata(entry.path())?
                .modified()
                .map_err(|e| Error::Io(std::io::Error::other(format!("mtime: {e}"))))?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| Error::Io(std::io::Error::other(format!("mtime before epoch: {e}"))))?
                .as_secs();
            if mtime > newest {
                newest = mtime;
            }
        }
    }
    Ok(format!("local-{newest}"))
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
