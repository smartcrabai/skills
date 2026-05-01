use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{config_dir, write_atomic};
use crate::error::Result;

/// Where a skill is installed: globally or in a specific project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Global,
    Project,
}

/// Whether agent dirs hold a symlink or a deep copy of the master.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    Symlink,
    Copy,
}

/// Persisted record of an installed skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub name: String,
    pub source: String,
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub commit: String,
    pub scope: Scope,
    pub project_path: Option<PathBuf>,
    pub method: Method,
    pub agents: Vec<String>,
    pub store_path: PathBuf,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// On-disk registry of every installed skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub skills: Vec<InstalledSkill>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 1,
            skills: Vec::new(),
        }
    }
}

impl Registry {
    /// Path to `skills.json` (alongside `config.json`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::ConfigError`] if the config dir cannot be resolved.
    pub fn path() -> Result<PathBuf> {
        Ok(config_dir()?.join("smartcrab-skills").join("skills.json"))
    }

    /// Load the registry, returning an empty one if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] / [`crate::error::Error::Json`] on read or parse failure.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        let r: Self = serde_json::from_str(&text)?;
        Ok(r)
    }

    /// Atomically write the registry to disk.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Io`] / [`crate::error::Error::Json`] on serialization or I/O failure.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        write_atomic(&path, &serde_json::to_vec_pretty(self)?)
    }

    /// Find a skill by name + scope + `project_path`.
    ///
    /// `project_path` should match the stored value (i.e. an absolute path
    /// for project-scoped skills, or `None` for global).
    #[must_use]
    pub fn find(
        &self,
        name: &str,
        scope: Scope,
        project_path: Option<&Path>,
    ) -> Option<&InstalledSkill> {
        self.skills.iter().find(|s| {
            s.name == name
                && s.scope == scope
                && project_path_eq(s.project_path.as_deref(), project_path)
        })
    }

    /// Mutable variant of [`Self::find`].
    pub fn find_mut(
        &mut self,
        name: &str,
        scope: Scope,
        project_path: Option<&Path>,
    ) -> Option<&mut InstalledSkill> {
        self.skills.iter_mut().find(|s| {
            s.name == name
                && s.scope == scope
                && project_path_eq(s.project_path.as_deref(), project_path)
        })
    }

    /// Append a new entry.
    pub fn add(&mut self, skill: InstalledSkill) {
        self.skills.push(skill);
    }

    /// Remove and return the matching entry, if any.
    pub fn remove(
        &mut self,
        name: &str,
        scope: Scope,
        project_path: Option<&Path>,
    ) -> Option<InstalledSkill> {
        let idx = self.skills.iter().position(|s| {
            s.name == name
                && s.scope == scope
                && project_path_eq(s.project_path.as_deref(), project_path)
        })?;
        Some(self.skills.remove(idx))
    }

    /// Iterate over skills in a given scope (and matching project root).
    pub fn iter_scope<'a>(
        &'a self,
        scope: Scope,
        project_path: Option<&'a Path>,
    ) -> impl Iterator<Item = &'a InstalledSkill> + 'a {
        self.skills.iter().filter(move |s| {
            s.scope == scope && project_path_eq(s.project_path.as_deref(), project_path)
        })
    }
}

fn project_path_eq(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_skill(name: &str) -> InstalledSkill {
        let now = Utc::now();
        InstalledSkill {
            name: name.to_string(),
            source: "owner/repo/path".to_string(),
            ref_: None,
            commit: "deadbeef".to_string(),
            scope: Scope::Global,
            project_path: None,
            method: Method::Symlink,
            agents: vec!["claude-code".to_string()],
            store_path: PathBuf::from("/tmp/store/global/foo"),
            installed_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn add_find_remove() {
        let mut r = Registry::default();
        r.add(sample_skill("foo"));
        assert!(r.find("foo", Scope::Global, None).is_some());
        let removed = r.remove("foo", Scope::Global, None);
        assert!(removed.is_some());
        assert!(r.find("foo", Scope::Global, None).is_none());
    }
}
