//! Project-local `skills-lock.json` (vercel-labs/skills upper-compatible).
//!
//! Records project-scoped installs so `skills install` can reproduce the set
//! on a fresh checkout. Top-level shape and field names match
//! `vercel-labs/skills` (`version`, `skills`, `source`, `ref`, `sourceType`),
//! plus Rust-side `commit` and `agents` to round-trip with
//! [`crate::registry::InstalledSkill`].

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::error::Result;

const LOCK_FILE_NAME: &str = "skills-lock.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, LockEntry>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub source: String,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub agents: Vec<String>,
}

impl Lockfile {
    /// Lockfile path under `project_root`.
    #[must_use]
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(LOCK_FILE_NAME)
    }

    /// Load lock at `path`. Returns [`Ok(None)`] when no file exists.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] on read failure or
    /// [`crate::error::Error::Json`] on malformed JSON.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(Some(serde_json::from_str(&text)?))
    }

    /// Atomically write the lockfile to `path`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] / [`crate::error::Error::Json`] on
    /// serialization or I/O failure.
    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// One bundle to be (re-)installed from the lockfile in a single `add` call.
///
/// Co-located skills sharing the same `(source, ref)` are bundled together
/// so we clone once and pick all of them with `--skill <name>...`.
#[derive(Debug, Clone)]
pub struct LockGroup {
    pub source: String,
    pub ref_: Option<String>,
    pub skill_names: Vec<String>,
    pub agents: Vec<String>,
}

/// Group lock entries by `(source, ref)`. Agents within a group are unioned,
/// preserving first-appearance order.
#[must_use]
pub fn group_by_source(lock: &Lockfile) -> Vec<LockGroup> {
    let mut groups: Vec<LockGroup> = Vec::new();
    for (name, entry) in &lock.skills {
        if let Some(g) = groups
            .iter_mut()
            .find(|g| g.source == entry.source && g.ref_ == entry.ref_)
        {
            g.skill_names.push(name.clone());
            for a in &entry.agents {
                if !g.agents.contains(a) {
                    g.agents.push(a.clone());
                }
            }
        } else {
            groups.push(LockGroup {
                source: entry.source.clone(),
                ref_: entry.ref_.clone(),
                skill_names: vec![name.clone()],
                agents: entry.agents.clone(),
            });
        }
    }
    groups
}

/// Merge `entries` into the lock at `lock_path`. Existing entries with the
/// same skill name are overwritten; **unrelated entries are preserved**
/// (same semantics as `vercel-labs/skills`).
///
/// # Errors
///
/// Returns [`crate::error::Error::Io`] / [`crate::error::Error::Json`] on the
/// respective failures.
pub fn merge_and_write(
    lock_path: &Path,
    entries: impl IntoIterator<Item = (String, LockEntry)>,
) -> Result<()> {
    let mut lock = Lockfile::load(lock_path)?.unwrap_or_default();
    for (name, entry) in entries {
        lock.skills.insert(name, entry);
    }
    lock.save(lock_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(source: &str, ref_: Option<&str>, commit: Option<&str>, agents: &[&str]) -> LockEntry {
        LockEntry {
            source: source.into(),
            source_type: "github".into(),
            ref_: ref_.map(str::to_string),
            commit: commit.map(str::to_string),
            agents: agents.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn round_trip_empty_lockfile() -> Result<()> {
        let lock = Lockfile::default();
        let json = serde_json::to_string(&lock)?;
        let back: Lockfile = serde_json::from_str(&json)?;
        assert_eq!(back.version, 1);
        assert!(back.skills.is_empty());
        Ok(())
    }

    #[test]
    fn merge_preserves_unrelated_entries() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("skills-lock.json");
        merge_and_write(
            &path,
            [(
                "a".to_string(),
                entry("owner/a", None, Some("aaaa"), &["claude-code"]),
            )],
        )?;
        merge_and_write(
            &path,
            [(
                "b".to_string(),
                entry("owner/b", Some("main"), Some("bbbb"), &["claude-code"]),
            )],
        )?;
        let loaded = Lockfile::load(&path)?.ok_or(io::Error::other("absent"))?;
        assert_eq!(loaded.skills.len(), 2);
        assert!(loaded.skills.contains_key("a"));
        assert!(loaded.skills.contains_key("b"));
        Ok(())
    }

    #[test]
    fn merge_overwrites_same_name() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("skills-lock.json");
        merge_and_write(
            &path,
            [(
                "a".to_string(),
                entry("owner/a", None, Some("aaaa"), &["claude-code"]),
            )],
        )?;
        merge_and_write(
            &path,
            [(
                "a".to_string(),
                entry(
                    "owner/a-new",
                    Some("v2"),
                    Some("cccc"),
                    &["claude-code", "opencode"],
                ),
            )],
        )?;
        let loaded = Lockfile::load(&path)?.ok_or(io::Error::other("absent"))?;
        let e = loaded
            .skills
            .get("a")
            .ok_or(io::Error::other("missing a"))?;
        assert_eq!(e.source, "owner/a-new");
        assert_eq!(e.ref_.as_deref(), Some("v2"));
        assert_eq!(e.agents, vec!["claude-code", "opencode"]);
        Ok(())
    }

    #[test]
    fn load_missing_returns_none() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("never-existed.json");
        assert!(Lockfile::load(&path)?.is_none());
        Ok(())
    }

    #[test]
    fn group_by_source_bundles_same_repo() -> Result<()> {
        let mut skills = BTreeMap::new();
        skills.insert(
            "alpha".to_string(),
            entry("o/r", Some("main"), None, &["claude-code"]),
        );
        skills.insert(
            "beta".to_string(),
            entry("o/r", Some("main"), None, &["opencode"]),
        );
        skills.insert(
            "gamma".to_string(),
            entry("o/other", None, None, &["claude-code"]),
        );
        let lock = Lockfile { version: 1, skills };
        let groups = group_by_source(&lock);
        assert_eq!(groups.len(), 2);
        let g1 = groups
            .iter()
            .find(|g| g.source == "o/r")
            .ok_or(io::Error::other("o/r missing"))?;
        assert_eq!(g1.skill_names, vec!["alpha", "beta"]);
        assert_eq!(g1.agents, vec!["claude-code", "opencode"]);
        let g2 = groups
            .iter()
            .find(|g| g.source == "o/other")
            .ok_or(io::Error::other("o/other missing"))?;
        assert_eq!(g2.skill_names, vec!["gamma"]);
        Ok(())
    }
}
